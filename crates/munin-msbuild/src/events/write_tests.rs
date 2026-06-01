// Copyright (c) Michael Grier
//
// Round-trip tests: every `write_*` is paired with its `read_*` and the
// resulting event is compared against the original via JSON serialization
// (event structs don't derive PartialEq).

use std::io::Cursor;

use serde_json::Value;

use crate::{
    context::BuildEventContext,
    events::*,
    field_flags::BuildEventArgsFieldFlags,
    fields::{BuildEventArgsFields, ExtendedDataFields},
    primitives::BinlogDateTime,
    writers::WriteContext,
};

/// Format version high enough that every modern field is exercised.
const V: i32 = 22;

fn fields_with_message(msg: &str) -> BuildEventArgsFields {
    BuildEventArgsFields {
        flags: BuildEventArgsFieldFlags::MESSAGE
            | BuildEventArgsFieldFlags::THREAD_ID
            | BuildEventArgsFieldFlags::TIMESTAMP,
        message: Some(msg.to_string()),
        thread_id: 7,
        timestamp: Some(BinlogDateTime {
            ticks: 1234567,
            kind: 1,
        }),
        ..Default::default()
    }
}

fn make_ctx() -> WriteContext {
    WriteContext::new(V)
}

fn to_json<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap()
}

macro_rules! roundtrip_with_nvl {
    ($name:ident, $version:expr, $build:expr, $write:path, $read:path) => {
        #[test]
        fn $name() {
            let v: i32 = $version;
            let ev = $build;
            let mut ctx = WriteContext::new(v);
            let mut buf = Vec::new();
            $write(&mut buf, &mut ctx, &ev).expect("write");
            let mut cur = Cursor::new(buf);
            let decoded = $read(&mut cur, &ctx.strings, &ctx.nvl_table, v).expect("read");
            assert_eq!(to_json(&ev), to_json(&decoded));
        }
    };
}

macro_rules! roundtrip_no_nvl {
    ($name:ident, $version:expr, $build:expr, $write:path, $read:path) => {
        #[test]
        fn $name() {
            let v: i32 = $version;
            let ev = $build;
            let mut ctx = WriteContext::new(v);
            let mut buf = Vec::new();
            $write(&mut buf, &mut ctx, &ev).expect("write");
            let mut cur = Cursor::new(buf);
            let decoded = $read(&mut cur, &ctx.strings, v).expect("read");
            assert_eq!(to_json(&ev), to_json(&decoded));
        }
    };
}

