---
id: TASK-69.2
title: Versioned NNUE network file format and loader
status: To Do
assignee: []
created_date: '2026-07-20 19:40'
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
