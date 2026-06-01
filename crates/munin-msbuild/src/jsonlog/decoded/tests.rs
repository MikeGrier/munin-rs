// Copyright (c) Michael Grier

//! Per-variant JSON round-trip tests for the decoded event types.
//!
//! Each test constructs a default value of one decoded variant,
//! serializes it to a `serde_json::Value`, deserializes it back, and
//! re-serializes the result. The two `Value`s must compare equal.
//! This proves that every field is present in both directions of the
//! serde derive and that no field is lost or silently renamed.

use super::*;

macro_rules! roundtrip_test {
    ($name:ident, $t:ty) => {
        #[test]
        fn $name() {
            let original = <$t as Default>::default();
            let json = serde_json::to_value(&original).expect("serialize");
            let back: $t = serde_json::from_value(json.clone()).expect("deserialize");
            let json2 = serde_json::to_value(&back).expect("re-serialize");
            assert_eq!(json, json2);
        }
    };
}

roundtrip_test!(roundtrip_build_started, BuildStarted);
roundtrip_test!(roundtrip_build_finished, BuildFinished);
roundtrip_test!(roundtrip_project_started, ProjectStarted);
roundtrip_test!(roundtrip_project_finished, ProjectFinished);
roundtrip_test!(roundtrip_target_started, TargetStarted);
roundtrip_test!(roundtrip_target_finished, TargetFinished);
roundtrip_test!(roundtrip_target_skipped, TargetSkipped);
roundtrip_test!(roundtrip_task_started, TaskStarted);
roundtrip_test!(roundtrip_task_finished, TaskFinished);
roundtrip_test!(roundtrip_task_command_line, TaskCommandLine);
roundtrip_test!(roundtrip_task_parameter, TaskParameter);
roundtrip_test!(roundtrip_error, Error);
roundtrip_test!(roundtrip_warning, Warning);
roundtrip_test!(roundtrip_message, Message);
roundtrip_test!(roundtrip_critical_build_message, CriticalBuildMessage);
roundtrip_test!(
    roundtrip_project_evaluation_started,
    ProjectEvaluationStarted
);
roundtrip_test!(
    roundtrip_project_evaluation_finished,
    ProjectEvaluationFinished
);
roundtrip_test!(roundtrip_property_reassignment, PropertyReassignment);
roundtrip_test!(
    roundtrip_uninitialized_property_read,
    UninitializedPropertyRead
);
roundtrip_test!(
    roundtrip_property_initial_value_set,
    PropertyInitialValueSet
);
roundtrip_test!(roundtrip_environment_variable_read, EnvironmentVariableRead);
roundtrip_test!(roundtrip_response_file_used, ResponseFileUsed);
roundtrip_test!(roundtrip_assembly_load, AssemblyLoad);
roundtrip_test!(roundtrip_project_imported, ProjectImported);
roundtrip_test!(roundtrip_build_check_message, BuildCheckMessage);
roundtrip_test!(roundtrip_build_check_warning, BuildCheckWarning);
roundtrip_test!(roundtrip_build_check_error, BuildCheckError);
roundtrip_test!(roundtrip_build_check_tracing, BuildCheckTracing);
roundtrip_test!(roundtrip_build_check_acquisition, BuildCheckAcquisition);
roundtrip_test!(roundtrip_build_submission_started, BuildSubmissionStarted);
roundtrip_test!(roundtrip_build_canceled, BuildCanceled);
