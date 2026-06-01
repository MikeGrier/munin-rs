<!-- Copyright (c) Michael Grier -->
# Munin — Design Notes

## Purpose

Munin reads and decodes MSBuild binary log (`.binlog`) files and provides a seekable
data model over the recorded build events. Named after Odin's raven who watches the
world and brings back memories — we watch recorded builds and bring back structured
data.

## Decision Index

| ID | Decision | Section |
|----|----------|---------|
| D-1 | File format version scope | §Format Scope |
| D-2 | Owned data model, not streaming callbacks | §Data Model Strategy |
| D-3 | Our specification of the binlog format | §Binlog Format Specification |

---

### D-1: Format Scope

We target binlog format version 18 through 25 (current as of MSBuild 17.x). Version 18
introduced forward-compatible reading with record lengths, making it the natural minimum.
Versions below 18 lack record lengths and cannot be safely skipped — we do not support
them.

The reference implementation is the C# code in `dotnet/msbuild`:
- `BinaryLogger.cs` — writer, version constants
- `BuildEventArgsReader.cs` — reader, deserialization
- `BuildEventArgsWriter.cs` — writer, serialization
- `BinaryLogRecordKind.cs` — record type enum

We chose to match the reference implementation's behavior because it is sensible for our
users. Our specification is written down here; if a future MSBuild version changes
behavior, we update our specification explicitly.

### D-2: Data Model Strategy

We provide an **owned data model** rather than streaming callbacks. The reader produces
a structured representation of the entire build log (or a filtered subset) that can be
queried, indexed, and navigated randomly. This is the "seekable" part of our purpose.

The streaming/callback approach (like `BinaryLogReplayEventSource` in C#) is useful for
replay but forces callers to maintain their own state. Since our use cases center on
analysis and querying, an owned indexed model is the right default.

### D-3: Binlog Format Specification

This section is our specification. The reference implementation was used to derive it.
Changing values in this specification is a breaking change.

#### File structure

The entire `.binlog` file is a GZip-compressed stream. The decompressed content
begins with:

1. **Header** (first 8 decompressed bytes):
   - 4 bytes: file format version (`i32`, little-endian). Current: 25.
   - 4 bytes: minimum reader version (`i32`, little-endian). Current: 18.
     (Present only for version ≥ 18.)

2. **Record stream**: The remainder of the decompressed stream consists of
   serialized build event records.

#### Record framing

Each record in the decompressed stream:
1. Record kind: 7-bit variable-length encoded `i32` (`BinaryLogRecordKind`).
2. Record length: 7-bit variable-length encoded `i32` (byte count of the payload).
   Present only for versions ≥ 18.
3. Payload: `length` bytes of serialized event data.

The stream ends when `EndOfFile` (0) is encountered as a record kind.

#### 7-bit variable-length integer encoding

Integers are encoded as a sequence of bytes where the low 7 bits of each byte carry
data and the high bit signals whether more bytes follow (1 = more, 0 = last).
This is the same encoding as .NET `BinaryWriter.Write7BitEncodedInt`.

#### Record kinds

```
EndOfFile                = 0
BuildStarted             = 1
BuildFinished            = 2
ProjectStarted           = 3
ProjectFinished          = 4
TargetStarted            = 5
TargetFinished           = 6
TaskStarted              = 7
TaskFinished             = 8
Error                    = 9
Warning                  = 10
Message                  = 11
TaskCommandLine          = 12
CriticalBuildMessage     = 13
ProjectEvaluationStarted = 14
ProjectEvaluationFinished= 15
ProjectImported          = 16
ProjectImportArchive     = 17
TargetSkipped            = 18
PropertyReassignment     = 19
UninitializedPropertyRead= 20
EnvironmentVariableRead  = 21
PropertyInitialValueSet  = 22
NameValueList            = 23
String                   = 24
TaskParameter            = 25
ResponseFileUsed         = 26
AssemblyLoad             = 27
BuildCheckMessage        = 28
BuildCheckWarning        = 29
BuildCheckError          = 30
BuildCheckTracing        = 31
BuildCheckAcquisition    = 32
BuildSubmissionStarted   = 33
BuildCanceled            = 34
```

#### Auxiliary (meta-data) records

Three record kinds are "auxiliary" and do not produce build events:
- `String` (24) — a deduplicated string. Payload is raw UTF-8 bytes; the record
  length provides the string length (no dotnet length prefix). Strings are assigned
  sequential indices starting at 10 (0 = null, 1 = empty, 2–9 reserved).
- `NameValueList` (23) — a deduplicated list of (key-index, value-index) pairs.
  Payload: record length (7-bit int, version ≥ 18), count (7-bit int), then
  count × (key-index, value-index) pairs (each 7-bit int). Indices start at 10.
- `ProjectImportArchive` (17) — an embedded zip archive of project/import files.

These are interleaved with event records and must be ingested before the event
that references them.

#### BuildEventArgsFieldFlags (bitflags)

```
None            = 0x0000
BuildEventContext = 0x0001
HelpKeyword     = 0x0002
Message         = 0x0004
SenderName      = 0x0008
ThreadId        = 0x0010
Timestamp       = 0x0020
Subcategory     = 0x0040
Code            = 0x0080
File            = 0x0100
ProjectFile     = 0x0200
LineNumber      = 0x0400
ColumnNumber    = 0x0800
EndLineNumber   = 0x1000
EndColumnNumber = 0x2000
Arguments       = 0x4000
Importance      = 0x8000
Extended        = 0x10000
```

#### BuildEventContext

Seven 7-bit encoded `i32` fields, always in this order:
1. NodeId
2. ProjectContextId
3. TargetId
4. TaskId
5. SubmissionId
6. ProjectInstanceId
7. EvaluationId (version > 1)

#### Primitive types

| Type | Encoding |
|------|----------|
| `i32` | 7-bit variable-length |
| `i64` | 8 bytes, little-endian |
| `bool` | 1 byte |
| `DateTime` | `i64` ticks + `i32` DateTimeKind |
| `TimeSpan` | `i64` ticks |
| `Guid` | 16 bytes |
| `string` (raw) | .NET BinaryWriter format: 7-bit-length-prefixed UTF-8 |
| `string` (dedup) | 7-bit-int index into string table |
