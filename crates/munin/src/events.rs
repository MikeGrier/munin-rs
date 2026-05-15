// Copyright (c) Michael Grier

//! Typed event structs and reader functions for MSBuild binary log events.
//!
//! Each event type has:
//! - A struct holding the decoded fields.
//! - A `read_*` function that deserializes the event from a stream, given
//!   the common `BuildEventArgsFields` prefix and the string/NVL tables.
//!
//! The deserialization order and field presence logic matches the C# reference
//! implementation in `BuildEventArgsReader.cs`. Changing the read order or
//! field semantics is a breaking change.

use std::io::Read;

use crate::{
    context::{read_build_event_context, BuildEventContext},
    error::MuninError,
    fields::{read_build_event_args_fields, BuildEventArgsFields},
    nvl_table::NameValueListTable,
    primitives::{read_7bit_int, read_7bit_length, read_bool},
    readers::{read_dedup_string, read_optional_string, read_string_dictionary},
    string_table::StringTable,
};

// ---------------------------------------------------------------------------
// Helper: read diagnostic fields (Error, Warning, CriticalBuildMessage, Message)
// ---------------------------------------------------------------------------

/// Read the 8 diagnostic location fields that are always present (not flag-driven)
/// for Error, Warning, and CriticalBuildMessage events.
fn read_diagnostic_fields(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<DiagnosticLocation, MuninError> {
    let subcategory = read_optional_string(reader, strings, file_format_version)?;
    let code = read_optional_string(reader, strings, file_format_version)?;
    let file = read_optional_string(reader, strings, file_format_version)?;
    let project_file = read_optional_string(reader, strings, file_format_version)?;
    let line_number = read_7bit_int(reader)?;
    let column_number = read_7bit_int(reader)?;
    let end_line_number = read_7bit_int(reader)?;
    let end_column_number = read_7bit_int(reader)?;
    Ok(DiagnosticLocation {
        subcategory,
        code,
        file,
        project_file,
        line_number,
        column_number,
        end_line_number,
        end_column_number,
    })
}

/// Location fields for diagnostic events (Error, Warning, CriticalBuildMessage).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticLocation {
    pub subcategory: Option<String>,
    pub code: Option<String>,
    pub file: Option<String>,
    pub project_file: Option<String>,
    pub line_number: i32,
    pub column_number: i32,
    pub end_line_number: i32,
    pub end_column_number: i32,
}

// ---------------------------------------------------------------------------
// MN-11: Build/Project lifecycle events
// ---------------------------------------------------------------------------

/// `BuildStarted` event — emitted once at the start of the build.
#[derive(Debug, Clone)]
pub struct BuildStartedEvent {
    pub fields: BuildEventArgsFields,
    /// Environment variables at build start (key → value).
    pub environment: Option<Vec<(String, String)>>,
}

/// Read a `BuildStarted` event from the stream.
pub fn read_build_started(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<BuildStartedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let environment = read_string_dictionary(reader, strings, nvl_table, file_format_version)?;
    Ok(BuildStartedEvent {
        fields,
        environment,
    })
}

/// `BuildFinished` event — emitted once at the end of the build.
#[derive(Debug, Clone)]
pub struct BuildFinishedEvent {
    pub fields: BuildEventArgsFields,
    pub succeeded: bool,
}

/// Read a `BuildFinished` event from the stream.
pub fn read_build_finished(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<BuildFinishedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let succeeded = read_bool(reader)?;
    Ok(BuildFinishedEvent { fields, succeeded })
}

/// `ProjectStarted` event.
#[derive(Debug, Clone)]
pub struct ProjectStartedEvent {
    pub fields: BuildEventArgsFields,
    pub parent_context: Option<BuildEventContext>,
    pub project_file: Option<String>,
    pub project_id: i32,
    pub target_names: Option<String>,
    pub tools_version: Option<String>,
    pub global_properties: Option<Vec<(String, String)>>,
    pub property_list: Option<Vec<(String, String)>>,
    pub item_list: Option<Vec<ItemGroup>>,
}

