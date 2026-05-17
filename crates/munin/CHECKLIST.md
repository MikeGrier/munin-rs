<!-- Copyright (c) Michael Grier -->
# Munin — Checklist

## Issue #14 — Bounds-check parsed `i32` lengths/counts before casting to `usize`

A malformed binlog with a negative length/count value sign-extends through
`as usize` to a huge value, causing `Vec::with_capacity` / `vec![0u8; n]` to
OOM or panic. Validate at every cast site so the parser returns
`MuninError::InvalidFormat` instead.

### Milestones

- [x] **M1 — Helper.** Add `read_7bit_length(reader, what: &'static str) -> Result<usize, MuninError>` in `primitives.rs` that reads a 7-bit varint and rejects negatives with `MuninError::InvalidFormat`. In `index.rs` the record-length path uses `read_7bit_int_counted` (to also capture bytes consumed) and validates the sign inline with the same logic.
- [x] **M2 — Replace call sites.** Swap `read_7bit_int(...)? as usize` for `read_7bit_length(...)?` at:
  - `reader.rs` — record length, NVL pair count
  - `index.rs` — record length (validated inline since the counted helper returns bytes-consumed too), NVL pair count
  - `primitives.rs` — `read_dotnet_string` length (refactored to use helper)
  - `readers.rs` — legacy string-dictionary count
  - `fields.rs` — arguments count
  - `events.rs` — profile entry count, string list count, task item count, item group count
- [x] **M3 — Tests.** Unit tests for `read_7bit_length` rejecting negatives (including `i32::MIN`) and a `read_legacy_string_dictionary` test that confirms a negative count is rejected with `MuninError::InvalidFormat`.
- [x] **M4 — Verify.** `cargo_check`, `cargo_test`, `cargo_clippy` all clean.
- [x] **M5 — Commit and PR.** Conventional commit `fix(parser): reject negative lengths/counts in binlog payloads (#14)`.
