# Fix Plan: Audit Findings

This plan addresses every issue found in a full review of the codebase (written against commit `7e277cf`). The full project rules, architecture and behavior spec are in [`docs/AGENTS.md`](AGENTS.md); read that first. The constraints and decisions that matter most here are repeated below.

## How to use this file

- Work through the items **in order**. Some later items depend on earlier ones (noted inline).
- Check off tasks (`[x]`) as you finish them and commit the updated plan with the item's code.
- Line numbers below refer to commit `7e277cf` and will drift as you go; search for the named function or symbol if they no longer match.

---

## Project constraints

From `docs/AGENTS.md`:

- Rust stable, MSVC toolchain, Windows 10/11 only.
- UI is raw Win32 + Direct2D + DirectWrite. **No** egui, wgpu, winit, WinUI/XAML, Qt, GTK, Electron/WebView, or async runtimes.
- Single EXE, release build **< 1 MB** (currently 217 KB). Don't change `[profile.release]`.
- Restrained native look: Segoe UI, spacing in 8/16/24 px steps, no gradients or shadows.
- **Zero idle CPU**: no polling. Redraw only in response to `WM_PAINT`, input, or a real state change.
- Failing silently is fine. Never crash because of a misbehaving monitor. Log only in debug builds.

Decisions made during planning (already reflected in `docs/AGENTS.md`; they replace the old "no threads / no timers / Dxva2 only" rules):

| Topic | Decision |
|---|---|
| Threads | Allowed. Use **one worker thread per monitor** for all hardware I/O (item 6). |
| Timers | Allowed: **one-shot** timers that kill themselves (debounced save, display-change refresh). Never periodic polling. |
| Built-in laptop panels | Use **WMI** (`root\WMI`) when Dxva2 doesn't work for a display (item 4). |
| Broken config file | Rename it to `brightnessctl.json.bak`, continue with defaults, show the error in the status bar (item 5). |
| Menu bar in dark mode | Leave it light. Only the title bar goes dark (item 13). |

## Workflow (lightweight)

- Work on a branch: `fixes/audit`.
- **One commit per item**, using conventional commits: `fix(monitors): …`, `feat(wmi): …`, `refactor(app): …`, `test(...)`, `docs(...)`.
- Write unit tests alongside the change wherever the logic can be tested on its own (test-first is not required).
- After **every** item, run:
  ```
  cargo build
  cargo test
  cargo clippy
  ```
- After Phases 2, 3 and 6, also check the release size:
  ```
  cargo build --release
  ```
  `target\release\brightnessctl.exe` must stay under 1 MB.

## Baseline at `7e277cf`

- `cargo build`: OK, 4 dead-code warnings.
- `cargo test`: 21/21 pass.
- `cargo clippy`: 22 warnings (mostly collapsible `if`s).
- Release binary: 221,696 bytes.

## Test hardware

| Machine | Displays | Good for |
|---|---|---|
| Desktop | Two external DDC/CI monitors: Gigabyte `GBT2402`, Acer `ACR0B96` | DDC/CI paths, sync mode, two monitors at once |
| Laptop (Acer Aspire) | Built-in panel (plus any external monitor you connect) | WMI (item 4), DPI scaling (laptops usually default to 125–150%), device loss on hybrid graphics |

- If you're on the laptop without an external monitor, you can't test the DDC/CI paths by hand. Rely on the unit tests and re-check on the desktop later.
- Useful PowerShell checks:
  ```powershell
  Get-PnpDevice -Class Monitor -PresentOnly | Select InstanceId, FriendlyName
  Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorBrightness   # built-in panel only
  ```

---

## Phase 1: Confirmed bugs

### 1. Fix monitor identity lookup (`monitors.rs`)

**Problem (confirmed on the desktop):** `get_monitor_device_id` (`monitors.rs:64`) calls `EnumDisplayDevicesW(..., 0)`. That returns a driver-key style ID:

```
MONITOR\GBT2402\{4d36e96e-e325-11ce-bfc1-08002be10318}\0004
```

`get_edid_from_registry` (`monitors.rs:108`) compares it with SetupDi **device instance IDs**:

```
DISPLAY\GBT2402\5&145FDE2&5&UID24837
```

They never match, so `Monitor.identity` is always `None`. As a result:

