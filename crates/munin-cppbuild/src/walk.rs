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

use crate::schema::{GlobalProperty, PropertySource};

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

    /// Convert `global_properties` to schema [`GlobalProperty`]
    /// entries. Every entry is tagged with
    /// [`PropertySource::CommandLine`]; see DESIGN-NOTES.md
    /// §D-CPP-PROPSRC1 for why `Project` is not yet emitted.
    pub fn to_global_properties(&self) -> Vec<GlobalProperty> {
        self.global_properties
            .iter()
            .map(|(name, value)| GlobalProperty {
                name: name.clone(),
                value: value.clone(),
                source: PropertySource::CommandLine,
            })
            .collect()
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

/// Name of the MSBuild task that drives `cl.exe`.
///
/// The C++ `ClCompile` *target* invokes a *task* literally named
/// `CL`. Real-binlog validation (CPP-4.1 spike) will confirm whether
/// any other names appear in practice.
pub const CL_TASK_NAME: &str = "CL";

/// One `cl.exe` invocation captured from the binlog stream.
///
/// Each TaskStarted/TaskFinished pair whose `task_name` equals
/// [`CL_TASK_NAME`] under an active project produces one
/// `CompileInvocation`. `command_line` is taken from the
/// `TaskCommandLine` event recorded inside the bracket; `messages`
/// is the raw text of every `Message` event recorded inside the
/// bracket, in stream order. The `/showIncludes` parser (CPP-3.3)
/// consumes `messages`.
#[derive(Debug, Clone)]
pub struct CompileInvocation {
    /// `BuildEventContext::project_context_id` of the enclosing
    /// project. Used to attach this CL invocation to a
    /// [`ProjectInvocation`] downstream.
    pub project_context_id: i32,

    /// Verbatim `cl.exe` command line from the `TaskCommandLine`
    /// event, or `None` if MSBuild did not emit one inside the
    /// task bracket.
    pub command_line: Option<String>,

    /// Raw text of every `Message` event recorded inside the
    /// CL task bracket, in stream order. Includes `/showIncludes`
    /// output, the source-file echo line, and any other compiler
    /// diagnostics MSBuild forwarded.
    pub messages: Vec<String>,
}

/// Walk every CL-task invocation in `index`, in stream order.
///
/// Bracketing rules:
///
/// - A project is "open" between a `ProjectStarted` event and the
///   matching `ProjectFinished` event (matched by
///   `BuildEventContext::project_context_id`). Nested projects
///   stack: the innermost open project is the parent of any
///   subsequent task.
/// - A CL task is "open" between a `TaskStarted` with
///   `task_name == "CL"` and its matching `TaskFinished`. Tasks
///   are assumed to be non-nested within their parent project
///   (MSBuild does not nest tasks).
/// - While a CL task is open, the first `TaskCommandLine` event
///   inside the bracket populates `command_line`; every `Message`
///   event inside the bracket is appended to `messages`.
/// - When the CL task closes, the captured `CompileInvocation` is
///   emitted with the enclosing project's context id.
pub fn walk_cl_tasks(index: &BinlogIndex) -> Result<Vec<CompileInvocation>, MuninError> {
    walk_named_tasks(
        index,
        CL_TASK_NAME,
        |project_context_id, command_line, messages| CompileInvocation {
            project_context_id,
            command_line,
            messages,
        },
    )
}

/// Name of the MSBuild task that drives `link.exe`.
pub const LINK_TASK_NAME: &str = "Link";

/// One `link.exe` invocation captured from the binlog stream.
///
/// Each TaskStarted/TaskFinished pair whose `task_name` equals
/// [`LINK_TASK_NAME`] under an active project produces one
/// `LinkInvocation`. Field semantics mirror [`CompileInvocation`].
/// The [`crate::link_cmdline`] tokenizer consumes `command_line`;
/// the [`crate::link_verbose`] parser consumes `messages`.
#[derive(Debug, Clone)]
pub struct LinkInvocation {
    /// `BuildEventContext::project_context_id` of the enclosing
    /// project.
    pub project_context_id: i32,
    /// Verbatim `link.exe` command line from the `TaskCommandLine`
    /// event, or `None` if MSBuild did not emit one inside the
    /// task bracket.
    pub command_line: Option<String>,
    /// Raw text of every `Message` event recorded inside the
    /// Link task bracket, in stream order. With `/VERBOSE` enabled
    /// this includes `Searching libraries`, `Found`, `Loaded`,
    /// `Unused libraries:`, and other per-line linker diagnostics.
    pub messages: Vec<String>,
}

/// Walk every Link-task invocation in `index`, in stream order.
///
/// Bracketing rules mirror [`walk_cl_tasks`], with `task_name ==
/// "Link"`. The `/VERBOSE` output classified by D-CPP-LINK1 lives
/// in `LinkInvocation::messages`.
pub fn walk_link_tasks(index: &BinlogIndex) -> Result<Vec<LinkInvocation>, MuninError> {
    walk_named_tasks(
        index,
        LINK_TASK_NAME,
        |project_context_id, command_line, messages| LinkInvocation {
            project_context_id,
            command_line,
            messages,
        },
    )
}

fn walk_named_tasks<T, F>(
    index: &BinlogIndex,
    task_name: &str,
    mut build: F,
) -> Result<Vec<T>, MuninError>
where
    F: FnMut(i32, Option<String>, Vec<String>) -> T,
{
    struct Cur {
        project_context_id: i32,
        command_line: Option<String>,
        messages: Vec<String>,
    }

    let mut out: Vec<T> = Vec::new();
    let mut project_stack: Vec<i32> = Vec::new();
    let mut current: Option<Cur> = None;

    for (i, _meta) in index.iter_meta() {
        let event = match index.get(i)? {
            Some(ev) => ev,
            None => continue,
        };

        match event {
            BinlogEvent::ProjectStarted(ev) => {
                let id = ev
                    .fields
                    .build_event_context
                    .as_ref()
                    .map(|ctx| ctx.project_context_id)
                    .unwrap_or(0);
                project_stack.push(id);
            }
            BinlogEvent::ProjectFinished(_) => {
                project_stack.pop();
            }
            BinlogEvent::TaskStarted(ev)
                if current.is_none() && ev.task_name.as_deref() == Some(task_name) =>
            {
                let project_context_id = project_stack.last().copied().unwrap_or(0);
                current = Some(Cur {
                    project_context_id,
                    command_line: None,
                    messages: Vec::new(),
                });
            }
            BinlogEvent::TaskFinished(ev)
                if ev.task_name.as_deref() == Some(task_name) && current.is_some() =>
            {
                if let Some(c) = current.take() {
                    out.push(build(c.project_context_id, c.command_line, c.messages));
                }
            }
            BinlogEvent::TaskCommandLine(ev) => {
                if let Some(c) = current.as_mut()
                    && c.command_line.is_none()
                {
                    c.command_line = ev.command_line.clone();
                }
            }
            BinlogEvent::Message(ev) => {
                if let Some(c) = current.as_mut()
                    && let Some(msg) = &ev.fields.message
                {
                    c.messages.push(msg.clone());
                }
            }
            _ => {}
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests;
