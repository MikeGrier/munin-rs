// Copyright (c) Michael Grier

//! Redaction of sensitive strings from a [`BinlogIndex`].
//!
//! See `D-RDX-1` in the crate `DESIGN-NOTES.md`. Functional intent is parity
//! with `binlogtool redact` (MIT-licensed reference; engine source not
//! published). Munin defines its own set of "common patterns" rather than
//! consuming or deriving from any closed-source catalog.
//!
//! The redactor rewrites entries of [`BinlogIndex::strings_mut`] in place.
//! Because every event payload references strings by index, scrubbing the
//! string table propagates to every event without touching payload bytes.

use regex::Regex;

use crate::{
    error::MuninError, index::BinlogIndex, reader::BinlogEvent, record_kind::BinaryLogRecordKind,
};

/// A configured set of redaction rules.
///
/// Build via [`Redactor::new`] and a chain of `with_*` methods, then call
/// [`Redactor::apply`] on a mutable [`BinlogIndex`].
#[derive(Debug, Default)]
pub struct Redactor {
    /// Literal-string rules (substring replace, case-sensitive).
    exact: Vec<ExactRule>,
    /// Regex rules, applied in insertion order after exact rules.
    regex: Vec<RegexRule>,
    /// Whether to walk `BuildStarted` env on [`apply`] to derive username
    /// rules. Implemented in JL-3.5-R3.
    autodetect_username: bool,
    /// Whether to install the built-in common-pattern regex set on `apply`.
    common_patterns: bool,
}

#[derive(Debug)]
struct ExactRule {
    needle: String,
    replacement: String,
}

#[derive(Debug)]
struct RegexRule {
    re: Regex,
    replacement: String,
}

impl Redactor {
    /// An empty redactor — calling [`apply`](Self::apply) is a no-op.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace every occurrence of `value` in every string with
    /// `"*****"`. Equivalent to `binlogtool redact -p:<value>`.
    pub fn with_token(mut self, value: impl Into<String>) -> Self {
        self.exact.push(ExactRule {
            needle: value.into(),
            replacement: "*****".to_string(),
        });
        self
    }

    /// Replace every match of `pattern` with `replacement`. The
    /// `replacement` is interpreted using the [`regex`] crate's
    /// `$0`/`$1`/`${name}` capture-reference syntax.
    pub fn with_regex(
        mut self,
        pattern: &str,
        replacement: impl Into<String>,
    ) -> Result<Self, MuninError> {
        let re = Regex::new(pattern)
            .map_err(|e| MuninError::InvalidFormat(format!("invalid redaction regex: {e}")))?;
        self.regex.push(RegexRule {
            re,
            replacement: replacement.into(),
        });
        Ok(self)
    }

    /// Install munin's built-in set of common sensitive-data patterns.
    ///
    /// See `D-RDX-1` in the crate `DESIGN-NOTES.md` for the exact pattern
    /// list, replacement strings, and rationale. The patterns are appended
    /// to the regex rule list on [`apply`](Self::apply), after any
    /// caller-supplied exact-token rules and before any caller-supplied
    /// regex rules added *after* this call. Calling more than once is a
    /// no-op beyond the first.
    pub fn with_common_patterns(mut self) -> Self {
        self.common_patterns = true;
        self
    }

    /// On [`apply`](Self::apply), inspect the index's `BuildStarted`
    /// event for a captured environment dictionary; for every non-empty
    /// value under `USERNAME`, `USER`, `USERPROFILE`, or `HOME`,
    /// register an exact-token rule mapping that value (and its
    /// appearance inside the standard user-home path prefixes
    /// `C:\\Users\\<u>\\`, `/home/<u>/`, `/Users/<u>/`) to
    /// `REDACTED-USER`.
    ///
    /// If the `BuildStarted` event has no environment dictionary (the
    /// build was logged without environment capture), this is a no-op.
    /// We deliberately do **not** fall back to the host process's
    /// current username — that would leak the redactor's environment
    /// into someone else's binlog.
    pub fn with_autodetect_username(mut self) -> Self {
        self.autodetect_username = true;
        self
    }

