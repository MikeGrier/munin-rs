// Copyright (c) Michael Grier

//! Munin C++ build analysis — derive structured per-project data from
//! MSBuild `.binlog` files produced by C++ (`.vcxproj`) builds.
//!
//! Given a `.binlog` captured with `cl.exe /showIncludes` and
//! `link.exe /VERBOSE` enabled, this crate produces a JSON document
//! describing, for each project invocation:
//!
//! - Project metadata (path, platform, configuration, MSBuild
//!   command-line / global properties).
//! - For each compiled translation unit: the command line, include
//!   search paths, preprocessor defines, the include tree resolved by
//!   the compiler, and a flat list of every header consumed.
//! - For each linked output: the command line, the inputs the linker
//!   actually consumed, and the inputs it dropped or did not reference.
//!
//! See [`DESIGN-NOTES.md`] for the full schema (D-CPPSCHEMA-1) and the
//! locked decisions (D-CPP-1..8).
//!
//! [`DESIGN-NOTES.md`]: https://github.com/MikeGrier/munin-rs/blob/main/crates/munin-cppbuild/DESIGN-NOTES.md

// Modules are added in subsequent CHECKLIST items:
// - CPP-1.2 schema
// - CPP-1.3 path_root
// - CPP-1.4 alias

pub mod alias;
pub mod emit;
pub mod path_root;
pub mod schema;
pub mod walk;

pub use alias::AliasTable;
pub use emit::project_from_invocation;
pub use path_root::to_rooted;
pub use schema::{
    CppBuildAnalysis, Define, DroppedInput, GlobalProperty, Header, IncludeNode, LinkInput,
    LinkInputKind, LinkInputOrigin, Output, Project, PropertySource, Root, RootedPath,
    SCHEMA_VERSION, Source,
};
pub use walk::{ProjectInvocation, walk_projects};