/// Read a `ProjectStarted` event from the stream.
pub fn read_project_started(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<ProjectStartedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;

    let parent_context = if read_bool(reader)? {
        Some(read_build_event_context(reader, file_format_version)?)
    } else {
        None
    };

    let project_file = read_optional_string(reader, strings, file_format_version)?;
    let project_id = read_7bit_int(reader)?;
    let target_names = read_dedup_string(reader, strings)?;
    let tools_version = read_optional_string(reader, strings, file_format_version)?;

    let global_properties = if file_format_version > 6 {
        // In v18+ global properties are always written. In v7-17 a bool prefix
        // controls presence.
        if file_format_version >= 18 || read_bool(reader)? {
            read_string_dictionary(reader, strings, nvl_table, file_format_version)?
        } else {
            None
        }
    } else {
        None
    };

    let property_list = read_property_list(reader, strings, nvl_table, file_format_version)?;
    let item_list = read_project_items(reader, strings, nvl_table, file_format_version)?;

    Ok(ProjectStartedEvent {
        fields,
        parent_context,
        project_file,
        project_id,
        target_names,
        tools_version,
        global_properties,
        property_list,
        item_list,
    })
}

/// `ProjectFinished` event.
#[derive(Debug, Clone)]
pub struct ProjectFinishedEvent {
    pub fields: BuildEventArgsFields,
    pub project_file: Option<String>,
    pub succeeded: bool,
}

/// Read a `ProjectFinished` event from the stream.
pub fn read_project_finished(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<ProjectFinishedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let project_file = read_optional_string(reader, strings, file_format_version)?;
    let succeeded = read_bool(reader)?;
    Ok(ProjectFinishedEvent {
        fields,
        project_file,
        succeeded,
    })
}

// ---------------------------------------------------------------------------
// MN-12: Target lifecycle events
// ---------------------------------------------------------------------------

/// `TargetStarted` event.
#[derive(Debug, Clone)]
pub struct TargetStartedEvent {
    pub fields: BuildEventArgsFields,
    pub target_name: Option<String>,
    pub project_file: Option<String>,
    pub target_file: Option<String>,
    pub parent_target: Option<String>,
    /// `TargetBuiltReason` (introduced in format version 4).
    pub build_reason: i32,
}

/// Read a `TargetStarted` event from the stream.
pub fn read_target_started(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<TargetStartedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let target_name = read_optional_string(reader, strings, file_format_version)?;
    let project_file = read_optional_string(reader, strings, file_format_version)?;
    let target_file = read_optional_string(reader, strings, file_format_version)?;
    let parent_target = read_optional_string(reader, strings, file_format_version)?;
    let build_reason = if file_format_version > 3 {
        read_7bit_int(reader)?
    } else {
        0 // TargetBuiltReason::None
    };
    Ok(TargetStartedEvent {
        fields,
        target_name,
        project_file,
        target_file,
        parent_target,
        build_reason,
    })
}

/// `TargetFinished` event.
#[derive(Debug, Clone)]
pub struct TargetFinishedEvent {
    pub fields: BuildEventArgsFields,
    pub succeeded: bool,
    pub project_file: Option<String>,
    pub target_file: Option<String>,
    pub target_name: Option<String>,
    pub target_outputs: Option<Vec<TaskItem>>,
}

/// Read a `TargetFinished` event from the stream.
pub fn read_target_finished(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<TargetFinishedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let succeeded = read_bool(reader)?;
    let project_file = read_optional_string(reader, strings, file_format_version)?;
    let target_file = read_optional_string(reader, strings, file_format_version)?;
    let target_name = read_optional_string(reader, strings, file_format_version)?;
    let target_outputs = read_task_item_list(reader, strings, nvl_table, file_format_version)?;
    Ok(TargetFinishedEvent {
        fields,
        succeeded,
        project_file,
        target_file,
        target_name,
        target_outputs,
    })
}

/// `TargetSkipped` event.
#[derive(Debug, Clone)]
pub struct TargetSkippedEvent {
    pub fields: BuildEventArgsFields,
    pub target_file: Option<String>,
    pub target_name: Option<String>,
    pub parent_target: Option<String>,
    pub condition: Option<String>,
    pub evaluated_condition: Option<String>,
    pub originally_succeeded: bool,
    /// `TargetSkipReason` (raw i32, introduced in format version 14).
    pub skip_reason: i32,
    pub build_reason: i32,
    pub original_build_event_context: Option<BuildEventContext>,
}