    /// Apply all configured rules to `index`'s string table, in place.
    ///
    /// Order:
    ///
    /// 1. Exact-token rules, longest-needle-first so that a short needle
    ///    cannot pre-empt a longer one containing it as a substring.
    /// 2. Regex rules, in the order they were added.
    pub fn apply(&self, index: &mut BinlogIndex) {
        // Sort exact rules by descending needle length without mutating
        // self.
        let mut exact_owned: Vec<ExactRule> = Vec::new();
        if self.autodetect_username {
            exact_owned.extend(autodetect_username_rules(index));
        }
        let mut exact_order: Vec<&ExactRule> =
            self.exact.iter().chain(exact_owned.iter()).collect();
        exact_order.sort_by_key(|r| std::cmp::Reverse(r.needle.len()));

        // Built-in common-pattern rules (D-RDX-1) are installed once per
        // `apply` call, in the documented order, *between* the caller's
        // exact-token rules and the caller's regex rules. The caller's
        // regex rules then follow.
        let common: Vec<RegexRule> = if self.common_patterns {
            common_patterns()
        } else {
            Vec::new()
        };

        for entry in index.strings_mut().entries_mut() {
            // Skip empty strings — no rule can match.
            if entry.is_empty() {
                continue;
            }

            for rule in &exact_order {
                if rule.needle.is_empty() {
                    continue;
                }
                if entry.contains(&rule.needle) {
                    *entry = entry.replace(&rule.needle, &rule.replacement);
                }
            }

            for rule in common.iter().chain(self.regex.iter()) {
                // Cow::Borrowed when no match, so this is cheap on the
                // common no-match path.
                let replaced = rule.re.replace_all(entry, rule.replacement.as_str());
                if let std::borrow::Cow::Owned(s) = replaced {
                    *entry = s;
                }
            }
        }
    }
}

/// Build the D-RDX-1 common-pattern rule list.
///
/// All patterns are compiled here so an invalid pattern in the catalog is
/// caught by unit tests (`common_patterns_all_compile`).
fn common_patterns() -> Vec<RegexRule> {
    fn r(pat: &str, repl: &str) -> RegexRule {
        RegexRule {
            re: Regex::new(pat).expect("common-pattern regex must compile (see D-RDX-1)"),
            replacement: repl.to_string(),
        }
    }
    vec![
        // 1. URL with embedded credentials.
        r(
            r"(?P<scheme>[a-zA-Z][a-zA-Z0-9+.\-]*)://[^/\s:@]+:[^/\s:@]+@",
            "$scheme://*****:*****@",
        ),
        // 2. GitHub PAT.
        r(r"gh[pousr]_[A-Za-z0-9]{36,}", "gh*_*****"),
        // 3. Azure DevOps PAT (lowercase base32, 52 chars).
        r(r"\b[a-z2-7]{52}\b", "*****"),
        // 4. HTTP Bearer header.
        r(
            r"(?i)(?P<prefix>bearer\s+)[A-Za-z0-9._\-]+",
            "${prefix}*****",
        ),
        // 5. Email address.
        r(
            r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}",
            "*****@*****",
        ),
    ]
}

/// Scan `index` for a `BuildStarted` event with a captured environment
/// dictionary and synthesize exact-token rules for the user identity
/// values it contains.
///
/// Returns an empty vec when no `BuildStarted` event is present, when
/// the event has no environment dictionary, or when none of the
/// recognized variables hold a non-empty value.
fn autodetect_username_rules(index: &BinlogIndex) -> Vec<ExactRule> {
    const REPLACEMENT: &str = "REDACTED-USER";
    const KEYS: &[&str] = &["USERNAME", "USER", "USERPROFILE", "HOME"];

    let mut out = Vec::new();

    for i in index.indices_by_kind(BinaryLogRecordKind::BuildStarted) {
        let Ok(Some(BinlogEvent::BuildStarted(ev))) = index.get(i) else {
            continue;
        };
        let Some(env) = ev.environment.as_ref() else {
            continue;
        };
        for (k, v) in env {
            if v.is_empty() {
                continue;
            }
            if !KEYS.iter().any(|w| k.eq_ignore_ascii_case(w)) {
                continue;
            }
            let leaf = path_basename(v);
            if leaf.is_empty() {
                continue;
            }
            out.push(ExactRule {
                needle: leaf.to_string(),
                replacement: REPLACEMENT.to_string(),
            });
            for prefix in [
                format!(r"C:\Users\{leaf}\"),
                format!("/home/{leaf}/"),
                format!("/Users/{leaf}/"),
            ] {
                let scrubbed = prefix.replace(leaf, REPLACEMENT);
                out.push(ExactRule {
                    needle: prefix,
                    replacement: scrubbed,
                });
            }
        }
    }

    out.sort_by(|a, b| a.needle.cmp(&b.needle));
    out.dedup_by(|a, b| a.needle == b.needle);
    out
}

/// Return the trailing path component of `s` (after the last `/` or
/// `\`), or `s` itself if no separator is present.
fn path_basename(s: &str) -> &str {
    match s.rfind(['/', '\\']) {
        Some(i) => &s[i + 1..],
        None => s,
    }
}

#[cfg(test)]
mod tests;
