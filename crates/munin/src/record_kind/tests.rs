// Copyright (c) Michael Grier

use super::BinaryLogRecordKind;

#[test]
fn round_trip_all_variants() {
    let all = [
        (0, BinaryLogRecordKind::EndOfFile),
        (1, BinaryLogRecordKind::BuildStarted),
        (2, BinaryLogRecordKind::BuildFinished),
        (3, BinaryLogRecordKind::ProjectStarted),
        (4, BinaryLogRecordKind::ProjectFinished),
        (5, BinaryLogRecordKind::TargetStarted),
        (6, BinaryLogRecordKind::TargetFinished),
        (7, BinaryLogRecordKind::TaskStarted),
        (8, BinaryLogRecordKind::TaskFinished),
        (9, BinaryLogRecordKind::Error),
        (10, BinaryLogRecordKind::Warning),
        (11, BinaryLogRecordKind::Message),
        (12, BinaryLogRecordKind::TaskCommandLine),
        (13, BinaryLogRecordKind::CriticalBuildMessage),
        (14, BinaryLogRecordKind::ProjectEvaluationStarted),
        (15, BinaryLogRecordKind::ProjectEvaluationFinished),
        (16, BinaryLogRecordKind::ProjectImported),
        (17, BinaryLogRecordKind::ProjectImportArchive),
        (18, BinaryLogRecordKind::TargetSkipped),
        (19, BinaryLogRecordKind::PropertyReassignment),
        (20, BinaryLogRecordKind::UninitializedPropertyRead),
        (21, BinaryLogRecordKind::EnvironmentVariableRead),
        (22, BinaryLogRecordKind::PropertyInitialValueSet),
        (23, BinaryLogRecordKind::NameValueList),
        (24, BinaryLogRecordKind::String),
        (25, BinaryLogRecordKind::TaskParameter),
        (26, BinaryLogRecordKind::ResponseFileUsed),
        (27, BinaryLogRecordKind::AssemblyLoad),
        (28, BinaryLogRecordKind::BuildCheckMessage),
        (29, BinaryLogRecordKind::BuildCheckWarning),
        (30, BinaryLogRecordKind::BuildCheckError),
        (31, BinaryLogRecordKind::BuildCheckTracing),
        (32, BinaryLogRecordKind::BuildCheckAcquisition),
        (33, BinaryLogRecordKind::BuildSubmissionStarted),
        (34, BinaryLogRecordKind::BuildCanceled),
    ];

    for (raw, expected) in &all {
        let parsed = BinaryLogRecordKind::from_raw(*raw);
        assert_eq!(parsed, Some(*expected), "from_raw({raw}) mismatch");
        assert_eq!(
            *expected as u8, *raw as u8,
            "repr mismatch for {expected:?}"
        );
    }
}

#[test]
fn from_raw_unknown_returns_none() {
    assert_eq!(BinaryLogRecordKind::from_raw(-1), None);
    assert_eq!(BinaryLogRecordKind::from_raw(35), None);
    assert_eq!(BinaryLogRecordKind::from_raw(255), None);
    assert_eq!(BinaryLogRecordKind::from_raw(i32::MAX), None);
}

#[test]
fn is_auxiliary_correct() {
    assert!(BinaryLogRecordKind::String.is_auxiliary());
    assert!(BinaryLogRecordKind::NameValueList.is_auxiliary());
    assert!(BinaryLogRecordKind::ProjectImportArchive.is_auxiliary());

    assert!(!BinaryLogRecordKind::EndOfFile.is_auxiliary());
    assert!(!BinaryLogRecordKind::BuildStarted.is_auxiliary());
    assert!(!BinaryLogRecordKind::Error.is_auxiliary());
    assert!(!BinaryLogRecordKind::Message.is_auxiliary());
    assert!(!BinaryLogRecordKind::TaskParameter.is_auxiliary());
    assert!(!BinaryLogRecordKind::BuildCanceled.is_auxiliary());
}

#[test]
fn variant_count_is_35() {
    // 0 through 34 inclusive = 35 variants.
    let mut count = 0;
    for i in 0..=255 {
        if BinaryLogRecordKind::from_raw(i).is_some() {
            count += 1;
        }
    }
    assert_eq!(count, 35);
}