roundtrip_with_nvl!(
    rt_build_started,
    V,
    BuildStartedEvent {
        fields: fields_with_message("BuildStarted"),
        environment: Some(vec![
            ("FOO".to_string(), "bar".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ]),
    },
    write_build_started,
    read_build_started
);

roundtrip_no_nvl!(
    rt_build_finished,
    V,
    BuildFinishedEvent {
        fields: fields_with_message("done"),
        succeeded: true,
    },
    write_build_finished,
    read_build_finished
);

roundtrip_with_nvl!(
    rt_project_started,
    V,
    ProjectStartedEvent {
        fields: fields_with_message("project"),
        parent_context: Some(BuildEventContext {
            node_id: 1,
            project_context_id: 2,
            target_id: 3,
            task_id: 4,
            submission_id: 5,
            project_instance_id: 6,
            evaluation_id: 7,
        }),
        project_file: Some("a.csproj".to_string()),
        project_id: 42,
        target_names: Some("Build;Clean".to_string()),
        tools_version: Some("17.0".to_string()),
        global_properties: Some(vec![("Configuration".to_string(), "Release".to_string())]),
        property_list: Some(vec![("Foo".to_string(), "1".to_string())]),
        item_list: Some(vec![ItemGroup {
            item_type: "Compile".to_string(),
            items: vec![TaskItem {
                item_spec: Some("Program.cs".to_string()),
                metadata: Some(vec![("Link".to_string(), "src/Program.cs".to_string())]),
            }],
        }]),
    },
    write_project_started,
    read_project_started
);

roundtrip_no_nvl!(
    rt_project_finished,
    V,
    ProjectFinishedEvent {
        fields: fields_with_message("project done"),
        project_file: Some("a.csproj".to_string()),
        succeeded: false,
    },
    write_project_finished,
    read_project_finished
);

roundtrip_no_nvl!(
    rt_target_started,
    V,
    TargetStartedEvent {
        fields: fields_with_message("target"),
        target_name: Some("Build".to_string()),
        project_file: Some("a.csproj".to_string()),
        target_file: Some("Microsoft.Common.targets".to_string()),
        parent_target: None,
        build_reason: 3,
    },
    write_target_started,
    read_target_started
);

roundtrip_with_nvl!(
    rt_target_finished,
    V,
    TargetFinishedEvent {
        fields: fields_with_message("target done"),
        succeeded: true,
        project_file: Some("a.csproj".to_string()),
        target_file: None,
        target_name: Some("Build".to_string()),
        target_outputs: Some(vec![TaskItem {
            item_spec: Some("bin/a.dll".to_string()),
            metadata: None,
        }]),
    },
    write_target_finished,
    read_target_finished
);

roundtrip_no_nvl!(
    rt_target_skipped,
    V,
    TargetSkippedEvent {
        fields: BuildEventArgsFields {
            flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE,
            message: Some("skipped".to_string()),
            importance: 2,
            ..Default::default()
        },
        target_file: Some("t.targets".to_string()),
        target_name: Some("Build".to_string()),
        parent_target: None,
        condition: Some("'$(X)' == 'y'".to_string()),
        evaluated_condition: Some("false".to_string()),
        originally_succeeded: true,
        skip_reason: 1,
        build_reason: 0,
        original_build_event_context: Some(BuildEventContext::default()),
    },
    write_target_skipped,
    read_target_skipped
);

roundtrip_no_nvl!(
    rt_task_started,
    V,
    TaskStartedEvent {
        fields: fields_with_message("task"),
        task_name: Some("Csc".to_string()),
        project_file: Some("a.csproj".to_string()),
        task_file: Some("Microsoft.CSharp.targets".to_string()),
        task_assembly_location: Some("/sdk/Csc.dll".to_string()),
    },
    write_task_started,
    read_task_started
);

roundtrip_no_nvl!(
    rt_task_finished,
    V,
    TaskFinishedEvent {
        fields: fields_with_message("task done"),
        succeeded: true,
        task_name: Some("Csc".to_string()),
        project_file: Some("a.csproj".to_string()),
        task_file: None,
    },
    write_task_finished,
    read_task_finished
);

roundtrip_no_nvl!(
    rt_task_command_line,
    V,
    TaskCommandLineEvent {
        fields: BuildEventArgsFields {
            flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE,
            message: Some("csc /nologo".to_string()),
            importance: 1,
            ..Default::default()
        },
        command_line: Some("csc /nologo Program.cs".to_string()),
        task_name: Some("Csc".to_string()),
    },
    write_task_command_line,
    read_task_command_line
);

roundtrip_with_nvl!(
    rt_task_parameter,
    V,
    TaskParameterEvent {
        fields: BuildEventArgsFields {
            flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE,
            message: Some("param".to_string()),
            importance: 0,
            ..Default::default()
        },
        kind: 1,
        item_type: Some("Compile".to_string()),
        items: Some(vec![TaskItem {
            item_spec: Some("Program.cs".to_string()),
            metadata: None,
        }]),
        parameter_name: Some("Sources".to_string()),
        property_name: None,
    },
    write_task_parameter,
    read_task_parameter
);

roundtrip_no_nvl!(
    rt_build_error,
    V,
    BuildErrorEvent {
        fields: fields_with_message("error!"),
        location: DiagnosticLocation {
            subcategory: Some("sub".to_string()),
            code: Some("CS1234".to_string()),
            file: Some("Program.cs".to_string()),
            project_file: Some("a.csproj".to_string()),
            line_number: 10,
            column_number: 5,
            end_line_number: 10,
            end_column_number: 20,
        },
    },
    write_build_error,
    read_build_error
);

roundtrip_no_nvl!(
    rt_build_warning,
    V,
    BuildWarningEvent {
        fields: fields_with_message("warn!"),
        location: DiagnosticLocation {
            subcategory: None,
            code: Some("CS0168".to_string()),
            file: Some("Program.cs".to_string()),
            project_file: None,
            line_number: 1,
            column_number: 2,
            end_line_number: 3,
            end_column_number: 4,
        },
    },
    write_build_warning,
    read_build_warning
);

roundtrip_no_nvl!(
    rt_build_message,
    V,
    BuildMessageEvent {
        fields: BuildEventArgsFields {
            flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE,
            message: Some("msg".to_string()),
            importance: 2,
            ..Default::default()
        },
    },
    write_build_message,
    read_build_message
);

roundtrip_no_nvl!(
    rt_critical_build_message,
    V,
    CriticalBuildMessageEvent {
        fields: BuildEventArgsFields {
            flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE,
            message: Some("crit".to_string()),
            importance: 0,
            ..Default::default()
        },
    },
    write_critical_build_message,
    read_critical_build_message
);

roundtrip_no_nvl!(
    rt_project_evaluation_started,
    V,
    ProjectEvaluationStartedEvent {
        fields: fields_with_message("eval start"),
        project_file: Some("a.csproj".to_string()),
    },
    write_project_evaluation_started,
    read_project_evaluation_started
);

roundtrip_with_nvl!(
    rt_project_evaluation_finished,
    V,
    ProjectEvaluationFinishedEvent {
        fields: fields_with_message("eval done"),
        project_file: Some("a.csproj".to_string()),
        global_properties: Some(vec![("Foo".to_string(), "1".to_string())]),
        property_list: Some(vec![("Bar".to_string(), "2".to_string())]),
        item_list: None,
        has_profile_data: true,
        profile_data: Some(vec![ProfileEntry {
            location: EvaluationLocation {
                element_name: Some("Import".to_string()),
                description: None,
                evaluation_description: Some("desc".to_string()),
                file: Some("a.targets".to_string()),
                kind: 1,
                evaluation_pass: 2,
                line: Some(10),
                id: 100,
                parent_id: Some(50),
            },
            profiled_location: ProfiledLocation {
                number_of_hits: 3,
                exclusive_time_ticks: 1000,
                inclusive_time_ticks: 2000,
            },
        }]),
    },
    write_project_evaluation_finished,
    read_project_evaluation_finished
);

roundtrip_no_nvl!(
    rt_property_reassignment,
    V,
    PropertyReassignmentEvent {
        fields: BuildEventArgsFields {
            flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE,
            message: Some("reassigned".to_string()),
            importance: 2,
            ..Default::default()
        },
        property_name: Some("Configuration".to_string()),
        previous_value: Some("Debug".to_string()),
        new_value: Some("Release".to_string()),
        location: Some("a.csproj(10,5)".to_string()),
    },
    write_property_reassignment,
    read_property_reassignment
);

roundtrip_no_nvl!(
    rt_uninitialized_property_read,
    V,
    UninitializedPropertyReadEvent {
        fields: BuildEventArgsFields {
            flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE,
            message: Some("uninit".to_string()),
            importance: 2,
            ..Default::default()
        },
        property_name: Some("Foo".to_string()),
    },
    write_uninitialized_property_read,
    read_uninitialized_property_read
);

roundtrip_no_nvl!(
    rt_property_initial_value_set,
    V,
    PropertyInitialValueSetEvent {
        fields: BuildEventArgsFields {
            flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE,
            message: Some("initial".to_string()),
            importance: 2,
            ..Default::default()
        },
        property_name: Some("Configuration".to_string()),
        property_value: Some("Debug".to_string()),
        property_source: Some("Command Line".to_string()),
    },
    write_property_initial_value_set,
    read_property_initial_value_set
);

roundtrip_no_nvl!(
    rt_environment_variable_read,
    V,
    EnvironmentVariableReadEvent {
        fields: BuildEventArgsFields {
            flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE,
            message: Some("env".to_string()),
            importance: 2,
            ..Default::default()
        },
        environment_variable_name: Some("PATH".to_string()),
        line: 10,
        column: 5,
        file_name: Some("a.csproj".to_string()),
    },
    write_environment_variable_read,
    read_environment_variable_read
);

roundtrip_no_nvl!(
    rt_response_file_used,
    V,
    ResponseFileUsedEvent {
        fields: fields_with_message("rsp"),
        response_file_path: Some("Foo.rsp".to_string()),
    },
    write_response_file_used,
    read_response_file_used
);

roundtrip_no_nvl!(
    rt_assembly_load,
    V,
    AssemblyLoadEvent {
        fields: fields_with_message("asm"),
        context: 1,
        loading_initiator: Some("MSBuild".to_string()),
        assembly_name: Some("Foo, Version=1.0.0.0".to_string()),
        assembly_path: Some("/sdk/Foo.dll".to_string()),
        mvid: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        app_domain_name: Some("Default".to_string()),
    },
    write_assembly_load,
    read_assembly_load
);

roundtrip_no_nvl!(
    rt_project_imported,
    V,
    ProjectImportedEvent {
        fields: BuildEventArgsFields {
            flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE,
            message: Some("imported".to_string()),
            importance: 2,
            ..Default::default()
        },
        import_ignored: true,
        imported_project_file: Some("imp.targets".to_string()),
        unexpanded_project: Some("$(X).targets".to_string()),
    },
    write_project_imported,
    read_project_imported
);

roundtrip_with_nvl!(
    rt_build_check_tracing,
    V,
    BuildCheckTracingEvent {
        fields: fields_with_message("check trace"),
        tracing_data: Some(vec![("rule1".to_string(), "100".to_string())]),
    },
    write_build_check_tracing,
    read_build_check_tracing
);

roundtrip_no_nvl!(
    rt_build_check_acquisition,
    V,
    BuildCheckAcquisitionEvent {
        fields: fields_with_message("acq"),
        acquisition_path: "/path/to/check.dll".to_string(),
        project_path: "/path/to/a.csproj".to_string(),
    },
    write_build_check_acquisition,
    read_build_check_acquisition
);

roundtrip_with_nvl!(
    rt_build_submission_started,
    V,
    BuildSubmissionStartedEvent {
        fields: fields_with_message("sub"),
        global_properties: Some(vec![("Foo".to_string(), "1".to_string())]),
        entry_projects_full_path: Some(vec!["a.csproj".to_string(), "b.csproj".to_string()]),
        target_names: Some(vec!["Build".to_string()]),
        flags: 5,
        submission_id: 42,
    },
    write_build_submission_started,
    read_build_submission_started
);

roundtrip_no_nvl!(
    rt_build_canceled,
    V,
    BuildCanceledEvent {
        fields: fields_with_message("canceled"),
    },
    write_build_canceled,
    read_build_canceled
);

// Sanity check: WriteContext interns strings and increments indices.
#[test]
fn intern_string_assigns_sequential_indices() {
    let mut ctx = make_ctx();
    assert_eq!(ctx.intern_string(None), 0);
    assert_eq!(ctx.intern_string(Some("")), 1);
    let a = ctx.intern_string(Some("alpha"));
    let b = ctx.intern_string(Some("beta"));
    let a2 = ctx.intern_string(Some("alpha"));
    assert_eq!(a, a2);
    assert!(a >= 10 && b >= 10 && b != a);
}

// Extended fields should round-trip when the EXTENDED flag is set.
#[test]
fn extended_fields_round_trip() {
    let fields = BuildEventArgsFields {
        flags: BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::EXTENDED,
        message: Some("m".to_string()),
        extended: Some(ExtendedDataFields {
            extended_type: Some("MyType".to_string()),
            extended_metadata_index: 17,
            extended_data: Some("data".to_string()),
        }),
        ..Default::default()
    };
    let ev = BuildMessageEvent { fields };
    let mut ctx = WriteContext::new(V);
    let mut buf = Vec::new();
    write_build_message(&mut buf, &mut ctx, &ev).unwrap();
    let mut cur = Cursor::new(buf);
    let decoded = read_build_message(&mut cur, &ctx.strings, V).unwrap();
    assert_eq!(to_json(&ev), to_json(&decoded));
}
