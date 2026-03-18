# Architecture Overview

LaunchPad is split into three layers to keep launchd logic testable and UI lightweight.

## 1) Domain Layer (`src/domain`)

- `job.rs`
  - `JobScope`: UserAgent / GlobalAgent / SystemDaemon
  - `JobSummary`: UI-facing aggregate model
  - `JobCapabilities`: `can_trigger`, `can_delete` with reason messages
- `status.rs`
  - Normalized status enum and parser from `launchctl print` output
- `action.rs`
  - Trigger action enum (`start`, `stop`, `kickstart`)

This layer contains no process execution and is suitable for deterministic tests.

## 2) Adapter Layer (`src/adapter`)

- `fs_scan.rs`
  - scans known launchd directories and returns plist candidates
- `plist_reader.rs`
  - extracts `Label` from plist files
- `launchctl.rs`
  - wraps `launchctl` command invocations and error normalization
- `fs_ops.rs`
  - file deletion abstraction

Adapters are trait-based, so tests can inject mock behavior.

## 3) Service Layer (`src/service`)

- `job_service.rs`
  - orchestration for scan + read label + query status + capability derivation
- `action_service.rs`
  - validates action capability and executes trigger commands
- `delete_service.rs`
  - safe deletion flow:
    1. capability check
    2. `bootout`
    3. delete plist

## UI Layer (`ui/main.slint` + `src/main.rs`)

Slint provides a minimal desktop UI:

- list of jobs
- selected job details
- action buttons
- status message area

`main.rs` wires UI callbacks into service calls and refresh logic.

## Safety Boundaries

- System daemons are treated as read-only by default.
- Delete requires capability to be true.
- UI uses two-step confirmation before delete execution.
