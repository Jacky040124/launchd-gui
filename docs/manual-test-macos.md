# macOS Manual Test Script (MVP)

This checklist validates the current MVP scope: status read, trigger actions, delete, starred jobs, copy details, and advanced runtime details.

## Preconditions

1. Use a macOS machine with GUI session.
2. Build and run LaunchPad:
   ```bash
   cargo run
   ```
3. Prepare at least one controllable user LaunchAgent under:
   `~/Library/LaunchAgents`.

## A. Status Read

1. Launch the app.
2. Click **Refresh**.
3. Verify jobs appear from expected scopes:
   - user agents
   - global agents (if visible)
   - system daemons (visible, read-only)
4. Select a job and verify detail fields:
   - Label
   - Scope
   - Status
   - Path

Expected:
- App does not crash if some plist is malformed.
- Status text is shown (running/loaded/disabled/unknown).
- Refresh remains responsive for medium-size job lists.

## B. Trigger Actions

1. Select a controllable user agent.
2. Click **Start**.
3. Click **Refresh** and verify status change if applicable.
4. Repeat for **Stop** and **Kickstart**.
5. Click **Enable**, then **Disable** and verify status messages update.
6. Click **Load** and **Unload** and verify status messages update.

Expected:
- Status message shows command success/failure.
- Unsupported scopes keep trigger controls disabled.

## C. Delete Flow

1. Select a user agent plist you can safely remove.
2. Click **Delete** once.
3. Verify app shows a confirmation panel containing:
   - target label
   - irreversible warning text
   - **Confirm Delete** and **Cancel** buttons
4. Click **Cancel** once and verify the confirmation panel disappears.
5. Click **Delete** again, then click **Confirm Delete**.
5. Click **Refresh**.

Expected:
- Item disappears from list after successful deletion.
- If `bootout` says service is not loaded, deletion still proceeds.
- Permission failures produce explicit error message.

## D. Read-only Guardrails

1. Select a system daemon item.
2. Verify **Start/Stop/Kickstart/Delete** are disabled.

Expected:
- No destructive action allowed for system daemons.

## E. Search and Scope Filters

1. Enter a keyword in the search input (for example, part of a label).
2. Verify the list updates to matching jobs.
3. Click each scope filter button:
   - **All**
   - **User Agents**
   - **Global Agents**
   - **Daemons**
4. Verify the list updates and the filter badge text changes accordingly.

Expected:
- Search and scope filter can be combined.
- Filter status text reflects both scope and query when both are set.

## F. Starred Jobs + Starred-only Filter

1. Select a user or global agent.
2. Click **Star**.
3. Verify:
   - row displays `★`
   - the details panel button changes to **Unstar**
4. Enable **Starred only** filter.
5. Verify only starred jobs remain visible.
6. Click **Unstar** on the selected item while starred-only filter is enabled.

Expected:
- Item disappears from list after unstar when starred-only filter is on.
- Refresh preserves current star state.

## G. Copy Details + Advanced Runtime Panel

1. Select a job.
2. Click **Show Advanced**.
3. Verify advanced fields are shown:
   - PID
   - Last Exit
   - Last Run
4. Click **Copy Details**.
5. Paste into a text editor.

Expected:
- Copied payload is multi-line and human-readable.
- Includes label/scope/status/path and runtime diagnostics.
- If runtime fields are unavailable, output contains `N/A` or hint text instead of crashing.

## H. Star Persistence

1. Star one or more jobs.
2. Quit LaunchPad.
3. Launch LaunchPad again and click **Refresh**.

Expected:
- Previously starred jobs remain starred.

## I. Advanced Attribute Filters

1. Use status filter buttons:
   - **Running**
   - **Loaded**
   - **Disabled**
   - **Unknown**
2. Verify list updates according to selected status.
3. Click tri-state filter toggles repeatedly:
   - `Disabled:any -> yes -> no -> any`
   - `RunAtLoad:any -> yes -> no -> any`
   - `KeepAlive:any -> yes -> no -> any`
   - `HasError:any -> yes -> no -> any`

Expected:
- Filters can be combined with search and scope filters.
- Active filter badge reflects advanced filters.

## J. Plist Editor + New Job

1. Select an existing user agent.
2. In **Plist Editor (Standard + XML)** update one or more fields:
   - Label / Program / ProgramArguments / WorkingDirectory
   - StartInterval
   - EnvironmentVariables (`KEY=VALUE, KEY2=VALUE2`)
   - RunAtLoad / KeepAlive toggles
3. Confirm XML preview updates immediately as fields change.
4. Click **Save** and verify status message indicates success.
5. Click **New User Job** (or **New Global Job**), adjust fields, then **Save**.
6. Click **Refresh** and verify the newly created plist appears in list.

Expected:
- Editor shows validation errors for invalid values (for example malformed env pairs).
- Existing plist edits can be saved safely.
- New plist creation succeeds in allowed directories and is visible after refresh.

## K. Built-in Log Viewer

1. Select a job with known recent output.
2. In editor area, click **Refresh Logs**.
3. Verify logs appear in the **Logs** panel.
4. Switch to another job and click **Refresh Logs** again.

Expected:
- Logs update to the selected job context.
- Empty results show a friendly message instead of crashing.
- On unsupported environments, an explicit error is shown in status/log panel.