- brightness is never saved,
- Smart Restore never runs,
- labels fall back to "Generic PnP Monitor".

**Verified fix:** call `EnumDisplayDevicesW` with `EDD_GET_DEVICE_INTERFACE_NAME` (`1`). It returns:

```
\\?\DISPLAY#GBT2402#5&145fde2&5&UID24837#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}
```

Strip the leading `\\?\`, drop the trailing `#{guid}`, and replace `#` with `\`. That gives `DISPLAY\GBT2402\5&145fde2&5&UID24837`, which matches the SetupDi ID **when compared ignoring case**.

- [x] Add a pure helper `interface_path_to_instance_id(&str) -> Option<String>`, with unit tests that use the real strings above.
- [x] Use `EDD_GET_DEVICE_INTERFACE_NAME` and the helper. Keep the comparison case-insensitive.
- [x] Look up the device ID **once** per `HMONITOR`. Today `monitor_enum_proc` calls it twice (`monitors.rs:166-167`).
- [x] Handle duplicated (cloned) displays, where one `HMONITOR` has several physical monitors: pair physical monitor *i* with `EnumDisplayDevicesW(adapter, i, …)`.
- [x] Store the instance ID in `Monitor.device_path`. Item 4 uses it to find WMI instances.
- [x] **Stable hash.** This was minor item 16, moved here so no unstable keys are ever saved. In `edid.rs:66-69`, replace `DefaultHasher` (its algorithm isn't guaranteed to stay the same between Rust versions) with a small inline FNV-1a 64-bit hash. Keep `test_fallback_hash_determinism` and add a test that pins a known hash value. No migration is needed: because of this bug, no identities were ever saved.

*Result:* the hardware test `cargo test test_enumerated_monitors_have_identity -- --ignored --nocapture` finds `GBT:9218:22350B003157` and `ACR:2966:1329143D03W01` on the desktop. Only active display devices are used. The fallback hash covers the 128-byte base EDID block only.

**Done when:**
- after one drag, `brightnessctl.json` has entries under `monitor_states`,
- labels show the EDID-based name (e.g. `GBT …`),
- the saved brightness is restored after restarting the app.

### 2. Fix the garbled bullet (`render.rs:260`)

**Problem (confirmed):** the source contains the bytes `C3 A2 E2 82 AC C2 A2`, which is "•" encoded as UTF-8 twice. It renders as `â€¢`.

- [x] Replace it with `\u{2022}`.
- [x] Search `src/` for any other non-ASCII mis-encoding. (None found; `src/` is now pure ASCII.)

---

## Phase 2: Display scaling and laptop panels

### 3. Handle display scaling (DPI) correctly

**Problem:** the process declares itself Per-Monitor-V2 DPI aware (`win.rs`), but:

- The drawing surface is created with `D2D1_RENDER_TARGET_PROPERTIES::default()` (`render.rs:124`). DPI 0 means system DPI, so Direct2D scales everything.
- Layout uses `GetClientRect` pixel widths as if they were device-independent units (DIPs; 96 per inch at 100% scaling).
- Hit-testing (`main.rs:446-482`) compares raw mouse pixels with DIP-based constants.

At 125% or 150% scaling the slider runs past the right edge and clicks land on the wrong values. `WM_DPICHANGED` isn't handled. The layout numbers are also written twice (`render.rs` and `handle_input`, with a "MUST match" comment).

- [x] Add `src/layout.rs`: a pure function along the lines of `compute(width_dip: f32, monitor_count: usize) -> Layout`. It returns the title rect, and for each row the label, percentage, track and hit rects, plus the status bar rect. Unit tests:
  - rows don't overlap,
  - each hit rect contains its track,
  - the percentage rect is flush right,
  - width is clamped to at least 1.
- [x] Use `layout::compute` in both `render.rs` and `handle_input`, and delete the duplicated constants in `main.rs`.
- [x] Create the drawing surface with `dpiX`/`dpiY` = `GetDpiForWindow(hwnd)`. Do all layout in DIPs: `width_dip = px * 96 / dpi`.
- [x] Convert mouse coordinates from pixels to DIPs before hit-testing.
- [x] Handle `WM_DPICHANGED`: `SetWindowPos` to the suggested rect (in `lParam`), `SetDpi` on the drawing surface, redraw.
- [x] Make the starting window size 400×600 **DIPs**, scaled for the window's monitor (use `AdjustWindowRectExForDpi`, which takes the menu bar into account).

*Result:*
- `layout::compute(width, height, rows)` also returns the tooltip rect, and provides `hit_test`, `value_at`, `thumb_x` and `px_to_dip`.
- The renderer reads the DIP size from the render target (`GetSize`) and calls `SetDpi` whenever `GetDpiForWindow` changes, which covers both `WM_DPICHANGED` and the first paint.
- The default size is now a 384×544 DIP **client area** (what 400×600 outer pixels gave at 100%). The window is created hidden, sized for its DPI, then shown.
- Checked by screenshot at 100% on the desktop (client 384×544, layout unchanged). Scaled displays still need a manual check.

**Done when:**
- at 100%, 125%, 150% and 175% the slider fits the window and clicking the thumb doesn't make it jump,
- moving the window between monitors with different scaling rescales it.

Test on the laptop's built-in screen.

### 4. Support built-in laptop panels through WMI

**Problem:** `GetMonitorBrightness`/`SetMonitorBrightness` (Dxva2's high-level monitor API) only work over DDC/CI. Built-in laptop panels don't support DDC/CI; Windows controls them through ACPI/WMI. There's no fallback, so a laptop panel shows 50% and is disabled after 10 failed writes. That contradicts the README.

- [x] Add a `Backend` enum to `Monitor`:
  ```rust
  enum Backend { HighLevel, Vcp { max: u32 }, Wmi { instance: String }, Unsupported }
  ```
  Choose it **once**, when monitors are detected, by trying the backends in order: HighLevel → VCP `0x10` → WMI. In item 9 this probing moves into the worker thread.
- [x] Add `src/wmi.rs`:
  - connect with `IWbemLocator` (`WbemLocator` CLSID) → `ConnectServer("root\\WMI")`,
  - read: `SELECT InstanceName, CurrentBrightness, Level FROM WmiMonitorBrightness`,
  - write: `WmiMonitorBrightnessMethods.WmiSetBrightness(Timeout: u32 = 0, Brightness: u8 0–100)` via `ExecMethod`.
- [x] Match a WMI `InstanceName` to a monitor. It is expected to be the device instance ID plus a `_0` suffix (check with `Get-CimInstance` on the laptop). Strip the suffix and compare, ignoring case, with `Monitor.device_path` from item 1.
- [x] Add these to the `windows` features in `Cargo.toml` (both exist in windows 0.62.2):
  - `Win32_System_Wmi`
  - `Win32_System_Variant`
- [x] Unit tests for the pure parts: instance-name matching, and mapping the panel's supported levels to 0–100.
- [x] Re-check that the release binary is under 1 MB.

*Result:*
- **`Backend::Wmi(wmi::Panel)`.** The panel holds its WMI connection, instance name, method path and a reusable in-parameters object, so writes don't look anything up again.
- **Probing.** `brightness::probe_all` chooses each backend and returns current values. WMI is connected only if a monitor fails both Dxva2 paths.
- **Levels.** WMI reads and writes are already percentages (the driver snaps to supported levels), so `Level` isn't read. The unit tests cover instance matching and `VARIANT` conversions instead.
- **Name format.** On the desktop, `WmiMonitorID` (same `InstanceName` format) confirms `<instance ID>_0`, and the ignored test `test_wmi_instances_match_enumerated_monitors` checks the matching.
- **COM gotcha.** `CoSetProxyBlanket` must be given the `IWbemServices` / enumerator proxy itself. Passing a cast `IUnknown` sets the wrong proxy and every query fails with `WBEM_E_ACCESS_DENIED`.
- **Found while testing: physical monitor handles start at `0`.** `Monitor.physical` is now `Option<HANDLE>`. Before, handle 0 was treated as invalid and never freed.
- **Displays without a Dxva2 handle** are kept, with `physical: None`, so WMI can still control them.
- **VCP writes** use the maximum cached at probe time. Each write is now SET + read-back instead of GET + SET + GET.
- **Release binary:** 232,448 bytes.
- **Hardware test.** The ignored test `test_probe_all_reads_every_monitor` reads (never writes) every monitor. On the desktop both monitors probe as `HighLevel`.
- **Still to check on the laptop:** the WMI write path.

**Done when:** the Acer Aspire's built-in panel can be controlled with the slider, the wheel and the hotkeys, and the desktop's DDC monitors still work.

---

## Phase 3: Config, background threads and saving

### 5. Make the config file reliable (`settings.rs`)

**Problems:**

- `load()` and `save()` use the relative path `"brightnessctl.json"`, which is relative to the **working directory**, not the exe's folder as the README says.
- If parsing fails, `load()` returns defaults **and overwrites the user's file** (`settings.rs:131-133`).
- `theme` has no serde default, so a file without it fails to parse.

Tasks:

- [x] Find the config path with `std::env::current_exe()` → parent folder → `brightnessctl.json`.
- [x] Put `#[serde(default)]` on the `Settings` struct so missing fields fall back to `Default`.
- [x] If parsing fails:
  - rename the file to `brightnessctl.json.bak`, replacing any existing `.bak`,
  - continue with defaults,
  - set the status message to something like `Config error at line L, column C. Saved as .bak; using defaults` (from `serde_json::Error::line()` / `column()`).
- [x] Validate after loading:
  - presets: drop NaN, clamp to 0–1,
  - `step_size`: must be greater than 0 and at most 1, otherwise use the default.
- [x] Save atomically: write `brightnessctl.json.tmp`, then `std::fs::rename` over the real file.
- [x] Make loading and saving take a path so they can be tested. Tests, in a temporary folder:
  - missing `theme` → defaults,
  - invalid JSON → `.bak` created and defaults returned,
  - out-of-range values are clamped,
  - save then load gives the same settings.

*Result:*
- `Settings::load()` returns `(Settings, Option<String>)`; the warning becomes the first status message. `load_from` and `save_to` take a path.
- After backing up a broken file, fresh defaults are written to the config path.
- If the rename fails, nothing is written, so the user's file is never touched.
- If the file exists but can't be read, defaults are used and the file isn't overwritten.
- **Checked by hand:** a copy of the exe in a temporary folder, with a broken config, started from the repo root. It showed `Config error at line 3, column 3. Saved as .bak; using defaults`, kept the original text in `.bak`, and left the repo-root `brightnessctl.json` untouched.

**Done when:**
- starting the exe from a different working directory uses the config next to the exe,
- a config with a typo ends up as `.bak` instead of being wiped.

### 6. One background thread per monitor

**Problems:**

- All monitor I/O runs on the window's thread. A DDC/CI transaction takes roughly 40–50+ ms, and each write does a GET, a SET and a GET (`brightness.rs:69-105`), so the window freezes while it waits.
- `set_brightness` silently drops any write within 100 ms of the previous one (`brightness.rs:56-63`). Only dragging sends a final forced write (on `WM_LBUTTONUP`). The wheel (`main.rs:361`) and hotkeys (`main.rs:399`) can therefore leave the slider showing a value the monitor never received.

**Design:**

- **Thread per monitor.** Add `src/worker.rs`. Each monitor gets a thread that **owns** its physical monitor handle. A WMI-backed thread calls `CoInitializeEx(COINIT_MULTITHREADED)` and owns its WMI connection.
- **Latest-value mailbox.** Each thread has a `Mutex<Option<u32>>` + `Condvar` that holds only the newest requested value. Values in between are skipped; the final value is always written.
- **Write spacing.** Leave at least a minimum gap between writes. Start at about 50 ms and tune on real hardware.
- **Confirmation.** After a write, if no newer value is waiting, read the brightness back to confirm the final value. In-between writes are never read back.
- **Reporting back.** The thread sends results to the window with `PostMessageW(hwnd, WM_APP_WRITE_DONE, monitor_index, packed_result)`: written value, success, confirmed. `HWND` isn't `Send`, so pass it as an `isize`. The window thread keeps the failure count and sets `disabled` after 10 failures.
- **Window side.** Add a single entry point, `request_brightness(targets, value)`, used by drag, wheel, hotkey, typed value and presets. The slider value updates immediately, and the request is posted to each target's thread.

Tasks:

- [x] Put hardware access behind a trait, e.g. `trait BrightnessBackend { fn read(&mut self) -> Option<u32>; fn write(&mut self, v: u32) -> bool; }`, so worker logic can be tested with a fake backend.
- [x] Implement the worker and the mailbox as described above.
- [x] Route **all** input paths through `request_brightness`.
- [x] Remove the rate limiter (`last_write_time`) and `set_brightness_forced`.
- [x] Keep brightness **only on `Monitor`** (`ui_value` + `confirmed_value`). Remove `AppState.brightness` and `Monitor.normalized_value`, which duplicate it. (This covers item 21.)
- [x] Remove `unsafe impl Send/Sync for Monitor` (`state.rs:66-67`). Use a small wrapper around just the physical monitor handle that is `Send`.
- [x] Shutdown and refresh: tell the threads to stop, join them, **then** call `DestroyPhysicalMonitors`.
- [x] Unit tests with a fake backend and a controllable clock:
  - ten fast requests → only the last is written,
  - the minimum gap is respected,
  - a read-back happens only once no new value is waiting,
  - failures are reported back.

*Result:*
- **Trait and implementations.** The trait has `read` / `write` / `verify`, with `HighLevel`, `Vcp` and `Wmi` implementations in `brightness.rs`. `Vcp` verifies the raw VCP value. `Wmi` counts an accepted write as confirmed, because the driver snaps writes to the panel's supported levels.
- **Owned handles.** `PhysicalHandle` owns a Dxva2 handle and destroys it on drop. It moves into the worker through `BackendSpec`, so dropping a `Monitor` stops and joins its worker, which then frees the handle.
- **`Backend` is plain data now** (`Wmi` has no payload). COM objects can't cross threads, so each WMI worker connects itself.
- **`WriteResult.confirmed` is `Option<bool>`.** It's `None` for in-between writes, which weren't read back; those leave `write_confirmed` unchanged, so the label doesn't flicker during a drag.
- **Stale results.** Results carry a worker generation, so results still queued from before a Refresh are ignored.
- **Brightness fields.** Brightness now lives in `Monitor.ui_value`, `written_value` and `last_requested`; the last one makes requests idempotent. Also removed: `observed_min` / `observed_max`, which were recorded but never used.
- **Item 23, done early:** failure counting is now `Monitor::apply_write_result`, and its tests call the real code. The fake `test_backoff_escalation` is gone.
- **Tests and timing.**
  - The worker tests are deterministic: the fake backend blocks until released, and they passed 30 out of 30 runs.
  - The ignored hardware test `test_worker_rewrites_current_value_on_hardware` writes each monitor's current value back through a real worker. On the desktop it confirmed the write in 104–116 ms, the delay that used to block the window.
- **Unchanged:** the 50 ms wheel input throttle. It limits input, not writes.
- **Measured:** no CPU used while idle; release binary 266,240 bytes. The startup probe still runs before the window opens; that's item 9.

**Done when:**
- after fast scrolling, holding a hotkey, or dragging in sync mode, every monitor ends up at the value its slider shows,
- the window never stalls during a drag.

### 7. Save brightness when it actually changes

**Problem:** settings are saved only in `WM_LBUTTONUP` (`main.rs:272-306`), and on **every** mouse release, even clicks on empty space. Changes from hotkeys, the wheel and typed values are never saved. It also saves the slider's value instead of the value confirmed by the monitor.

- [x] When a `WM_APP_WRITE_DONE` message for a successful write arrives, mark settings as dirty and restart a **one-shot timer of about 1 s**. When it fires, save and kill the timer.
- [x] If anything is unsaved, save it on `WM_CLOSE` and `WM_ENDSESSION`.
- [x] Save `confirmed_value` (the value that was actually written), not the slider's value.
- [x] Remove the save from `WM_LBUTTONUP`.
- [x] Tests: the dirty flag is set and cleared correctly, and a click with no brightness change doesn't mark anything dirty.

*Result:*
- **Write results.** `AppState::handle_write_result` applies a worker result: it ignores stale generations, and it marks settings dirty only for a successful write to an identified monitor.
- **Saving.** `persist_monitor_states` copies `Monitor::to_state()` (built from `written_value`) into the settings. A monitor the app never wrote is never saved, so its saved state from an earlier session isn't overwritten.
- **Timer.** `SetTimer(SAVE_TIMER_ID, 1000)` is re-armed on every result, so a burst of writes saves once.
- **Checked by hand,** by posting fake results to a temporary copy of the app:
  - not saved after 0.3 s, saved by 1.5 s,
  - a result posted 100 ms before closing was saved by `WM_CLOSE`.

---

## Phase 4: Robustness

### 8. Recover from graphics device loss (`render.rs`)

**Problem:** the result of `EndDraw` is ignored (`render.rs:456`). After `D2DERR_RECREATE_TARGET` (driver reset, GPU switch, some remote desktop sessions), the drawing surface is never recreated. Also, `Resize` is called on every paint.

- [x] Check the `EndDraw` result. On `D2DERR_RECREATE_TARGET`, throw away only the device-dependent resources (drawing surface and brushes, but keep the text formats) and redraw.
- [x] Add `Renderer::resize(w, h)` and call it from `WM_SIZE`. Remove the per-paint `Resize`.

*Result:*
- `discard_device_resources` drops the drawing surface and brushes and keeps the text formats. `render` calls it on `D2DERR_RECREATE_TARGET` and invalidates the window, so the next `WM_PAINT` rebuilds the resources.
- `WM_SIZE` passes the new pixel size to `Renderer::resize`.
- Checked by screenshot after an external resize to 560×420: the sliders stretch and the status bar sits at the new bottom.
- **Still to check by hand:** Win+Ctrl+Shift+B.

**Done when:** after pressing **Win+Ctrl+Shift+B** (restarts the graphics driver), the window draws correctly again. Test on both machines.

### 9. Open the window instantly

**Problem:** `main` finds the monitors, reads them over DDC and applies Smart Restore writes before the window is even created (`main.rs:44-87`). With DDC delays that can take hundreds of milliseconds or more.

- [x] Create and show the window first, then `PostMessageW(WM_APP_INIT)`.
- [x] In `WM_APP_INIT`, find the monitors. This only reads the registry and SetupDi and doesn't talk to the monitors.
- [x] Start one worker per monitor. Each one:
  - chooses its `Backend` (moved here from item 4),
  - reads the current brightness,
  - applies Smart Restore if there's a saved state with `last_set_by_app && write_confirmed`,
  - posts `WM_APP_MONITOR_READY` with its backend and current value.
- [x] Until a monitor reports ready, its row shows `Detecting…` and its slider ignores input.
- [x] Debug builds only: log the time until the window appears and how long each write takes.

*Result:*
- **Startup order.** `UpdateWindow` paints the window before `WM_APP_INIT` runs. Finding the monitors takes about 2 ms.
- **Probing on the worker.** `brightness::with_probed_backend` runs on the worker thread; `probe_all`, `BackendSpec` and `worker_spec` are gone.
- **`WM_APP_MONITOR_READY`.** Its `lParam` carries a boxed `MonitorReady` (probe + restore result), taken with `worker::take_ready`. The box is freed even for a stale generation, or if `PostMessageW` fails.
- **Input gating.** `Monitor::accepts_input` (ready, controllable, not disabled) now gates every input path, so disabled and unsupported monitors no longer move either.
- **Found by the hardware test: DDC/CI rejects a write sent straight after the probe read** (it failed in under 1 ms). A `worker::Pacer` now spaces *every* transaction (probe, Smart Restore write, writes, read-backs) by `MIN_IO_GAP` = 50 ms, and `MIN_WRITE_GAP` was renamed to match.
  - Post-probe work is in `worker::serve`, and its tests cover the pacing and Smart Restore.
  - On hardware, a write straight after the probe now succeeds and is confirmed (about 205–215 ms including gaps).
- **Debug log.** `log::debug_log!` writes to `brightnessctl.log` next to a debug exe and is compiled out of release builds.
- **Measured in release** (process start until the window is visible, warm runs, fresh config): about 160 ms before → about 45 ms after. Both monitors report ready about 56 ms and 114 ms after `WM_APP_INIT`.
- **Item 18, done early:** Refresh Monitors now goes through the same path, so it also applies Smart Restore.

**Done when:** the window appears instantly, sliders become active as each monitor reports in, and dragging is smooth on both desktop monitors.

---

## Phase 5: Input and UI correctness

### 10. Fix typed-value editing

**Problems:**

- Clicking elsewhere throws the typed value away (`main.rs:496,501`), but the spec says it should be committed.
- `WM_KILLFOCUS` isn't handled.
- `ch as u8 as char` (`main.rs:347`) cuts UTF-16 characters down to one byte, so some non-ASCII keys are read as digits.

Tasks:

- [x] Add a pure `parse_percent(&str) -> Option<f32>`. Tests:

  | Input | Result |
  |---|---|
  | `""` | `None` |
  | `"0"` | `0.0` |
  | `"100"` | `1.0` |
  | `"150"` | `1.0` |
  | `"abc"` | `None` |

- [x] Use `char::from_u32(ch as u32)` and accept only ASCII digits.
- [x] Commit the typed value when the user clicks elsewhere (including another monitor's label) and on `WM_KILLFOCUS`. `Esc` still cancels.
- [x] Commit through `request_brightness` (item 6).

*Result:*
- **Editing moved into `AppState`:** `begin_edit`, `edit_input`, `commit_edit` and `cancel_edit`, all unit-tested. The Win32 handlers only forward to them.
- **Clicks.** Clicking the label already being edited keeps editing. Clicking any other spot commits, and so does clicking another monitor's label before it opens.
- **`parse_percent`** also rejects signs and non-ASCII digits. An overflowing digit string clamps to 100%.
- **Gating.** Editing requires `Monitor::accepts_input`, so a monitor that is still detecting can't be edited.
- **Checked end to end** by posting a click, characters and `WM_KILLFOCUS` to a temporary copy of the app. Focus loss committed 25% with exactly one write, and Enter restored 24% with one write.

### 11. Show the correct control method in labels

**Problem:** the label says "DDC Active" when `write_confirmed` is true (`render.rs:252-258`). That is also true for the high-level API, and before any write every monitor shows "High-level".

- [x] Build the label from `Backend` and the monitor's state, with a unit test:

  | Monitor state | Label |
  |---|---|
  | `HighLevel` | `DDC/CI` |
  | `Vcp` | `DDC/CI (VCP)` |
  | `Wmi` | `WMI` |
  | `Unsupported` | `Unsupported` |
  | disabled | `Disabled` |
  | not ready yet | `Detecting…` |

*Result:* `Monitor::status_label()`, used by the renderer. The precedence is not ready, then disabled, then backend. Checked by screenshot: both desktop monitors show `DDC/CI`, including after a confirmed write (which used to flip the label to "DDC Active").

---

## Phase 6: Minor issues

- [x] **12. Flicker.** Handle `WM_ERASEBKGND` by returning 1. Remove `hbrBackground` (`window.rs:25`); that brush is also never freed.
  - *Result:* the window class has no background brush, and `WM_ERASEBKGND` returns 1. Direct2D already paints every pixel, so nothing is left unpainted (checked by screenshot after a resize).
- [x] **13. Theme.**
  - Turn on a dark title bar with `DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)` (add the `Win32_Graphics_Dwm` feature) and update it when the theme changes.
  - Remember the system dark-mode flag instead of reading the registry on every paint (`render.rs:203` → `settings.rs:95`).
  - Refresh the flag on `WM_SETTINGCHANGE` when `lParam` is `"ImmersiveColorSet"`.
  - The menu bar **stays light** (decision).
  - *Result:*
    - `window::set_dark_title_bar` is applied before the window is first shown, so a dark window never flashes a light title bar, and again on every theme change.
    - The flag lives in `AppState.system_dark`. `ThemeMode::is_dark(system_dark)` is a pure function with a unit test, and `AppState::is_dark()` is what the renderer uses. `Settings::is_dark_mode` is gone.
    - The three theme menu handlers became one `set_theme` helper.
    - Checked by screenshot: the title bar is dark at startup and follows Light, Dark and Auto from the menu. A `WM_SETTINGCHANGE` "ImmersiveColorSet" sent to the window is handled without changing the registry.
- [x] **14. Brushes.** Stop creating `track_brush` and `accent_brush` on every paint (`render.rs:327,344`). Create them once and change their color with `SetColor` for hover and active states.
  - *Result:* `brush_track` and `brush_fill` are created with the other brushes and dropped with them (theme change, device loss) through one `drop_brushes` helper. Paint no longer creates any brush, and nothing inside `BeginDraw`/`EndDraw` can return early with `?` any more. Checked by screenshot: an active slider is drawn lighter while the other row keeps the normal colors.
- [x] **15. Brightness math.**
  - Scale high-level API values to 0–100 using the min/max it returns (`brightness.rs:21-24`).
  - Round instead of truncating in VCP conversions (`brightness.rs:35,84`).
  - Put the conversions in pure functions and test round trips for max = 100, 255 and 50.
  - *Result:*
    - `brightness::Range { min, max }` does every conversion (`to_raw`, `to_percent`) with rounding, and both Dxva2 backends use it. An empty or inverted range falls back to 0–100; out-of-range raw values are clamped.
    - `verify` compares raw values for both Dxva2 backends, so a monitor with fewer than 100 steps isn't reported as unconfirmed.
    - Tests: endpoints, every percent round trips for max 100 and 255, every raw level round trips for max 50 (percents within 1), rounding cases, clamping.
    - Both desktop monitors report 0–100, so they behave exactly as before (hardware tests pass, writes confirmed).
- [x] ~~**16. Stable hash**~~: moved into item 1.
- [x] **17. Hotkeys.** Call `HotkeyManager::unregister_all` in `WM_DESTROY` (`hotkeys.rs:34` is never called today).
  - *Result:* `WM_DESTROY` unregisters every hotkey before quitting. The empty `Drop` impl (it only held a comment about this) is gone. A unit test registers a free key combination on the test thread, shows a second registration fails while it's held, and succeeds again after `unregister_all`. The app still closes cleanly with exit code 0.
- [ ] **18. Refresh.**
  - ~~Make `IDM_REFRESH` (`main.rs:159-173`) stop and join the worker threads, find the monitors again, and use the startup path, including Smart Restore.~~ Done in item 9 (`start_monitors`).
  - Reset the hover, active, editing and drag indexes.
  - Refresh automatically on `WM_DISPLAYCHANGE`, delayed with a one-shot timer of about 500 ms.

## Phase 7: Code structure and tests

- [ ] **19. Remove the global state.** Replace the `static mut` globals (`main.rs:32-37`) with an `App` struct. Pass it through `CreateWindowExW`'s `lpParam`, store it in `GWLP_USERDATA` during `WM_NCCREATE`, and fetch it in `wnd_proc`. Remove `#![allow(static_mut_refs)]`.
  - Don't hold a `&mut App` across calls that run their own message loop, such as `MessageBoxW` in About.
  - Worker threads never touch `App`; they only post messages.
- [x] **20.** Layout defined twice: done in item 3.
- [x] **21.** Brightness stored in several places: done in item 6.
- [ ] **22. Remove unused code and warnings.**
  - Remove the unused `Target` and `Command` (`actions.rs:23,29`), `should_restore` (`brightness.rs:124`, used only by its own test) and `is_registered` (`hotkeys.rs:46`).
  - Fix every `cargo clippy --all-targets` warning.
  - Add `cargo clippy --all-targets --locked -- -D warnings` to `.github/workflows/ci.yml`.
- [ ] **23. Fix the tests.**
  - `test_backoff_escalation` (`brightness.rs:155`) copies the logic instead of calling it. Move failure handling into a real function, e.g. `record_failure()`, and test that.
  - Remove the always-true `assert!(monitors.len() >= 0)` (`monitors.rs:204`).
  - Add tests for the wheel step size: fine step with `Shift`, coarse without.
  - Mark tests that need real monitors with `#[ignore]`.

## Phase 8: Documentation

- [ ] **24. README.**
  - Default step size is 0.1, not 0.05 (`README.md:84`).
  - The config file lives next to the exe, and a broken config is backed up to `.bak`.
  - Built-in laptop panels are supported through WMI (`README.md:32`).
  - Startup is now instant (monitors are detected after the window opens).
- [ ] **25. `docs/AGENTS.md`.** Bring it in line with the finished code:
  - add `layout.rs`, `wmi.rs` and `worker.rs` to the module map and data flow,
  - remove every *(→ fix plan #N)* note whose item is done,
  - update the startup sequence (window first, then monitor detection),
  - record the final minimum write gap from item 6,
  - update the release binary size,
  - move anything learned while fixing (e.g. the confirmed WMI instance-name format) into the relevant section.
