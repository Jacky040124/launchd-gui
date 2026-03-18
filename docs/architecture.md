# Architecture Overview

LaunchPad is split into three layers to keep launchd logic testable and UI lightweight.

## 1) Domain Layer (`src/domain`)

- `job.rs`
  - `JobScope`: UserAgent / GlobalAgent / SystemDaemon
  - `JobSummary`: UI-facing aggregate model (includes `is_starred`)
  - `JobCapabilities`: `can_trigger`, `can_delete` with reason messages
- `job_detail.rs`
  - `JobRuntimeDetails`: on-demand runtime diagnostics (`pid`, `last_exit_status`, `last_run`, `raw_hint`)
- `status.rs`
  - Normalized status enum and parser from `launchctl print` output
- `action.rs`
  - Trigger action enum (`start`, `stop`, `kickstart`, `enable`, `disable`, `load`, `unload`)
- `filter.rs`
  - advanced filter model (status + tri-state attribute filters)
- `plist_document.rs`
  - standard plist editor document model
  - editor mode enum and documented key definitions
  - searchable 36+ key definitions and standard-key classifier

This layer contains no process execution and is suitable for deterministic tests.

## 2) Adapter Layer (`src/adapter`)

- `fs_scan.rs`
  - scans known launchd directories and returns plist candidates
- `plist_reader.rs`
  - extracts `Label` from plist files
  - extracts lightweight metadata (`RunAtLoad`, `KeepAlive`, `Disabled`)
- `plist_doc.rs`
  - loads/saves standard plist editor model
  - serializes XML preview
  - preserves expert string key entries for advanced launchd options
- `log_stream.rs`
  - wraps `log show` for history logs and `log stream` for short live capture (macOS)
- `ai/`
  - provider abstraction and provider-specific adapters
  - includes Claude Agent sidecar bridge and provider stubs
- `quicklaunch.rs`
  - QuickLaunch menu-bar provider abstraction
  - includes no-op provider, helper-bridge provider (`--sync-json` contract),
    and file snapshot provider (for external menu helper polling)
- `launchctl.rs`
  - wraps `launchctl` command invocations and error normalization
  - parses `launchctl list` bulk status output
  - parses `launchctl print` best-effort runtime details
- `fs_ops.rs`
  - file deletion abstraction
- `star_store.rs`
  - persistent star store abstraction + JSON implementation
- `clipboard.rs`
  - clipboard abstraction (`pbcopy` on macOS)

Adapters are trait-based, so tests can inject mock behavior.

## 3) Service Layer (`src/service`)

- `job_service.rs`
  - orchestration for scan + read label + bulk status query + capability derivation
  - on-demand runtime detail fetch per selected job
- `action_service.rs`
  - validates action capability and executes trigger commands
  - supports start/stop/kickstart/enable/disable/load/unload
- `delete_service.rs`
  - safe deletion flow:
    1. capability check
    2. `bootout`
    3. delete plist
- `star_service.rs`
  - loads persisted stars
  - toggles star/unstar and saves state
  - applies star state to job list models
- `plist_service.rs`
  - standard editor load/save orchestration
  - new plist creation in allowed scope directories
  - XML preview and input parsing helpers (env + expert key/value entries)
- `diagnostic_service.rs`
  - static rule analysis for plist validity and safety hints
- `log_service.rs`
  - recent-log query orchestration for selected jobs
- `ai_service.rs`
  - AI suggestion orchestration (sidecar-first with local heuristic fallback)
- `quicklaunch_service.rs`
  - prepares and syncs QuickLaunch item set (supports starred-only + max-count + group-by policy)

## UI Layer (`ui/main.slint` + `ui/components/*` + `src/main.rs`)

Slint provides a minimal desktop UI:

- virtualized list of jobs (`ListView`)
- selected job details + expandable advanced diagnostics
- star/unstar + starred-only filter
- advanced attribute filters (status/disabled/run-at-load/keep-alive/has-error)
- embedded plist editor with real-time XML preview
- real-time diagnostics panel for editor changes
- new user/global plist creation flow
- built-in recent log viewer for selected job
- history/live log mode toggle with configurable stream seconds
- AI suggestion panel for natural-language launchd guidance
- command palette for keyboard-first actions
- copy details action
- action buttons
- status message area

`ui/main.slint` now acts as composition root, while reusable sections are split into `ui/components/*`:
- `top_command_bar.slint`
- `search_scope_bar.slint`
- `status_filter_bar.slint`
- `jobs_workspace.slint`
- `action_bar.slint`
- `editor_logs_panel.slint`
- `editor_ai_panel.slint`
- `editor_key_panel.slint`
- `editor_main_panel.slint`
- `delete_confirm_panel.slint`
- `status_footer_panel.slint`

`main.rs` wires UI callbacks into service calls and refresh logic.

## Helper Binaries

- `src/bin/quicklaunch_helper.rs`
  - bridge helper that accepts `--sync-json` payloads
  - supports `--list` and `--summary` for external menu integrations

## Safety Boundaries

- System daemons are treated as read-only by default.
- Delete requires capability to be true.
- UI uses two-step confirmation before delete execution.
