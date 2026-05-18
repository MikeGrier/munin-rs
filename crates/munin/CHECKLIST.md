<!-- Copyright (c) Michael Grier -->
# Munin — Checklist

## Issue #14 — Bounds-check parsed `i32` lengths/counts before casting to `usize`

A malformed binlog with a negative length/count value sign-extends through
`as usize` to a huge value, causing `Vec::with_capacity` / `vec![0u8; n]` to
OOM or panic. Validate at every cast site so the parser returns
`MuninError::InvalidFormat` instead.

### Milestones

- [x] **M1 — Helper.** Add `read_7bit_length(reader, what: &'static str) -> Result<usize, MuninError>` in `primitives.rs` that reads a 7-bit varint and rejects negatives with `MuninError::InvalidFormat`. In `index.rs` the record-length path uses `read_7bit_int_counted` (to also capture bytes consumed) and validates the sign inline with the same logic.
- [x] **M2 — Replace call sites.** Swap `read_7bit_int(...)? as usize` for `read_7bit_length(...)?` at byte-buffer length sites:
  - `reader.rs` — record length
  - `index.rs` — record length (validated inline since the counted helper returns bytes-consumed too)
  - `primitives.rs` — `read_dotnet_string` length (refactored to use helper)

  Collection-count sites (`readers.rs`, `fields.rs`, `events.rs`, `reader.rs` NVL pair count, `index.rs` NVL pair count) were initially updated here but later migrated to `read_7bit_count` in M6.
- [x] **M3 — Tests.** Unit tests for `read_7bit_length` rejecting negatives (including `i32::MIN`) and a `read_legacy_string_dictionary` test that confirms a negative count is rejected with `MuninError::InvalidFormat`.
- [x] **M4 — Verify.** `cargo_check`, `cargo_test`, `cargo_clippy` all clean.
- [x] **M5 — Commit and PR.** Conventional commit `fix(parser): reject negative lengths/counts in binlog payloads (#14)`.
- [x] **M6 — Element-count ceiling (issue #19).** Add `MAX_BINLOG_ELEMENT_COUNT` (1 M) and `read_7bit_count` helper. Swap all collection-count call sites (NVL pair count, string-dict count, event arguments count, profile/string/task/item/item-group counts in `events.rs`) from `read_7bit_length` to `read_7bit_count` so a large element count cannot trigger a multi-GiB `Vec` allocation. Tests added for zero, positive, reject-above-max, and accepts-at-max.