/// Read a `TargetSkipped` event from the stream.
pub fn read_target_skipped(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<TargetSkippedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, true)?;
    let target_file = read_optional_string(reader, strings, file_format_version)?;
    let target_name = read_optional_string(reader, strings, file_format_version)?;
    let parent_target = read_optional_string(reader, strings, file_format_version)?;

    let mut condition = None;
    let mut evaluated_condition = None;
    let mut originally_succeeded = false;
    let mut skip_reason = 0; // TargetSkipReason::None

    if file_format_version >= 13 {
        condition = read_optional_string(reader, strings, file_format_version)?;
        evaluated_condition = read_optional_string(reader, strings, file_format_version)?;
        originally_succeeded = read_bool(reader)?;

        // Infer skip reason from available data (matches C# reference).
        skip_reason = if condition.is_some() {
            1 // TargetSkipReason::ConditionWasFalse
        } else if originally_succeeded {
            2 // TargetSkipReason::PreviouslyBuiltSuccessfully
        } else {
            3 // TargetSkipReason::PreviouslyBuiltUnsuccessfully
        };
    }

    let build_reason = read_7bit_int(reader)?;

    let mut original_build_event_context = None;
    if file_format_version >= 14 {
        skip_reason = read_7bit_int(reader)?;
        // Optional BuildEventContext (uses a format like BinaryReaderExtensions)
        if read_bool(reader)? {
            original_build_event_context =
                Some(read_build_event_context(reader, file_format_version)?);
        }
    }

    Ok(TargetSkippedEvent {
        fields,
        target_file,
        target_name,
        parent_target,
        condition,
        evaluated_condition,
        originally_succeeded,
        skip_reason,
        build_reason,
        original_build_event_context,
    })
}

// ---------------------------------------------------------------------------
// MN-13: Task lifecycle events
// ---------------------------------------------------------------------------

/// `TaskStarted` event.
#[derive(Debug, Clone)]
pub struct TaskStartedEvent {
    pub fields: BuildEventArgsFields,
    pub task_name: Option<String>,
    pub project_file: Option<String>,
    pub task_file: Option<String>,
    pub task_assembly_location: Option<String>,
}

/// Read a `TaskStarted` event from the stream.
pub fn read_task_started(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<TaskStartedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let task_name = read_optional_string(reader, strings, file_format_version)?;
    let project_file = read_optional_string(reader, strings, file_format_version)?;
    let task_file = read_optional_string(reader, strings, file_format_version)?;
    let task_assembly_location = if file_format_version > 19 {
        read_optional_string(reader, strings, file_format_version)?
    } else {
        None
    };
    Ok(TaskStartedEvent {
        fields,
        task_name,
        project_file,
        task_file,
        task_assembly_location,
    })
}

/// `TaskFinished` event.
#[derive(Debug, Clone)]
pub struct TaskFinishedEvent {
    pub fields: BuildEventArgsFields,
    pub succeeded: bool,
    pub task_name: Option<String>,
    pub project_file: Option<String>,
    pub task_file: Option<String>,
}

/// Read a `TaskFinished` event from the stream.
pub fn read_task_finished(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<TaskFinishedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let succeeded = read_bool(reader)?;
    let task_name = read_optional_string(reader, strings, file_format_version)?;
    let project_file = read_optional_string(reader, strings, file_format_version)?;
    let task_file = read_optional_string(reader, strings, file_format_version)?;
    Ok(TaskFinishedEvent {
        fields,
        succeeded,
        task_name,
        project_file,
        task_file,
    })
}

/// `TaskCommandLine` event — the command line used to invoke an external tool.
#[derive(Debug, Clone)]
pub struct TaskCommandLineEvent {
    pub fields: BuildEventArgsFields,
    pub command_line: Option<String>,
    pub task_name: Option<String>,
}

/// Read a `TaskCommandLine` event from the stream.
pub fn read_task_command_line(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<TaskCommandLineEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, true)?;
    let command_line = read_optional_string(reader, strings, file_format_version)?;
    let task_name = read_optional_string(reader, strings, file_format_version)?;
    Ok(TaskCommandLineEvent {
        fields,
        command_line,
        task_name,
    })
}

/// `TaskParameter` event — input/output parameters of a task.
#[derive(Debug, Clone)]
pub struct TaskParameterEvent {
    pub fields: BuildEventArgsFields,
    /// `TaskParameterMessageKind` (raw i32).
    pub kind: i32,
    pub item_type: Option<String>,
    pub items: Option<Vec<TaskItem>>,
    pub parameter_name: Option<String>,
    pub property_name: Option<String>,
}

