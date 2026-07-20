---
id: TASK-69.2
title: Versioned NNUE network file format and loader
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-20 19:40'
updated_date: '2026-07-20 23:23'
labels:
  - nnue
  - inference
dependencies:
  - TASK-69.1
parent_task_id: TASK-69
priority: high
ordinal: 104000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement reading and writing the versioned network file defined by the design contract (TASK-69.1), in the engine crate. The loader parses the header, validates the architecture parameters and quantization scales against what the build supports, and refuses a file it does not understand rather than misinterpreting one. No inference and no accumulator here — this task is purely the serialization boundary and its guarantees, so it can land and be reviewed on its own.

The file is the contract between the Rust engine and the Python trainer: the trainer writes it, the engine reads it, and nothing else is allowed to carry weights across the language boundary.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A network file with a valid header round-trips (write then read) to identical weights and metadata
- [ ] #2 A file whose version, architecture parameters, or quantization scales are unknown or unsupported is rejected with a clear typed error, never silently misread
- [ ] #3 Tests cover a valid file, a truncated file, an unknown-version file, and an architecture-mismatch file
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a pub 'nnue' module to the engine crate (engine/src/nnue/{mod.rs,format.rs}) sibling to eval, per the design contract.
2. format.rs: define the SBNN 64-byte little-endian header constants and a validated in-memory Network type (hidden width H, qa, qb, scale, and the four quantized weight blocks W_ft/b_ft/W_out/b_out) with a checked constructor and read accessors.
3. Implement write<W: Write>: emit the 64-byte header (magic, version=1, feature_set=0, input_dim=768, H, output_dim=1, activation=0, qa, qb, scale, param_bytes, FNV-1a param_hash, zeroed reserved) then the parameter blob in contract order.
4. Implement read<R: Read> with a typed LoadError enum covering all 9 rejection rules as distinct variants (truncated header/blob, trailing bytes, bad magic, unsupported version, unsupported feature_set/activation, input_dim/H/output_dim mismatch, non-positive qa/qb/scale, reserved non-zero, param_bytes disagreement, hash mismatch). Validate the full header before allocating/interpreting any weights; read the blob with take() to avoid pre-allocating on an untrusted size.
5. Tests: valid round-trip (write->read identical weights+metadata), truncated file, unknown-version file, architecture-mismatch (H not multiple of 16 / input_dim wrong / output_dim wrong), plus bad magic, non-positive scale, reserved non-zero, param_bytes mismatch, hash mismatch, trailing bytes.
6. Run cargo fmt --check, clippy -D warnings, cargo test --workspace; hand off for review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Claimed on branch task-69.2-nnue-file-format (worktree /Users/seabo/seaborg-worktrees/task-69.2-nnue-file-format), base 6d3d4ac.

Implemented the SBNN network file format in a new engine 'nnue' module (engine/src/nnue/{mod.rs,format.rs}), registered as pub in engine/src/lib.rs sibling to eval. This is the serialization boundary only — no inference or accumulator.

- format.rs defines the 64-byte little-endian header layout as named offset constants, a validated in-memory Network (hidden width H, qa/qb/scale, and the four quantized blocks W_ft i16 / b_ft i16 / W_out i16 / b_out i32) with read accessors, a public Parameters carrier so Network::new stays under the argument-count lint, a BuildError for in-memory construction, and a LoadError whose variants map one-to-one onto the contract's rejection rules.
- write() emits magic SBNN, version 1, feature_set 0, input_dim 768, H, output_dim 1, activation 0, qa, qb, scale, param_bytes, an FNV-1a param_hash, and zeroed reserved bytes, then the blob in contract order.
- read() validates the entire header (magic, version, feature_set/activation, input_dim/H/output_dim, positive qa/qb/scale, zero reserved, param_bytes-vs-dimensions) before touching weights, then reads the blob via Read::take so an untrusted param_bytes cannot pre-size an allocation, rejects trailing bytes, and checks the FNV-1a hash before decoding.
- 17 unit tests: valid round-trip to identical weights+metadata; truncated header and truncated blob; trailing bytes; bad magic; unknown version; unknown feature-set; unknown activation; input-dim mismatch; H not a multiple of 16; zero H; wrong output_dim; non-positive qa/qb/scale; non-zero reserved; param_bytes disagreement; corrupt-blob hash mismatch; empty input; and BuildError paths for Network::new.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-20 23:23
---
Implementation handoff
Branch: task-69.2-nnue-file-format
Worktree: /Users/seabo/seaborg-worktrees/task-69.2-nnue-file-format
Base: 6d3d4ac98a40a455959b4cea18d0b0a82b0c7867
Implementation target: 346db314adb4418c688430ea83c45fa01fe56c50
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean, exit 0)
- cargo test --workspace: pass (457 passed, 0 failed, 2 ignored; engine 320 incl. 17 new nnue::format tests)
Known failures: none
---
<!-- COMMENTS:END -->
