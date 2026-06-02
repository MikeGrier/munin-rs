// Copyright (c) Michael Grier

//! Dumper: convert a [`BinlogIndex`] to a [`JsonlogFile`] on the wire.
//!
//! Walks the index in stored order, mirrors the header, string and
//! name-value-list tables and embedded archives, then emits each event
//! either as a fully decoded JSON object (when the current decoder
//! understands the record kind) or as an opaque `payload_b64`
//! fallback. See `D-JL-2` in the workspace `DESIGN-NOTES.md`.

use std::io::Write;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use crate::{
    error::MuninError,
    index::BinlogIndex,
    jsonlog::schema::{
        ArchiveB64, JsonlogEvent, JsonlogEventBody, JsonlogFile, JsonlogHeader,
        MUNIN_JSONLOG_VERSION,
    },
    reader::BinlogEvent,
};

/// Build a [`JsonlogFile`] mirror of `index` in memory.
///
/// On decode success the event body is `decoded`; on decode failure
/// the body is `payload_b64` of the stored payload bytes.
pub fn build(index: &BinlogIndex) -> Result<JsonlogFile, MuninError> {
    let header: JsonlogHeader = (*index.header()).into();

    let strings = index.strings().entries().to_vec();

    let name_value_lists = index
        .nvl_table()
        .entries()
        .iter()
        .map(|list| {
            list.iter()
                .map(|pair| [pair.key_index as u32, pair.value_index as u32])
                .collect()
        })
        .collect();

    let archives = index
        .archives()
        .iter()
        .map(|bytes| ArchiveB64 {
            data_b64: BASE64.encode(bytes),
        })
        .collect();

    let mut events = Vec::with_capacity(index.len());
    for (i, meta) in index.iter_meta() {
        let kind = format!("{:?}", meta.record_kind);
        let body = match index.get(i) {
            Ok(Some(event)) => match event_to_value(&event) {
                Ok(v) => JsonlogEventBody::Decoded(v),
                Err(_) => fallback_b64(index, i),
            },
            Ok(None) | Err(_) => fallback_b64(index, i),
        };
        events.push(JsonlogEvent {
            kind,
            byte_offset: meta.byte_offset,
            body,
        });
    }

    Ok(JsonlogFile {
        munin_jsonlog_version: MUNIN_JSONLOG_VERSION,
        header,
        strings,
        name_value_lists,
        archives,
        events,
    })
}

/// Dump `index` as a compact JSON `.jsonlog` document to `writer`.
pub fn dump_index<W: Write>(index: &BinlogIndex, writer: W) -> Result<(), MuninError> {
    let file = build(index)?;
    serde_json::to_writer(writer, &file)
        .map_err(|e| MuninError::InvalidFormat(format!("jsonlog serialize: {e}")))
}

/// Dump `index` as a pretty-printed JSON `.jsonlog` document.
pub fn dump_index_pretty<W: Write>(index: &BinlogIndex, writer: W) -> Result<(), MuninError> {
    let file = build(index)?;
    serde_json::to_writer_pretty(writer, &file)
        .map_err(|e| MuninError::InvalidFormat(format!("jsonlog serialize: {e}")))
}

fn fallback_b64(index: &BinlogIndex, i: usize) -> JsonlogEventBody {
    let bytes = index.payload_bytes(i).unwrap_or(&[]);
    JsonlogEventBody::PayloadB64(BASE64.encode(bytes))
}

fn event_to_value(event: &BinlogEvent) -> Result<serde_json::Value, serde_json::Error> {
    macro_rules! arm {
        ($($variant:ident),* $(,)?) => {
            match event {
                $( BinlogEvent::$variant(e) => serde_json::to_value(e), )*
            }
        };
    }
    arm!(
        BuildStarted,
        BuildFinished,
        ProjectStarted,
        ProjectFinished,
        TargetStarted,
        TargetFinished,
        TargetSkipped,
        TaskStarted,
        TaskFinished,
        TaskCommandLine,
        TaskParameter,
        Error,
        Warning,
        Message,
        CriticalBuildMessage,
        ProjectEvaluationStarted,
        ProjectEvaluationFinished,
        PropertyReassignment,
        UninitializedPropertyRead,
        PropertyInitialValueSet,
        EnvironmentVariableRead,
        ResponseFileUsed,
        AssemblyLoad,
        ProjectImported,
        BuildCheckMessage,
        BuildCheckWarning,
        BuildCheckError,
        BuildCheckTracing,
        BuildCheckAcquisition,
        BuildSubmissionStarted,
        BuildCanceled,
    )
}
