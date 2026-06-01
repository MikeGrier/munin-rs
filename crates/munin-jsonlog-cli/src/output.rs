// Copyright (c) Michael Grier

//! Output sink abstraction.
//!
//! Per repo convention: the CLI must route **all** output through a single
//! writer trait rather than calling `println!` / `eprintln!` from scattered
//! sites. Tests substitute an in-memory sink; the production binary uses
//! [`StdSink`].

use std::io::{self, Write};

/// A two-stream output sink (normal output + diagnostics).
pub trait OutputSink {
    /// Writer for the primary output stream (`stdout` equivalent).
    fn out(&mut self) -> &mut dyn Write;
    /// Writer for the diagnostic stream (`stderr` equivalent).
    fn err(&mut self) -> &mut dyn Write;
}

/// Production sink that writes to the real `stdout` / `stderr`.
pub struct StdSink {
    out: io::Stdout,
    err: io::Stderr,
}

impl StdSink {
    pub fn new() -> Self {
        Self {
            out: io::stdout(),
            err: io::stderr(),
        }
    }
}

impl Default for StdSink {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputSink for StdSink {
    fn out(&mut self) -> &mut dyn Write {
        &mut self.out
    }
    fn err(&mut self) -> &mut dyn Write {
        &mut self.err
    }
}

/// In-memory sink for tests.
#[derive(Default)]
pub struct VecSink {
    pub out: Vec<u8>,
    pub err: Vec<u8>,
}

impl VecSink {
    pub fn new() -> Self {
        Self::default()
    }
}

impl OutputSink for VecSink {
    fn out(&mut self) -> &mut dyn Write {
        &mut self.out
    }
    fn err(&mut self) -> &mut dyn Write {
        &mut self.err
    }
}
