// Copyright (c) Michael Grier

//! Entry point for the `munin-jsonlog` binary.

use std::process::ExitCode;

use clap::Parser;
use munin_jsonlog_cli::{cli::Cli, output::StdSink, run};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut sink = StdSink::new();
    match run::dispatch(cli, &mut sink) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Errors always go to stderr regardless of the sink, since
            // the sink may itself have failed.
            eprintln!("munin-jsonlog: {e}");
            ExitCode::FAILURE
        }
    }
}
