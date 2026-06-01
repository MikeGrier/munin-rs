// Copyright (c) Michael Grier

//! `BuildEventContext` — seven fields identifying the execution context of a
//! build event (node, project, target, task, submission, project instance,
//! and evaluation).

use std::io::Read;

use crate::{error::MuninError, primitives::read_7bit_int};

/// Sentinel value used when an evaluation ID is not available (format version ≤ 1).
const INVALID_EVALUATION_ID: i32 = -1;

/// Execution context attached to a build event.
///
/// All seven fields are 7-bit variable-length encoded `i32` values. The
/// `evaluation_id` field was introduced in format version 2; for earlier
/// versions it defaults to [`INVALID_EVALUATION_ID`] (-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildEventContext {
    pub node_id: i32,
    pub project_context_id: i32,
    pub target_id: i32,
    pub task_id: i32,
    pub submission_id: i32,
    pub project_instance_id: i32,
    pub evaluation_id: i32,
}

/// Read a `BuildEventContext` from the stream.
///
/// The `file_format_version` controls whether the `evaluation_id` field is
/// present (version > 1) or defaults to -1.
pub fn read_build_event_context(
    reader: &mut impl Read,
    file_format_version: i32,
) -> Result<BuildEventContext, MuninError> {
    let node_id = read_7bit_int(reader)?;
    let project_context_id = read_7bit_int(reader)?;
    let target_id = read_7bit_int(reader)?;
    let task_id = read_7bit_int(reader)?;
    let submission_id = read_7bit_int(reader)?;
    let project_instance_id = read_7bit_int(reader)?;
    let evaluation_id = if file_format_version > 1 {
        read_7bit_int(reader)?
    } else {
        INVALID_EVALUATION_ID
    };
    Ok(BuildEventContext {
        node_id,
        project_context_id,
        target_id,
        task_id,
        submission_id,
        project_instance_id,
        evaluation_id,
    })
}

#[cfg(test)]
mod tests;
