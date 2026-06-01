// Copyright (c) Michael Grier

//! Translate [`crate::cli::RedactArgs`] into a configured
//! [`munin_msbuild::redact::Redactor`].

use munin_msbuild::redact::Redactor;

use crate::cli::RedactArgs;

/// Build a [`Redactor`] from parsed CLI args.
///
/// Returns an error string if a `--redact-regex` value is malformed.
pub fn build_redactor(args: &RedactArgs) -> Result<Redactor, String> {
    let mut r = Redactor::new();
    for tok in &args.redact_token {
        r = r.with_token(tok.clone());
    }
    for spec in &args.redact_regex {
        let (pat, repl) = spec
            .split_once("=>")
            .ok_or_else(|| format!("--redact-regex value missing '=>' separator: {spec:?}"))?;
        r = r
            .with_regex(pat, repl)
            .map_err(|e| format!("--redact-regex {spec:?}: {e}"))?;
    }
    if args.redact_common {
        r = r.with_common_patterns();
    }
    if args.redact_username {
        r = r.with_autodetect_username();
    }
    Ok(r)
}

/// True if any redaction flag was provided.
pub fn is_active(args: &RedactArgs) -> bool {
    !args.redact_token.is_empty()
        || !args.redact_regex.is_empty()
        || args.redact_common
        || args.redact_username
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(tokens: &[&str], regexes: &[&str], username: bool, common: bool) -> RedactArgs {
        RedactArgs {
            redact_token: tokens.iter().map(|s| s.to_string()).collect(),
            redact_regex: regexes.iter().map(|s| s.to_string()).collect(),
            redact_username: username,
            redact_common: common,
        }
    }

    #[test]
    fn empty_args_yield_inactive() {
        let a = args(&[], &[], false, false);
        assert!(!is_active(&a));
        build_redactor(&a).expect("ok");
    }

    #[test]
    fn token_and_common_are_active() {
        let a = args(&["secret"], &[], false, true);
        assert!(is_active(&a));
        build_redactor(&a).expect("ok");
    }

    #[test]
    fn regex_separator_must_be_present() {
        let a = args(&[], &["no separator here"], false, false);
        let err = build_redactor(&a).unwrap_err();
        assert!(err.contains("=>"));
    }

    #[test]
    fn regex_invalid_pattern_is_reported() {
        let a = args(&[], &["(unbalanced=>x"], false, false);
        let err = build_redactor(&a).unwrap_err();
        assert!(err.contains("--redact-regex"));
    }

    #[test]
    fn regex_only_first_arrow_is_the_separator() {
        // Replacement may itself contain "=>".
        let a = args(&[], &["foo=>bar=>baz"], false, false);
        build_redactor(&a).expect("ok");
    }
}
