// Copyright (c) Michael Grier

//! JSON-friendly mirrors of every [`crate::BinlogEvent`] variant.
//!
//! These are re-exports of the canonical event structs from
//! [`crate::events`]; each struct already carries `Serialize` /
//! `Deserialize` derives and contains only JSON-friendly fields (no
//! `Cursor`, no raw payload bytes). The re-exports drop the trailing
//! `Event` suffix so the names line up with [`crate::BinlogEvent`]
//! variant names.

pub use crate::events::{
    AssemblyLoadEvent as AssemblyLoad, BuildCanceledEvent as BuildCanceled,
    BuildCheckAcquisitionEvent as BuildCheckAcquisition, BuildCheckErrorEvent as BuildCheckError,
    BuildCheckMessageEvent as BuildCheckMessage, BuildCheckTracingEvent as BuildCheckTracing,
    BuildCheckWarningEvent as BuildCheckWarning, BuildErrorEvent as Error,
    BuildFinishedEvent as BuildFinished, BuildMessageEvent as Message,
    BuildStartedEvent as BuildStarted, BuildSubmissionStartedEvent as BuildSubmissionStarted,
    BuildWarningEvent as Warning, CriticalBuildMessageEvent as CriticalBuildMessage,
    EnvironmentVariableReadEvent as EnvironmentVariableRead,
    ProjectEvaluationFinishedEvent as ProjectEvaluationFinished,
    ProjectEvaluationStartedEvent as ProjectEvaluationStarted,
    ProjectFinishedEvent as ProjectFinished, ProjectImportedEvent as ProjectImported,
    ProjectStartedEvent as ProjectStarted, PropertyInitialValueSetEvent as PropertyInitialValueSet,
    PropertyReassignmentEvent as PropertyReassignment, ResponseFileUsedEvent as ResponseFileUsed,
    TargetFinishedEvent as TargetFinished, TargetSkippedEvent as TargetSkipped,
    TargetStartedEvent as TargetStarted, TaskCommandLineEvent as TaskCommandLine,
    TaskFinishedEvent as TaskFinished, TaskParameterEvent as TaskParameter,
    TaskStartedEvent as TaskStarted, UninitializedPropertyReadEvent as UninitializedPropertyRead,
};
