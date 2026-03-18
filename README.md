# LaunchPad

A modern, lightweight, minimalist GUI hub for managing `launchd` jobs on macOS.

## Current MVP Scope

LaunchPad currently focuses on three core operations:

1. **Status read** for jobs under:
   - `~/Library/LaunchAgents`
   - `/Library/LaunchAgents`
   - `/Library/LaunchDaemons` (read-only)
2. **Trigger actions**:
   - `start`
   - `stop`
   - `kickstart`
3. **Delete flow** with safety guard:
   - two-step confirmation
   - capability checks (scope + file permission)

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

## License

MIT