/// Read a `TaskParameter` event from the stream.
pub fn read_task_parameter(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<TaskParameterEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, true)?;
    let kind = read_7bit_int(reader)?;
    let item_type = read_dedup_string(reader, strings)?;
    let items = read_task_item_list(reader, strings, nvl_table, file_format_version)?;
    let (parameter_name, property_name) = if file_format_version >= 21 {
        (
            read_dedup_string(reader, strings)?,
            read_dedup_string(reader, strings)?,
        )
    } else {
        (None, None)
    };
    Ok(TaskParameterEvent {
        fields,
        kind,
        item_type,
        items,
        parameter_name,
        property_name,
    })
}

// ---------------------------------------------------------------------------
// MN-14: Diagnostic events (Error, Warning, Message, CriticalBuildMessage)
// ---------------------------------------------------------------------------

/// A build error event.
#[derive(Debug, Clone)]
pub struct BuildErrorEvent {
    pub fields: BuildEventArgsFields,
    pub location: DiagnosticLocation,
}

/// Read a `BuildError` event from the stream.
pub fn read_build_error(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<BuildErrorEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let location = read_diagnostic_fields(reader, strings, file_format_version)?;
    Ok(BuildErrorEvent { fields, location })
}

/// A build warning event.
#[derive(Debug, Clone)]
pub struct BuildWarningEvent {
    pub fields: BuildEventArgsFields,
    pub location: DiagnosticLocation,
}

/// Read a `BuildWarning` event from the stream.
pub fn read_build_warning(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<BuildWarningEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let location = read_diagnostic_fields(reader, strings, file_format_version)?;
    Ok(BuildWarningEvent { fields, location })
}

/// A build message event.
#[derive(Debug, Clone)]
pub struct BuildMessageEvent {
    pub fields: BuildEventArgsFields,
}

/// Read a `BuildMessage` event from the stream.
pub fn read_build_message(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<BuildMessageEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, true)?;
    Ok(BuildMessageEvent { fields })
}

/// A critical build message event (same layout as message, different severity).
#[derive(Debug, Clone)]
pub struct CriticalBuildMessageEvent {
    pub fields: BuildEventArgsFields,
}

/// Read a `CriticalBuildMessage` event from the stream.
pub fn read_critical_build_message(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<CriticalBuildMessageEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, true)?;
    Ok(CriticalBuildMessageEvent { fields })
}

// ---------------------------------------------------------------------------
// MN-15: Evaluation events
// ---------------------------------------------------------------------------

/// `ProjectEvaluationStarted` event.
#[derive(Debug, Clone)]
pub struct ProjectEvaluationStartedEvent {
    pub fields: BuildEventArgsFields,
    pub project_file: Option<String>,
}

/// Read a `ProjectEvaluationStarted` event from the stream.
pub fn read_project_evaluation_started(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<ProjectEvaluationStartedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let project_file = read_dedup_string(reader, strings)?;
    Ok(ProjectEvaluationStartedEvent {
        fields,
        project_file,
    })
}

/// `ProjectEvaluationFinished` event.
#[derive(Debug, Clone)]
pub struct ProjectEvaluationFinishedEvent {
    pub fields: BuildEventArgsFields,
    pub project_file: Option<String>,
    pub global_properties: Option<Vec<(String, String)>>,
    pub property_list: Option<Vec<(String, String)>>,
    pub item_list: Option<Vec<ItemGroup>>,
    pub has_profile_data: bool,
    pub profile_data: Option<Vec<ProfileEntry>>,
}

/// A single profiler entry from evaluation profiling data.
#[derive(Debug, Clone)]
pub struct ProfileEntry {
    pub location: EvaluationLocation,
    pub profiled_location: ProfiledLocation,
}

/// Location data for an evaluation profiler entry.
#[derive(Debug, Clone)]
pub struct EvaluationLocation {
    pub element_name: Option<String>,
    pub description: Option<String>,
    pub evaluation_description: Option<String>,
    pub file: Option<String>,
    pub kind: i32,
    pub evaluation_pass: i32,
    pub line: Option<i32>,
    pub id: i64,
    pub parent_id: Option<i64>,
}

/// Timing data for an evaluation profiler entry.
#[derive(Debug, Clone, Copy)]
pub struct ProfiledLocation {
    pub number_of_hits: i32,
    /// Exclusive time in ticks.
    pub exclusive_time_ticks: i64,
    /// Inclusive time in ticks.
    pub inclusive_time_ticks: i64,
}

