// Copyright (c) Michael Grier

//! `munin-jsonlog` CLI — convert between MSBuild `.binlog` and munin's
//! jsonlog format, with optional redaction.

use clap::{Parser, Subcommand};

/// `munin-jsonlog` — convert between MSBuild binary log and munin jsonlog.
#[derive(Debug, Parser)]
#[command(name = "munin-jsonlog", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Read a `.binlog`, optionally redact, and write a `.jsonlog`.
    Dump(DumpArgs),
    /// Read a `.jsonlog`, optionally redact, and write a `.binlog`.
    Pack(PackArgs),
}

/// Shared redaction flags for `dump` and `pack`.
#[derive(Debug, clap::Args)]
pub struct RedactArgs {
    /// Add a literal token to scrub (repeatable). The match is replaced
    /// with `*****`.
    #[arg(long = "redact-token", value_name = "VAL")]
    pub redact_token: Vec<String>,

    /// Add a regex rule `PAT=>REPL` (repeatable). The literal sequence
    /// `=>` separates pattern from replacement; the pattern itself
    /// therefore cannot contain `=>`.
    #[arg(long = "redact-regex", value_name = "PAT=>REPL")]
    pub redact_regex: Vec<String>,

    /// Enable username autodetect (see `Redactor::with_autodetect_username`).
    #[arg(long = "redact-username")]
    pub redact_username: bool,

    /// Enable munin's built-in common-pattern catalog (see D-RDX-1).
    #[arg(long = "redact-common")]
    pub redact_common: bool,
}

/// `dump` subcommand: `.binlog` → `.jsonlog`.
#[derive(Debug, clap::Args)]
pub struct DumpArgs {
    /// Path to the input `.binlog`.
    pub input: std::path::PathBuf,

    /// Output path. Defaults to stdout when omitted.
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    pub output: Option<std::path::PathBuf>,

    /// Pretty-print the jsonlog output.
    #[arg(long)]
    pub pretty: bool,

    #[command(flatten)]
    pub redact: RedactArgs,
}

/// `pack` subcommand: `.jsonlog` → `.binlog`.
#[derive(Debug, clap::Args)]
pub struct PackArgs {
    /// Path to the input `.jsonlog`.
    pub input: std::path::PathBuf,

    /// Output path. Defaults to stdout when omitted.
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    pub output: Option<std::path::PathBuf>,

    #[command(flatten)]
    pub redact: RedactArgs,
}
