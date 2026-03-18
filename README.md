# LaunchPad

A modern, lightweight, minimalist GUI hub for managing `launchd` jobs on macOS.

## Current Scope

LaunchPad currently includes these core capabilities:

1. **Status read** for jobs under:
   - `~/Library/LaunchAgents`
   - `/Library/LaunchAgents`
   - `/Library/LaunchDaemons` (read-only)
   - with built-in **search + scope filters** (`All/User Agents/Global Agents/Daemons`)
2. **Trigger actions**:
   - `start`
   - `stop`
   - `kickstart`
   - `enable`
   - `disable`
   - `load` (`bootstrap`)
   - `unload` (`bootout`)
3. **Delete flow** with safety guard:
   - two-step confirmation
   - capability checks (scope + file permission)
4. **Job details + diagnostics**:
   - fast list status via `launchctl list`
   - on-demand deep details via `launchctl print`
   - runtime hints including `pid`, `last exit`, `last run` (best-effort)
5. **Starred jobs + persistence**:
   - star/unstar selected jobs
   - optional starred-only filter
   - persistent star store in user config
6. **Copy selected job details**:
   - multi-line diagnostic format for quick sharing/debugging
7. **Advanced attribute filters**:
   - status filter (`running/loaded/disabled/unknown`)
   - tri-state attribute filters (`Disabled`, `RunAtLoad`, `KeepAlive`, `HasError`)
8. **Plist editor + creation flow (standard mode)**:
   - edit common keys: `Label`, `Program`, `ProgramArguments`, `RunAtLoad`, `KeepAlive`,
     `StartInterval`, `WorkingDirectory`, `EnvironmentVariables`
   - real-time XML preview while editing
   - real-time diagnostics with issue descriptions and fix suggestions
   - create new user/global plist from the app
   - optional post-save actions: `Save+Load`, `Save+Load+Enable`
   - expert key entries (`KEY=VALUE`) for advanced/undocumented string keys
   - searchable key panel (36+ documented launchd keys, click to inject templates)
9. **Built-in log viewer (selected job)**:
   - fetch recent logs by selected label (no need to open Console.app)
   - history mode and short live-capture mode
10. **AI suggestion panel (native workflow scaffold)**:
   - natural-language prompt for launchd edits
   - Claude Agent sidecar integration hook
   - local heuristic fallback when remote provider unavailable
   - in-app provider switching (openai/openrouter/lm-studio/ollama/xai/anthropic/google/claude-sidecar)
   - patch diff preview + explicit confirm before applying edits to editor
   - provider HTTP calls for OpenAI-compatible, Anthropic, and Google (configurable endpoint/model)
11. **Command palette (Raycast-style interaction)**:
   - run commands from a single input (`refresh`, `start`, `stop`, `restart`, `enable`, `new user`, `save`, `save load`, `save load enable`, `logs`, `logs live`, `logs history`)
   - batch starred controls (`start starred`, `stop starred`, `enable starred`, etc.)
   - AI provider commands (`providers`, `provider <name>`)

## Tech Stack

- **Rust**
- **Slint** (native desktop UI, no web runtime)
- `plist` crate for plist parsing and XML serialization
- `launchctl` command adapter for status/actions
- pluggable AI provider abstraction (sidecar-first)

## Project Structure

```text
src/
  adapter/         # filesystem + launchctl + plist adapters
  domain/          # job/status/action models
  service/         # business orchestration (list/trigger/delete)
  main.rs          # Slint UI wiring + callbacks
ui/
  main.slint       # composition root UI
  components/      # reusable raycast-style sections
  theme/           # shared visual tokens
tests/
  *_tests.rs       # integration tests with mocks
```

## Development

### Requirements

- Rust stable (project validated on `rustc 1.94+`)
- Linux build dependencies for CI/local check:
  - `pkg-config`
  - `libfontconfig1-dev`
- Optional AI environment variables:
  - `LAUNCHPAD_AI_PROVIDER` (default AI provider)
  - `LAUNCHPAD_CLAUDE_SIDECAR` (Claude sidecar executable path)
- Optional QuickLaunch environment variables:
  - `LAUNCHPAD_QUICKLAUNCH_ENABLE` (`true/false`, default false)
  - `LAUNCHPAD_QUICKLAUNCH_STARRED_ONLY` (`true/false`, default true)
  - `LAUNCHPAD_QUICKLAUNCH_MAX_ITEMS` (default 12)
  - `LAUNCHPAD_QUICKLAUNCH_GROUP_BY` (`scope` / `status` / `starred-scope`, default `scope`)
  - `LAUNCHPAD_QUICKLAUNCH_HELPER` (optional helper executable receiving `--sync-json`)
  - when helper is not set and QuickLaunch is enabled, LaunchPad writes sync snapshot to:
    `~/.config/launchpad/quicklaunch-items.json`

### QuickLaunch Helper CLI

The repository now includes `quicklaunch_helper` binary for bridge mode:

```bash
cargo run --bin quicklaunch_helper -- --sync-json   # reads JSON from stdin
cargo run --bin quicklaunch_helper -- --list
cargo run --bin quicklaunch_helper -- --list-json
cargo run --bin quicklaunch_helper -- --summary
```

QuickLaunch payload now includes group + starred + last-updated timestamp for real-time menu indicators.
Helper command queue is also supported for external menu integrations:

```bash
cargo run --bin quicklaunch_helper -- --enqueue-action start id-1,id-2
cargo run --bin quicklaunch_helper -- --enqueue-group-action user-agent stop
cargo run --bin quicklaunch_helper -- --enqueue-group-action global-agent restart
cargo run --bin quicklaunch_helper -- --enqueue-starred-action disable
cargo run --bin quicklaunch_helper -- --drain-actions
```

When QuickLaunch sync is enabled, LaunchPad drains queued actions on refresh and executes them through the same action service used by the main UI.

For macOS native menu-bar control, the repository also includes:

```bash
cargo run --bin quicklaunch_menubar
```

`quicklaunch_menubar` reads summary from `quicklaunch_helper` and can execute starred/group batch actions directly from the menu bar via `launchctl`.

### Run

```bash
cargo run
```

### Validate

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

## Runtime Notes

- Real launchd behavior can only be fully validated on **macOS**.
- On Linux, tests validate parsing/flow/control logic through mocks.
- System daemons are intentionally non-destructive (read-only capabilities).
- Clipboard copy uses `pbcopy` on macOS builds.

## Performance Strategy

LaunchPad uses a layered performance approach:

1. **Renderer**:
   - macOS builds use Slint `renderer-skia` (GPU path)
   - non-macOS builds keep software renderer for CI/dev portability
2. **Virtualized list UI**:
   - job list rendered with Slint `ListView` to avoid creating all row widgets at once
3. **Cheaper refresh path**:
   - refresh uses one `launchctl list` call for bulk status
   - expensive `launchctl print` is deferred to selected-job details only

## Known Limitations

- No full expert editor for arbitrary nested keys yet (standard editor first).
- AI providers are scaffolded; remote API wiring is still being expanded.
- QuickLaunch menubar integration is scaffolded but not active yet in this build.
- `launchctl` output formats can vary across macOS versions; detail parsing is resilient but best-effort.

## Next Roadmap

- Add expert plist editor with arbitrary key injection panel (36+ documented keys).
- Add richer runtime diagnostics (launchctl print + log evidence cross-linking).
- Add streaming log mode (pause/resume/follow).
- Add AI-native workflow for natural-language create/edit with reviewable diffs.
- Expand macOS-only integration tests for real `launchctl` workflows.

## License

MIT
