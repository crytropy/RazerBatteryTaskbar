# Seelen UI integration

This optional **Fancy Toolbar** text plugin shows Razer battery data directly
from the local RazerBatteryTaskbar API. **It does not need Seelen UI to discover
or mirror the native Windows system tray icon.** You can use the native icon and
Fancy Toolbar item independently, or keep both visible.

Nothing in this folder accesses the USB device directly.

## Data flow

```text
Razer device
    ↓
Native HID transport + Rust core (existing polling)
    ↓
FrontendSnapshot → IntegrationSnapshot v1
    ↓
http://127.0.0.1:27212/v1/status.txt   (same JSON, served as text/plain)
    ↓
Seelen Fancy Toolbar remoteData → JSON.parse inside the plugin scripts
```

The plugin fetches the **cached status API every 5 seconds**. This does **not**
trigger an HID query every 5 seconds: the native app retains its configured
polling interval (at least 60 seconds), plus the
existing resume-triggered recovery polls.

## Local setup and verification

1. Run `RazerBatteryTaskbar-Native-Preview.exe` (no arguments) on a
   supported DeathAdder V3 Pro 1532:00B7 receiver. Start the native EXE only
   once. The Windows HID transport is the default.
2. Confirm the read-only API works from PowerShell:

   ```powershell
   Invoke-RestMethod http://127.0.0.1:27212/v1/status |
     Select-Object transportReady, transportHealthy, primaryDevice
   ```

3. With Seelen UI running, open PowerShell **from the repository root** and
   load the plugin:

   ```powershell
   slu resource load plugin .\integrations\seelen-ui
   ```

4. In **Seelen UI Settings → Fancy Toolbar**, add or select the plugin named
   **Razer Battery / Razer 電量** (`@crytropy/razer-battery`). Loading a resource
   makes it available for selection; it may not place itself on the toolbar.
5. For temporary development installs, use the same path to unload:

   ```powershell
   slu resource unload plugin .\integrations\seelen-ui
   ```

**Important:** `slu resource load` registers a resource for the current Seelen
session. After restarting Seelen UI, reload the resource or install it
permanently through Seelen UI's resource settings.

## Display and recovery behavior

| API state | Toolbar text | Tooltip |
| --- | --- | --- |
| Healthy, battery = 63.92157 | 🔋 `64%` | Device name and `63.9%` |
| Healthy, battery = 0 | 🔋 `0%` | Genuine 0% reading |
| Healthy, charging | 🔋 `64% ⚡` | Charging indicator |
| Device idle/asleep (`deviceNotResponding`), last battery retained | 🔋 `64% 💤` | Idle/asleep, showing last known battery |

The 🔋 stands for a Phosphor battery glyph (`PiBatteryFullFill` above 90%,
`PiBatteryHighFill` above 66%, `PiBatteryMediumFill` above 33%,
`PiBatteryLowFill` above 5%, otherwise `PiBatteryWarning`, the same steps as
Seelen's bundled power plugin). It is wrapped in a `Group` whose colour is
white at 50% and above, orange (`#fa8c16`) below 50%, and red (`#ff4d4f`)
below 20%.
| HID read failed / `battery: null` | `Razer ?%` | Temporary failure and auto-retry |
| Missing/invalid battery response | `Razer ?%` | Battery unavailable |
| Missing receiver, service, or incompatible schema | `Razer --` | No device / unavailable |
| Placeholder transport (`--placeholder-transport` debug run) | `Razer --` | HID transport pending |

Only validated, healthy numeric readings are displayed. After a temporary
`invalidResponse` error (such as immediately after Windows resumes from
sleep), the toolbar does **not** treat a missing battery as 0% or show a stale
percentage. The next successful native poll restores the battery display.

The tooltip lists all connected Razer devices and distinguishes query failure
from physical disconnection. The native transport is still limited
to the tested receiver model, not every Razer product listed in the database.

## If the native tray icon is missing in Seelen UI

This is separate from the Fancy Toolbar plugin. Seelen UI has previously had
cases where tray icons only appeared after restarting Seelen UI:
[Seelen UI issue #1173](https://github.com/eythaann/Seelen-UI/issues/1173).

1. Query the local API. If it returns battery data, the native process and
   battery core are operating independently of the missing UI icon.
2. Check whether the native Windows tray contains the icon. Seelen documents
   `Win+B`, then `Enter` as one way to open it when the custom tray is missing.
3. Try Seelen's documented reload shortcut `Ctrl+Win+Alt+R`; it should not
   require restarting the native battery process.
4. Test **Fancy Toolbar** independently. If it displays valid battery data while
   Seelen's mirrored tray icon is absent, focus further investigation on the
   Seelen tray display rather than changing HID polling.

Upstream `tray-icon 0.26.1` already handles Windows `TaskbarCreated` to
re-register icons after Explorer restarts. Do not force re-registration on every
battery poll or restart the HID service just to work around a Seelen-specific
display refresh.

## Tests

Run the plugin logic tests with Node.js:

```powershell
node --test integrations/seelen-ui/plugin.test.js
```

These tests cover valid battery values, 0%, unavailable or invalid responses,
transport failure, recovery indicators, and disconnected service. They do not
substitute for checking the appearance of a loaded plugin in Seelen UI.

## Script compatibility (sandboxjs)

Seelen UI runs `template`, `tooltip`, and the other script fields inside
`@nyariv/sandboxjs`, not a normal browser engine. Three things differ from a
browser and each makes the toolbar item render **nothing at all**:

1. The sandbox does not reliably short-circuit `&&` / `||` before a property
   access. `!status || status.schemaVersion !== 1` throws when `status` is
   `null`, and `typeof x === "string" && x.length > 0` throws when `x` is
   `null`.
2. `remoteData` values are absent until the first fetch completes, so the very
   first render runs with `status` undefined. A script that throws on that
   first render is dropped by the toolbar and never retried.

3. JSON remote data is unusable. Seelen stores fetched data in a Svelte
   `$state` and evaluates scripts inside a `$derived`. When the response is
   `application/json`, the parsed object becomes a deep state proxy, and
   **any** read of it from `@nyariv/sandboxjs` (even `status.schemaVersion`)
   throws `state_unsafe_mutation` (Seelen log:
   `Error executing sandboxed code: [Error: https://svelte.dev/e/state_unsafe_mutation]`).
   A string is a primitive and is never proxied, so this plugin fetches
   `/v1/status.txt` (same body, `text/plain`) and calls `JSON.parse` itself.

Keep null checks as plain nested `if` statements, start every script with a
`typeof status === "undefined"` guard, parse the string into a local `api`
object, and never touch a property of a value that may be null. The Node test
file uses `node:vm`, which is more permissive; when in doubt, run the script
through `@nyariv/sandboxjs` directly with no `status` at all, with
`status: null`, and with a real `/v1/status.txt` payload. Seelen's own log at
`%LOCALAPPDATA%\com.seelen.seelen-ui\logs\Seelen UI.log` shows every
sandbox error.

## Isolation rule

This integration must remain removable. Nothing under `native/razer-core` or
the HID transport may import, call, or depend on Seelen UI.
