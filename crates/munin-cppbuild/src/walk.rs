// Copyright (c) Michael Grier

//! Walk a `BinlogIndex` and extract per-project metadata.
//!
//! See `DESIGN-NOTES.md` §D-CPP-WALK1 for the bracketing and metadata
//! extraction rules.
//!
//! This is the intermediate representation between
//! `munin_msbuild::BinlogIndex` and the `schema::Project` JSON
//! emission. M2 produces one [`ProjectInvocation`] per `ProjectStarted`
//! event observed; later milestones attach sources and outputs to
//! these invocations.

use munin_msbuild::{
    BinaryLogRecordKind, BinlogEvent, BinlogIndex, MuninError, events::ProjectStartedEvent,
};

/// Per-project-invocation metadata extracted from a binlog.
///
/// Each `ProjectStarted` event in the binlog produces one
/// `ProjectInvocation`. Multiple invocations of the same `.vcxproj`
/// for different platform/configuration combos appear as distinct
/// entries.
#[derive(Debug, Clone)]
pub struct ProjectInvocation {
    /// `BuildEventContext::project_context_id` of the `ProjectStarted`
    /// event. Used to associate later events (CL, Link) with this
    /// invocation.
    pub project_context_id: i32,

    /// Absolute path to the project file as reported by MSBuild, or
    /// `None` if the event omitted it.
    pub project_file: Option<String>,

    /// Global properties set on this project invocation (typically
    /// includes `Configuration`, `Platform`, and any other `/p:`
    /// values). The binlog does not record which of these came from
    /// the MSBuild command line versus an API caller versus
    /// inheritance; see D-CPP-PROPSRC1.
    pub global_properties: Vec<(String, String)>,

    /// Full evaluated property list at project-start. May contain
    /// hundreds of entries and overlaps with `global_properties`.
    /// Retained so callers can look up properties that are not
    /// surfaced as globals (e.g. `Configuration` / `Platform` when
    /// they were declared in the project file rather than passed via
    /// `/p:`).
    pub property_list: Vec<(String, String)>,
}

impl ProjectInvocation {
    /// Look up the value of `Configuration`, preferring globals over
    /// the broader property list.
    pub fn configuration(&self) -> Option<&str> {
        self.lookup("Configuration")
    }

    /// Look up the value of `Platform`, preferring globals over the
    /// broader property list.
    pub fn platform(&self) -> Option<&str> {
        self.lookup("Platform")
    }

    fn lookup(&self, name: &str) -> Option<&str> {
        for (n, v) in &self.global_properties {
            if n.eq_ignore_ascii_case(name) {
                return Some(v.as_str());
            }
        }
        for (n, v) in &self.property_list {
            if n.eq_ignore_ascii_case(name) {
                return Some(v.as_str());
            }
        }
        None
    }
}

/// Walk every `ProjectStarted` record in `index` and produce one
/// [`ProjectInvocation`] per event, in the order they appear in the
/// binlog stream.
pub fn walk_projects(index: &BinlogIndex) -> Result<Vec<ProjectInvocation>, MuninError> {
    let mut out = Vec::new();

    for idx in index.indices_by_kind(BinaryLogRecordKind::ProjectStarted) {
        let event = match index.get(idx)? {
            Some(BinlogEvent::ProjectStarted(ev)) => ev,
            // The kind filter guarantees this is a ProjectStarted;
            // anything else would be an internal munin-msbuild bug.
            Some(other) => {
                return Err(MuninError::InvalidFormat(format!(
                    "index returned non-ProjectStarted event for kind ProjectStarted: \
                     got {:?}",
                    other.record_kind()
                )));
            }
            None => continue,
        };

        out.push(make_invocation(&event));
    }

    Ok(out)
}

fn make_invocation(event: &ProjectStartedEvent) -> ProjectInvocation {
    let project_context_id = event
        .fields
        .build_event_context
        .as_ref()
        .map(|ctx| ctx.project_context_id)
        .unwrap_or(0);

    let global_properties = event.global_properties.clone().unwrap_or_default();
    let property_list = event.property_list.clone().unwrap_or_default();

    ProjectInvocation {
        project_context_id,
        project_file: event.project_file.clone(),
        global_properties,
        property_list,
    }
}

#[cfg(test)]
mod tests;
