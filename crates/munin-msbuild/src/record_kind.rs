// Copyright (c) Michael Grier

//! Record kind discriminants for the binary log format.
//!
//! Each record in the decompressed binlog stream begins with a 7-bit encoded
//! integer identifying its kind. Changing any discriminant value is a breaking
//! change — these values are part of the on-disk format.

/// Indicates the type of record stored in the binary log.
///
/// There is a record type for each type of build event and also a few
/// meta-data record types (string data, name-value-list data, EOF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BinaryLogRecordKind {
    EndOfFile = 0,
    BuildStarted = 1,
    BuildFinished = 2,
    ProjectStarted = 3,
    ProjectFinished = 4,
    TargetStarted = 5,
    TargetFinished = 6,
    TaskStarted = 7,
    TaskFinished = 8,
    Error = 9,
    Warning = 10,
    Message = 11,
    TaskCommandLine = 12,
    CriticalBuildMessage = 13,
    ProjectEvaluationStarted = 14,
    ProjectEvaluationFinished = 15,
    ProjectImported = 16,
    ProjectImportArchive = 17,
    TargetSkipped = 18,
    PropertyReassignment = 19,
    UninitializedPropertyRead = 20,
    EnvironmentVariableRead = 21,
    PropertyInitialValueSet = 22,
    NameValueList = 23,
    String = 24,
    TaskParameter = 25,
    ResponseFileUsed = 26,
    AssemblyLoad = 27,
    BuildCheckMessage = 28,
    BuildCheckWarning = 29,
    BuildCheckError = 30,
    BuildCheckTracing = 31,
    BuildCheckAcquisition = 32,
    BuildSubmissionStarted = 33,
    BuildCanceled = 34,
}

impl BinaryLogRecordKind {
    /// Try to convert a raw `i32` value into a `BinaryLogRecordKind`.
    ///
    /// Returns `None` if the value is not a recognised record kind.
    pub fn from_raw(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::EndOfFile),
            1 => Some(Self::BuildStarted),
            2 => Some(Self::BuildFinished),
            3 => Some(Self::ProjectStarted),
            4 => Some(Self::ProjectFinished),
            5 => Some(Self::TargetStarted),
            6 => Some(Self::TargetFinished),
            7 => Some(Self::TaskStarted),
            8 => Some(Self::TaskFinished),
            9 => Some(Self::Error),
            10 => Some(Self::Warning),
            11 => Some(Self::Message),
            12 => Some(Self::TaskCommandLine),
            13 => Some(Self::CriticalBuildMessage),
            14 => Some(Self::ProjectEvaluationStarted),
            15 => Some(Self::ProjectEvaluationFinished),
            16 => Some(Self::ProjectImported),
            17 => Some(Self::ProjectImportArchive),
            18 => Some(Self::TargetSkipped),
            19 => Some(Self::PropertyReassignment),
            20 => Some(Self::UninitializedPropertyRead),
            21 => Some(Self::EnvironmentVariableRead),
            22 => Some(Self::PropertyInitialValueSet),
            23 => Some(Self::NameValueList),
            24 => Some(Self::String),
            25 => Some(Self::TaskParameter),
            26 => Some(Self::ResponseFileUsed),
            27 => Some(Self::AssemblyLoad),
            28 => Some(Self::BuildCheckMessage),
            29 => Some(Self::BuildCheckWarning),
            30 => Some(Self::BuildCheckError),
            31 => Some(Self::BuildCheckTracing),
            32 => Some(Self::BuildCheckAcquisition),
            33 => Some(Self::BuildSubmissionStarted),
            34 => Some(Self::BuildCanceled),
            _ => None,
        }
    }

    /// Returns `true` for auxiliary/meta-data records that do not produce build
    /// events (String, NameValueList, ProjectImportArchive).
    pub fn is_auxiliary(self) -> bool {
        matches!(
            self,
            Self::String | Self::NameValueList | Self::ProjectImportArchive
        )
    }
}

#[cfg(test)]
mod tests;