/// Read a `ProjectEvaluationFinished` event from the stream.
pub fn read_project_evaluation_finished(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<ProjectEvaluationFinishedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let project_file = read_dedup_string(reader, strings)?;

    let mut global_properties = None;
    let mut property_list = None;
    let mut item_list = None;
    let mut has_profile_data = false;
    let mut profile_data = None;

    if file_format_version >= 12 {
        // In v18+ global properties are always written. In v12-17 a bool prefix.
        if file_format_version >= 18 || read_bool(reader)? {
            global_properties =
                read_string_dictionary(reader, strings, nvl_table, file_format_version)?;
        }

        property_list = read_property_list(reader, strings, nvl_table, file_format_version)?;
        item_list = read_project_items(reader, strings, nvl_table, file_format_version)?;
    }

    // ProfilerResult introduced in version 5.
    if file_format_version > 4 {
        has_profile_data = read_bool(reader)?;
        if has_profile_data {
            let count = read_7bit_length(reader, "profile entry count")?;
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let location = read_evaluation_location(reader, strings, file_format_version)?;
                let profiled = read_profiled_location(reader)?;
                entries.push(ProfileEntry {
                    location,
                    profiled_location: profiled,
                });
            }
            profile_data = Some(entries);
        }
    }

    Ok(ProjectEvaluationFinishedEvent {
        fields,
        project_file,
        global_properties,
        property_list,
        item_list,
        has_profile_data,
        profile_data,
    })
}

// ---------------------------------------------------------------------------
// MN-16: Property event types
// ---------------------------------------------------------------------------

/// `PropertyReassignment` event — a property value was overwritten.
#[derive(Debug, Clone)]
pub struct PropertyReassignmentEvent {
    pub fields: BuildEventArgsFields,
    pub property_name: Option<String>,
    pub previous_value: Option<String>,
    pub new_value: Option<String>,
    pub location: Option<String>,
}

/// Read a `PropertyReassignment` event from the stream.
pub fn read_property_reassignment(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<PropertyReassignmentEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, true)?;
    let property_name = read_dedup_string(reader, strings)?;
    let previous_value = read_dedup_string(reader, strings)?;
    let new_value = read_dedup_string(reader, strings)?;
    let location = read_dedup_string(reader, strings)?;
    Ok(PropertyReassignmentEvent {
        fields,
        property_name,
        previous_value,
        new_value,
        location,
    })
}

/// `UninitializedPropertyRead` event — a property was read before being set.
#[derive(Debug, Clone)]
pub struct UninitializedPropertyReadEvent {
    pub fields: BuildEventArgsFields,
    pub property_name: Option<String>,
}

/// Read a `UninitializedPropertyRead` event from the stream.
pub fn read_uninitialized_property_read(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<UninitializedPropertyReadEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, true)?;
    let property_name = read_dedup_string(reader, strings)?;
    Ok(UninitializedPropertyReadEvent {
        fields,
        property_name,
    })
}

/// `PropertyInitialValueSet` event — a property's initial value was set.
#[derive(Debug, Clone)]
pub struct PropertyInitialValueSetEvent {
    pub fields: BuildEventArgsFields,
    pub property_name: Option<String>,
    pub property_value: Option<String>,
    pub property_source: Option<String>,
}

/// Read a `PropertyInitialValueSet` event from the stream.
pub fn read_property_initial_value_set(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<PropertyInitialValueSetEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, true)?;
    let property_name = read_dedup_string(reader, strings)?;
    let property_value = read_dedup_string(reader, strings)?;
    let property_source = read_dedup_string(reader, strings)?;
    Ok(PropertyInitialValueSetEvent {
        fields,
        property_name,
        property_value,
        property_source,
    })
}

// ---------------------------------------------------------------------------
// MN-17: Environment/Response event types
// ---------------------------------------------------------------------------

/// `EnvironmentVariableRead` event — an environment variable was read during evaluation.
#[derive(Debug, Clone)]
pub struct EnvironmentVariableReadEvent {
    pub fields: BuildEventArgsFields,
    pub environment_variable_name: Option<String>,
    /// Line number in the project file (format version ≥ 22 only).
    pub line: i32,
    /// Column number in the project file (format version ≥ 22 only).
    pub column: i32,
    /// File name where the read occurred (format version ≥ 22 only).
    pub file_name: Option<String>,
}

