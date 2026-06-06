// Copyright (c) Michael Grier

//! Shared fixtures for `munin-cppbuild` integration tests.
//!
//! Thin re-export of the public [`munin_cppbuild::testkit`] module so
//! both this crate's integration tests and downstream consumers
//! (e.g. `munin-jsonlog-cli`) build off the same synthesis helpers.

#![allow(unused_imports)]

pub use munin_cppbuild::testkit::{
    FIXTURE_PROJECT_PATH, synthetic_cl_link_binlog, synthetic_cl_task_binlog,
    synthetic_vcxproj_binlog,
};
