# LaunchPad

A modern, lightweight, minimalist GUI hub for managing `launchd` jobs on macOS.

## Current MVP Scope

LaunchPad currently focuses on three core operations:

1. **Status read** for jobs under:
   - `~/Library/LaunchAgents`
   - `/Library/LaunchAgents`
   - `/Library/LaunchDaemons` (read-only)
   - with built-in **search + scope filters** (`All/User Agents/Global Agents/Daemons`)
2. **Trigger actions**:
   - `start`
   - `stop`
   - `kickstart`
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

The app intentionally does **not** include job creation/editing in this iteration.

## Tech Stack

- **Rust**
- **Slint** (native desktop UI, no web runtime)
- `plist` crate for parsing job labels
- `launchctl` command adapter for status/actions

## Project Structure

```text
src/
  adapter/         # filesystem + launchctl + plist adapters
  domain/          # job/status/action models
  service/         # business orchestration (list/trigger/delete)
  main.rs          # Slint UI wiring + callbacks
ui/
  main.slint       # minimalist UI
tests/
  *_tests.rs       # integration tests with mocks
```

## Development

### Requirements

- Rust stable (project validated on `rustc 1.94+`)
- Linux build dependencies for CI/local check:
  - `pkg-config`
  - `libfontconfig1-dev`

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

- No create/edit wizard for plist jobs in current MVP.
- UI currently focuses on list/detail/action flow and does not include menu bar integration.
- `launchctl` output formats can vary across macOS versions; detail parsing is resilient but best-effort.

## Next Roadmap

- Add optional plist editor for common fields (`Label`, `ProgramArguments`, schedule keys).
- Add richer status diagnostics (last exit hints, disabled reason visibility).
- Add optional log preview panel for selected jobs.
- Expand macOS-only integration tests for real `launchctl` workflows.

## License

MIT