/// Read an `EnvironmentVariableRead` event from the stream.
pub fn read_environment_variable_read(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<EnvironmentVariableReadEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, true)?;
    let environment_variable_name = read_dedup_string(reader, strings)?;
    let (line, column, file_name) = if file_format_version >= 22 {
        let line = read_7bit_int(reader)?;
        let column = read_7bit_int(reader)?;
        let file_name = read_dedup_string(reader, strings)?;
        (line, column, file_name)
    } else {
        (0, 0, None)
    };
    Ok(EnvironmentVariableReadEvent {
        fields,
        environment_variable_name,
        line,
        column,
        file_name,
    })
}

/// `ResponseFileUsed` event — a response file was consumed by MSBuild.
#[derive(Debug, Clone)]
pub struct ResponseFileUsedEvent {
    pub fields: BuildEventArgsFields,
    pub response_file_path: Option<String>,
}

/// Read a `ResponseFileUsed` event from the stream.
pub fn read_response_file_used(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<ResponseFileUsedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let response_file_path = read_dedup_string(reader, strings)?;
    Ok(ResponseFileUsedEvent {
        fields,
        response_file_path,
    })
}

// ---------------------------------------------------------------------------
// MN-18: Assembly/Import event types
// ---------------------------------------------------------------------------

/// `AssemblyLoad` event — an assembly was loaded during the build.
#[derive(Debug, Clone)]
pub struct AssemblyLoadEvent {
    pub fields: BuildEventArgsFields,
    /// `AssemblyLoadingContext` (raw i32).
    pub context: i32,
    pub loading_initiator: Option<String>,
    pub assembly_name: Option<String>,
    pub assembly_path: Option<String>,
    /// Module version identifier (GUID, 16 bytes).
    pub mvid: [u8; 16],
    pub app_domain_name: Option<String>,
}

/// Read an `AssemblyLoad` event from the stream.
pub fn read_assembly_load(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<AssemblyLoadEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let context = read_7bit_int(reader)?;
    let loading_initiator = read_dedup_string(reader, strings)?;
    let assembly_name = read_dedup_string(reader, strings)?;
    let assembly_path = read_dedup_string(reader, strings)?;
    let mvid = crate::primitives::read_guid(reader)?;
    let app_domain_name = read_dedup_string(reader, strings)?;
    Ok(AssemblyLoadEvent {
        fields,
        context,
        loading_initiator,
        assembly_name,
        assembly_path,
        mvid,
        app_domain_name,
    })
}

/// `ProjectImported` event — a project file was imported.
#[derive(Debug, Clone)]
pub struct ProjectImportedEvent {
    pub fields: BuildEventArgsFields,
    /// Whether the import was ignored (format version > 2 only).
    pub import_ignored: bool,
    pub imported_project_file: Option<String>,
    pub unexpanded_project: Option<String>,
}

/// Read a `ProjectImported` event from the stream.
pub fn read_project_imported(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<ProjectImportedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, true)?;
    let import_ignored = if file_format_version > 2 {
        read_bool(reader)?
    } else {
        false
    };
    let imported_project_file = read_optional_string(reader, strings, file_format_version)?;
    let unexpanded_project = read_optional_string(reader, strings, file_format_version)?;
    Ok(ProjectImportedEvent {
        fields,
        import_ignored,
        imported_project_file,
        unexpanded_project,
    })
}

// ---------------------------------------------------------------------------
// MN-19: BuildCheck event types
// ---------------------------------------------------------------------------

// BuildCheckMessage, BuildCheckWarning, and BuildCheckError share the same
// binary layout as BuildMessage, BuildWarning, and BuildError respectively.
// Use `read_build_message`, `read_build_warning`, and `read_build_error` to
// read them. The record kind discriminant distinguishes them at the dispatch
// layer.

/// Type alias: a BuildCheck message has the same structure as a `BuildMessageEvent`.
pub type BuildCheckMessageEvent = BuildMessageEvent;

/// Type alias: a BuildCheck warning has the same structure as a `BuildWarningEvent`.
pub type BuildCheckWarningEvent = BuildWarningEvent;

/// Type alias: a BuildCheck error has the same structure as a `BuildErrorEvent`.
pub type BuildCheckErrorEvent = BuildErrorEvent;

/// `BuildCheckTracing` event — timing data for build check rules.
#[derive(Debug, Clone)]
pub struct BuildCheckTracingEvent {
    pub fields: BuildEventArgsFields,
    /// Raw tracing data: rule name → duration ticks as string.
    pub tracing_data: Option<Vec<(String, String)>>,
}

