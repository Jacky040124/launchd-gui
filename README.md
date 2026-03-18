# LaunchPad - Modern macOS launchd Manager

A free, open-source GUI tool for managing macOS launchd services.

## Problem

macOS launchd is powerful but notoriously difficult to use. Existing tools like LaunchControl and Lingon X are paid (~$15) and feel dated. There's no good free, modern alternative.

## Features (Planned)

- **Visual Dashboard** - See all your launch agents & daemons at a glance
- **Job Status** - Real-time status (running, scheduled, failed, disabled)
- **Form-based Editor** - Create/edit plist files without touching XML
- **Log Viewer** - Real-time log streaming for debugging
- **One-click Actions** - Enable/disable/start/stop services instantly
- **Templates** - Quick-start templates for common tasks:
  - Run scripts on schedule (daily, weekly, etc.)
  - Launch apps at startup
  - Watch folders for changes
  - Periodic cleanup tasks
- **Menu Bar Access** - Quick access to common actions

## Scope

- User launch agents (`~/Library/LaunchAgents`)
- Global launch agents (`/Library/LaunchAgents`)
- System daemons (`/Library/LaunchDaemons`) - view only

## Tech Stack

TBD - Considering:
- **Swift + SwiftUI** - Native macOS, best performance
- **Tauri + React** - Lighter weight, modern web stack
- **Electron** - Fastest to prototype

## Status

🚧 **Planning phase** - Contributions and ideas welcome!

## License

MIT
