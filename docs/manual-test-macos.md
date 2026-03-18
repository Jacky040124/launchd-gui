# macOS Manual Test Script (MVP)

This checklist validates the current MVP scope: status read, trigger actions, delete.

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

## B. Trigger Actions

1. Select a controllable user agent.
2. Click **Start**.
3. Click **Refresh** and verify status change if applicable.
4. Repeat for **Stop** and **Kickstart**.

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