/// Read a `BuildCheckTracing` event from the stream.
pub fn read_build_check_tracing(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<BuildCheckTracingEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let tracing_data = read_string_dictionary(reader, strings, nvl_table, file_format_version)?;
    Ok(BuildCheckTracingEvent {
        fields,
        tracing_data,
    })
}

/// `BuildCheckAcquisition` event — a build check rule was acquired.
#[derive(Debug, Clone)]
pub struct BuildCheckAcquisitionEvent {
    pub fields: BuildEventArgsFields,
    pub acquisition_path: String,
    pub project_path: String,
}

/// Read a `BuildCheckAcquisition` event from the stream.
pub fn read_build_check_acquisition(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<BuildCheckAcquisitionEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let acquisition_path = crate::primitives::read_dotnet_string(reader)?;
    let project_path = crate::primitives::read_dotnet_string(reader)?;
    Ok(BuildCheckAcquisitionEvent {
        fields,
        acquisition_path,
        project_path,
    })
}

// ---------------------------------------------------------------------------
// MN-20: Remaining event types
// ---------------------------------------------------------------------------

/// `BuildSubmissionStarted` event — a build submission was queued.
#[derive(Debug, Clone)]
pub struct BuildSubmissionStartedEvent {
    pub fields: BuildEventArgsFields,
    pub global_properties: Option<Vec<(String, String)>>,
    pub entry_projects_full_path: Option<Vec<String>>,
    pub target_names: Option<Vec<String>>,
    /// `BuildRequestDataFlags` (raw i32).
    pub flags: i32,
    pub submission_id: i32,
}

/// Read a `BuildSubmissionStarted` event from the stream.
pub fn read_build_submission_started(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<BuildSubmissionStartedEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    let global_properties =
        read_string_dictionary(reader, strings, nvl_table, file_format_version)?;
    let entry_projects_full_path = read_string_list(reader, strings)?;
    let target_names = read_string_list(reader, strings)?;
    let flags = read_7bit_int(reader)?;
    let submission_id = read_7bit_int(reader)?;
    Ok(BuildSubmissionStartedEvent {
        fields,
        global_properties,
        entry_projects_full_path,
        target_names,
        flags,
        submission_id,
    })
}

/// `BuildCanceled` event — the build was canceled.
#[derive(Debug, Clone)]
pub struct BuildCanceledEvent {
    pub fields: BuildEventArgsFields,
}

