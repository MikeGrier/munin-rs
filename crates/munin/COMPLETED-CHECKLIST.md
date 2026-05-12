<!-- Copyright (c) Michael Grier -->
# Munin — Completed Checklist

## Moved 2026-04-23 — M1: Foundation + M2: String and NameValueList Tables

### M1: Foundation

- [x] MN-1: Create error types (`MuninError` enum) and `Result<T>` alias
- [x] MN-2: Define `BinaryLogRecordKind` enum (`#[repr(u8)]`, values 0–34)
- [x] MN-3: Define `BuildEventArgsFieldFlags` bitflags (17 flags, `u32`)
- [x] MN-4: Implement 7-bit variable-length integer reader and .NET-style string reader
- [x] MN-5: Implement binlog header reader (version, min reader version, GZip decompression setup)

### M2: String and NameValueList Tables

- [x] MN-6: Implement string table (sequential index assignment, index-based lookup)
- [x] MN-7: Implement NameValueList table (key-value index pairs, index-based lookup)
- [x] MN-8: Implement `BuildEventContext` reader (7 fields)
- [x] MN-9: Implement `BuildEventArgsFields` reader (flag-based field presence decoding)
- [x] MN-10: Implement primitive type readers (bool, DateTime, TimeSpan, Guid)

## Moved 2026-04-23 — M3: Core Event Types and Readers

- [x] MN-11: Build/Project lifecycle event types and readers (BuildStarted, BuildFinished, ProjectStarted, ProjectFinished)
- [x] MN-12: Target lifecycle event types and readers (TargetStarted, TargetFinished, TargetSkipped)
- [x] MN-13: Task lifecycle event types and readers (TaskStarted, TaskFinished, TaskCommandLine, TaskParameter)
- [x] MN-14: Diagnostic event types and readers (Error, Warning, Message, CriticalBuildMessage)
- [x] MN-15: Evaluation event types and readers (ProjectEvaluationStarted, ProjectEvaluationFinished)

## Moved 2026-04-24 — M4: Remaining Event Types and Readers

- [x] MN-16: Property event types and readers (PropertyReassignment, UninitializedPropertyRead, PropertyInitialValueSet)
- [x] MN-17: Environment/Response event types and readers (EnvironmentVariableRead, ResponseFileUsed)
- [x] MN-18: Assembly/Import event types and readers (AssemblyLoad, ProjectImported)
- [x] MN-19: BuildCheck event types and readers (BuildCheckMessage/Warning/Error/Tracing/Acquisition)
- [x] MN-20: Remaining event types and readers (BuildSubmissionStarted, BuildCanceled)

## Moved 2026-04-24 — M5: Sequential Reader API

- [x] MN-21: Implement `BinlogReader` — sequential record reader over a `Read` stream
- [x] MN-22: Implement event dispatching (record kind → typed event deserialization)
- [x] MN-23: Implement forward-compatibility (skip unknown events/parts for version ≥ 18)
- [x] MN-24: Implement embedded content handling (ProjectImportArchive zip extraction)
- [x] MN-25: Integration test with a real `.binlog` file (generate one via `dotnet build /bl`)

## Moved 2026-04-23 — M6: Seekable Data Model

- [x] MN-26: Define the seekable data model — `BinlogIndex` with event metadata and byte offsets
- [x] MN-27: First-pass indexing: read all events, build offset table and event type index
- [x] MN-28: Random access: seek to event by index, deserialize on demand
- [x] MN-29: Query API: filter events by type, project, target, task
- [x] MN-30: Integration test: seekable random-access patterns over a real `.binlog` file
