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

use crate::{error::MuninError, index::BinlogIndex};

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
    /// Whether to install the built-in common-pattern regex set.
    /// Implemented in JL-3.5-R2.
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
        let mut exact_order: Vec<&ExactRule> = self.exact.iter().collect();
        exact_order.sort_by_key(|r| std::cmp::Reverse(r.needle.len()));

        // Suppress unused-field warnings until R2/R3 wire these in.
        let _ = self.autodetect_username;
        let _ = self.common_patterns;

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

            for rule in &self.regex {
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

#[cfg(test)]
mod tests;