/// Read a `BuildCanceled` event from the stream.
pub fn read_build_canceled(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<BuildCanceledEvent, MuninError> {
    let fields = read_build_event_args_fields(reader, strings, file_format_version, false)?;
    Ok(BuildCanceledEvent { fields })
}

// ---------------------------------------------------------------------------
// Shared data structures
// ---------------------------------------------------------------------------

/// A task item (item spec + metadata).
#[derive(Debug, Clone)]
pub struct TaskItem {
    pub item_spec: Option<String>,
    pub metadata: Option<Vec<(String, String)>>,
}

/// A group of items with the same type name.
#[derive(Debug, Clone)]
pub struct ItemGroup {
    pub item_type: String,
    pub items: Vec<TaskItem>,
}

// ---------------------------------------------------------------------------
// Shared item/property readers
// ---------------------------------------------------------------------------

/// Read a count-prefixed list of deduplicated strings.
///
/// Returns `None` if the count is zero.
fn read_string_list(
    reader: &mut impl Read,
    strings: &StringTable,
) -> Result<Option<Vec<String>>, MuninError> {
    let count = read_7bit_length(reader, "string-list count")?;
    if count == 0 {
        return Ok(None);
    }
    let mut list = Vec::with_capacity(count);
    for _ in 0..count {
        if let Some(s) = read_dedup_string(reader, strings)? {
            list.push(s);
        }
    }
    if list.is_empty() {
        Ok(None)
    } else {
        Ok(Some(list))
    }
}

/// Read a property list (string dictionary representing properties).
fn read_property_list(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<Option<Vec<(String, String)>>, MuninError> {
    read_string_dictionary(reader, strings, nvl_table, file_format_version)
}

/// Read a single task item (item spec + metadata dictionary).
fn read_task_item(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<TaskItem, MuninError> {
    let item_spec = read_dedup_string(reader, strings)?;
    let metadata = read_string_dictionary(reader, strings, nvl_table, file_format_version)?;
    Ok(TaskItem {
        item_spec,
        metadata,
    })
}

/// Read a list of task items (count-prefixed).
fn read_task_item_list(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<Option<Vec<TaskItem>>, MuninError> {
    let count = read_7bit_length(reader, "task-item count")?;
    if count == 0 {
        return Ok(None);
    }
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(read_task_item(
            reader,
            strings,
            nvl_table,
            file_format_version,
        )?);
    }
    Ok(Some(items))
}

/// Read project items (grouped by item type).
///
/// The format evolved across versions:
/// - Pre-v10: flat list of (item-name-string, task-item) pairs.
/// - v10–11: count-prefixed groups of (item-type-dedup, task-item-list).
/// - v12+: sequential groups terminated by an empty item-type string.
fn read_project_items(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<Option<Vec<ItemGroup>>, MuninError> {
    if file_format_version < 10 {
        let count = read_7bit_length(reader, "item count")?;
        if count == 0 {
            return Ok(None);
        }
        let mut groups: Vec<ItemGroup> = Vec::new();
        for _ in 0..count {
            let item_name = read_dotnet_string_via_reader(reader)?;
            let item = read_task_item(reader, strings, nvl_table, file_format_version)?;
            // Group by item_name.
            if let Some(group) = groups.iter_mut().find(|g| g.item_type == item_name) {
                group.items.push(item);
            } else {
                groups.push(ItemGroup {
                    item_type: item_name,
                    items: vec![item],
                });
            }
        }
        Ok(Some(groups))
    } else if file_format_version < 12 {
        let count = read_7bit_length(reader, "item-group count")?;
        if count == 0 {
            return Ok(None);
        }
        let mut groups = Vec::with_capacity(count);
        for _ in 0..count {
            let item_type = read_dedup_string(reader, strings)?.unwrap_or_default();
            let items = read_task_item_list(reader, strings, nvl_table, file_format_version)?
                .unwrap_or_default();
            groups.push(ItemGroup { item_type, items });
        }
        if groups.is_empty() {
            Ok(None)
        } else {
            Ok(Some(groups))
        }
    } else {
        // v12+: groups terminated by empty item type.
        let mut groups = Vec::new();
        loop {
            let item_type = read_dedup_string(reader, strings)?.unwrap_or_default();
            if item_type.is_empty() {
                break;
            }
            let items = read_task_item_list(reader, strings, nvl_table, file_format_version)?
                .unwrap_or_default();
            groups.push(ItemGroup { item_type, items });
        }
        if groups.is_empty() {
            Ok(None)
        } else {
            Ok(Some(groups))
        }
    }
}

/// Read a raw .NET string (used for pre-v10 item names).
fn read_dotnet_string_via_reader(reader: &mut impl Read) -> Result<String, MuninError> {
    crate::primitives::read_dotnet_string(reader)
}

/// Read an `EvaluationLocation` for profiler data.
fn read_evaluation_location(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<EvaluationLocation, MuninError> {
    let element_name = read_optional_string(reader, strings, file_format_version)?;
    let description = read_optional_string(reader, strings, file_format_version)?;
    let evaluation_description = read_optional_string(reader, strings, file_format_version)?;
    let file = read_optional_string(reader, strings, file_format_version)?;
    let kind = read_7bit_int(reader)?;
    let evaluation_pass = read_7bit_int(reader)?;

    let line = if read_bool(reader)? {
        Some(read_7bit_int(reader)?)
    } else {
        None
    };

    // id and parent_id introduced in format version 6.
    let (id, parent_id) = if file_format_version > 5 {
        let id = crate::primitives::read_i64_le(reader)?;
        let parent_id = if read_bool(reader)? {
            Some(crate::primitives::read_i64_le(reader)?)
        } else {
            None
        };
        (id, parent_id)
    } else {
        (0, None)
    };

    Ok(EvaluationLocation {
        element_name,
        description,
        evaluation_description,
        file,
        kind,
        evaluation_pass,
        line,
        id,
        parent_id,
    })
}

/// Read a `ProfiledLocation` (hit count + exclusive/inclusive times).
fn read_profiled_location(reader: &mut impl Read) -> Result<ProfiledLocation, MuninError> {
    let number_of_hits = read_7bit_int(reader)?;
    let exclusive_time_ticks = crate::primitives::read_i64_le(reader)?;
    let inclusive_time_ticks = crate::primitives::read_i64_le(reader)?;
    Ok(ProfiledLocation {
        number_of_hits,
        exclusive_time_ticks,
        inclusive_time_ticks,
    })
}

#[cfg(test)]
mod tests;
