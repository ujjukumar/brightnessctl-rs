# AGENTS.md: brightnessctl

Reference for anyone (human or AI agent) working on this repository. This file replaces the former `GEMINI.md` and `conductor/` files.

**Active work:** [`docs/fix-plan.md`](fix-plan.md). Read it before changing code. Where the current code doesn't match the rules below, the fix plan item that resolves it is noted as *(→ fix plan #N)*.

---

## 1. Product

A native, professional Windows brightness controller for power users and multi-monitor setups. It controls built-in laptop panels and external DDC/CI monitors with one slider per display.

**Success criteria:**
- Instant startup.
- Release EXE **< 1 MB**.
- **Zero idle CPU.**
- Native Windows 11 look.
- No framework bloat.

**Tone:** a system utility, not a consumer app. Labels are short and messages purely informative. No chatty language or filler.

---

## 2. Hard constraints

| Area | Rule |
|---|---|
| Language | Rust stable, **MSVC** toolchain, edition 2024 |
| Platform | Windows 10/11 x64 only |
| UI | Raw **Win32 + Direct2D + DirectWrite**. GDI only as a fallback. |
| Banned | egui, wgpu, winit, WinUI/XAML/MAUI/WPF, Qt, GTK, Electron/WebView, cross-platform UI crates, async runtimes |
| Brightness APIs | **Dxva2** high-level monitor API and **DDC/CI** (VCP code `0x10`). **WMI** (`root\WMI`) only for displays where Dxva2 fails, i.e. built-in laptop panels. |
| Output | A single EXE from `cargo build --release`. No installer, no runtime dependencies. |
| Dependencies | `windows`, `windows-numerics`, `serde`, `serde_json`. Add a new crate only with a strong reason; check its effect on binary size. |

### Runtime rules

- **No polling.** Redraw only on `WM_PAINT`, input, or a real state change.
- **Timers:** only **one-shot** timers that kill themselves (debounced save, delayed display-change refresh). Never periodic.
- **Threads:** allowed **only for monitor I/O**, with one worker thread per monitor (`worker.rs`):
  - workers talk to the window thread only through `PostMessageW`,
  - they never touch UI or app state,
  - the window thread never blocks on monitor I/O.
- **No global mutable state.** Keep app state in a struct reached through the window (`GWLP_USERDATA`). *(Currently `static mut` → fix plan #19.)*
- **Errors:**
  - failing silently is acceptable,
  - **never crash because of a misbehaving monitor**,
  - log only in debug builds: `log::debug_log!` appends to `brightnessctl.log` next to the exe and compiles away in release builds,
  - no dialogs except Help → About.
- Release builds use `panic = "abort"`, so a panic kills the app. Don't `unwrap` hardware results, and check indexes before using them.

### Release profile (do not change)

```toml
[profile.release]
opt-level = "z"
lto = "fat"
panic = "abort"
strip = "symbols"
codegen-units = 1
incremental = false
```

The release binary was 221,696 bytes (about 217 KB) at commit `7e277cf`. Check the size after any feature or dependency change.

---

## 3. Architecture

```
src/
├─ main.rs        Entry point, message loop, wnd_proc, input/hotkey/wheel/numeric handlers
├─ win.rs         COM init (STA) + DPI awareness (Per-Monitor V2)
├─ window.rs      Window class registration + CreateWindowExW
├─ menu.rs        Native menu bar and command IDs
├─ layout.rs      Window layout in DIPs, shared by rendering and hit-testing
├─ render.rs      Direct2D/DirectWrite renderer and color palettes
├─ state.rs       Monitor, MonitorIdentity, AppState; step/preset/sync math
├─ monitors.rs    Monitor enumeration, device ID and EDID lookup (SetupDi + registry)
├─ edid.rs        EDID parsing into a MonitorIdentity
├─ brightness.rs  Backend probing (high-level → VCP 0x10 → WMI), BrightnessBackend impls, PhysicalHandle
├─ worker.rs      One I/O thread per monitor: probe + Smart Restore, latest-value mailbox, Pacer, WM_APP_* messages
├─ log.rs         debug_log! macro: brightnessctl.log next to debug builds only
├─ wmi.rs         WMI backend for built-in panels (root\WMI)
├─ settings.rs    brightnessctl.json load/save, system theme detection
├─ hotkeys.rs     RegisterHotKey / UnregisterHotKey wrapper
└─ actions.rs     Action enum (hotkey IDs)
```

### Data flow

1. **Startup:** set up COM and DPI → load settings → create and paint the window (`UpdateWindow`) → post `WM_APP_INIT` → register hotkeys → message loop. `WM_APP_INIT` finds the monitors (registry and SetupDi only, about 2 ms) and starts one worker per monitor (`AppState::start_workers`). Each worker probes its monitor (`brightness::with_probed_backend`), applies Smart Restore, and reports `WM_APP_MONITOR_READY` (`AppState::handle_monitor_ready`). Until then the row shows `Detecting…` and ignores input (`Monitor::accepts_input`). Refresh Monitors uses the same path.
2. **Input** (mouse, wheel, keyboard, hotkeys, menu) changes `Monitor.ui_value`, then calls `AppState::request_brightness(targets)`. That posts the value to each target's worker, unless it is the value last sent.
3. **Workers** write the newest value and read back the value that ends a burst. Every transaction (probe, write or read-back) starts at least `worker::MIN_IO_GAP` (50 ms) after the previous one ended (`worker::Pacer`); DDC/CI monitors reject commands sent sooner. Each result goes back to the window as `WM_APP_WRITE_DONE` (`Monitor::apply_write_result`).
4. **Rendering only reads state.** It never changes anything.
5. **Brightness is normalized to 0.0–1.0 internally.** A percentage (0–100) is used only for display and at the hardware boundary.

### Rules

- One slider per physical monitor.
- Free physical monitor handles when a monitor is dropped (`DestroyPhysicalMonitors`). Dxva2 handles start at **0**, so a monitor's handle is `Option<HANDLE>`. Never use `HANDLE::is_invalid()` on them. Displays without a handle are kept, because WMI may still control them.
- Hit-testing and rendering must use the **same** layout code: `layout::compute`.
- **DPI:** all layout is in DIPs (96 per inch). The render target is created at `GetDpiForWindow` and follows DPI changes with `SetDpi`. Mouse coordinates are converted with `layout::px_to_dip` before hit-testing. `WM_DPICHANGED` applies the window rect Windows suggests. The default client area is 384×544 DIPs.
- **Device loss:** if `EndDraw` returns `D2DERR_RECREATE_TARGET`, the renderer drops its device resources (render target and brushes, keeping the text formats) and invalidates the window, so they are rebuilt on the next paint. The render target is resized only from `WM_SIZE` (`Renderer::resize`), never per paint.
- **No erase:** the window class has no background brush and `WM_ERASEBKGND` returns 1. Direct2D paints the whole client area, so an erase would only flash.
- **Brushes** are created once per device and theme. Per-row hover and active colors use `SetColor` on `brush_track` and `brush_fill`; never create brushes while painting.

---

## 4. Visual design

This should look like **a Windows system utility**, not a styled app. If anything feels framework-like, it's wrong.

### Window
- Standard bordered, resizable Win32 window.
- System title bar (never custom-drawn).
- Native menu bar, which stays light even in dark mode.

### Layout
- Vertical stack with a fixed content column.
- Spacing **8 / 16 / 24 px only**. Margins are 24.

### Typography
Segoe UI only:

| Use | Size |
|---|---|
| Title | 20–22 px (uses 22) |
| Body | 13–14 px (uses 14) |
| Labels | 12 px, subdued color |

### Color

| Element | Dark | Light |
|---|---|---|
| Background | near-black `0.10` | `0.95` |
| Foreground | off-white `0.95` | `0.10` |
| Subdued | `0.60` | `0.40` |
| Accent | `#0078D4` | `#0078D4` |

- One muted accent color.
- **No gradients, no shadows**, no non-native theming.

### Controls
- Custom-drawn sliders: a 4 px track, a filled part, and a round thumb.
- Large hit targets.
- Subtle hover and active feedback (lighter track, bigger thumb).
- Percentage aligned right, above the track.

### Monitor labels
Each row's label reads `<name> • <status>`. The name is the EDID manufacturer and serial (or the Windows description if there's no EDID). The status comes from `Monitor::status_label`:

| Monitor state | Status |
|---|---|
| not probed yet | `Detecting…` |
| 10 failed writes | `Disabled` |
| `HighLevel` backend | `DDC/CI` |
| `Vcp` backend | `DDC/CI (VCP)` |
| `Wmi` backend | `WMI` |
| nothing can control it | `Unsupported` |

### Themes
`Auto` (follows the Windows `AppsUseLightTheme` setting), `Light`, `Dark`. Changes apply immediately.
- The system setting is read once at startup into `AppState.system_dark` and again on `WM_SETTINGCHANGE` with `"ImmersiveColorSet"`, never per paint. `AppState::is_dark()` resolves the theme.
- The title bar follows the theme via `DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)` (`window::set_dark_title_bar`), set before the window is first shown. The menu bar stays light.

### Status bar
A 24 px bar at the bottom with a separator line. It shows the last action, `SYNC ACTIVE` while sync mode is on, and `HOTKEY ERROR (n)` if any hotkeys failed to register.

### Tooltip
Shown when hovering over a row: name, hardware range, device path, and EDID manufacturer/code/serial.

### Menu

| Menu | Items |
|---|---|
| File | Exit |
| View | Refresh Monitors; Sync All Monitors (Lock) |
| Theme | Auto / Light / Dark |
| Help | About |

---

## 5. Behavior specification

### 5.1 Monitor identity

- **Composite key:** `MFG:product_code:serial`.
  - `MFG` is the 3-letter EDID manufacturer ID.
  - `product_code` is a u16, little-endian.
  - `serial` is the EDID serial-number **string** (descriptor tag `0xFF`) if present, otherwise the numeric serial.
- **Fallback:** if there's no serial (or it is 0), use a 64-bit FNV-1a hash of the 128-byte base EDID block and set `is_fallback = true`. Keys are persisted, so the hash must never change (that's why `DefaultHasher` isn't used).
- **No EDID:** the monitor is unidentified. Nothing is saved for it.
- **Getting the EDID:**
  1. `EnumDisplayDevicesW(adapter, i, EDD_GET_DEVICE_INTERFACE_NAME)`, keeping only active devices,
  2. convert the interface path to the device instance ID (`interface_path_to_instance_id`),
  3. find the matching SetupDi device in the monitor class (comparison ignores case),
  4. read the registry value `EDID`.

  With cloned displays, physical monitor *i* of an `HMONITOR` is paired with display device *i*.

### 5.2 Brightness control

**Backend order:**
1. high-level `Get/SetMonitorBrightness`,
2. raw VCP `0x10` (`GetVCPFeatureAndVCPFeatureReply` / `SetVCPFeature`),
3. WMI for built-in panels: `WmiMonitorBrightness.CurrentBrightness` to read, `WmiMonitorBrightnessMethods.WmiSetBrightness` to write. Both are already percentages.

The backend is chosen **once per monitor** by its worker (`brightness::with_probed_backend`, reported as `Monitor.backend`), not tried again on every write.

**WMI notes:**
- WMI is connected lazily, only when a monitor fails both Dxva2 paths.
- A WMI `InstanceName` is the device instance ID plus `_0` (`wmi::instance_matches`).
- `CoSetProxyBlanket` must receive the interface proxy itself (`IWbemServices`, `IEnumWbemClassObject`), never a cast `IUnknown`. Otherwise every call fails with `WBEM_E_ACCESS_DENIED`.

**Values:**
- Normalize to 0–100 at the hardware boundary using the device's min/max, with rounding *(→ fix plan #15)*.
- Treat the hardware min/max as advisory.

**Reliability:**
- After the write that ends a burst, the worker reads the value back (`BrightnessBackend::verify`) to set `write_confirmed`. In-between writes aren't read back, and they leave the flag unchanged.
- Disable a monitor after **10 consecutive failures** (`Monitor::apply_write_result`). Refresh Monitors re-enables it.
- Don't flood the DDC/CI bus. Each worker keeps only the newest requested value, and every transaction starts at least `MIN_IO_GAP` (50 ms) after the previous one ended. Values in between are skipped; the final one is never lost. Without the gap, a write sent right after the probe read fails.
- **Ownership:** a monitor's `PhysicalHandle` moves into its worker. Dropping a `Monitor` stops and joins the worker, which then destroys the handle. Results carry a worker generation, so results from before a Refresh are ignored.

**Smart Restore:** when its worker starts, a monitor gets its saved value back **only if** `last_set_by_app && write_confirmed`, and only if it differs from the value just read. Otherwise the monitor keeps its current value. The worker writes the saved value before reporting ready (`MonitorReady.restore`). Never overwrite a value the user changed outside the app.

### 5.3 Input

**All input paths go through one write function**, `AppState::request_brightness`, and are idempotent: if the value didn't change, nothing is written.

**Mouse drag:**
- Click the track to jump to that value; drag to adjust.
- Hardware updates live during the drag, and the final value is always written.

**Mouse wheel:**
- Works while hovering over a slider; no focus needed.
- Coarse step **0.05**, or fine step **0.01** while holding **Shift**. No other modifiers are read, to avoid clashing with hotkeys.
- Clamped to 0–1, never wraps.

**Typed value** (`AppState::begin_edit` / `edit_input` / `commit_edit` / `cancel_edit`):
- Clicking the percentage label turns it into an edit field (ASCII digits only, up to 3).
- **Enter**, **losing focus** (`WM_KILLFOCUS`), or clicking anywhere else commits, including clicking another monitor's label. **Esc** cancels. Empty or invalid text keeps the old value.
- Commits produce exactly one write, and nothing is written while typing.
- Values above 100 are clamped (`parse_percent`). After commit or cancel the label shows a whole-number percentage again.

**Global hotkeys:**
- Defined in the config; the defaults are:

  | Hotkey | Action |
  |---|---|
  | `Ctrl+Alt+Up` | Step Up |
  | `Ctrl+Alt+Down` | Step Down |
  | `Ctrl+Alt+P` | Cycle Presets |

- `Action` values (`StepUp = 1`, `StepDown = 2`, `CyclePresets = 3`) are used as the `RegisterHotKey` IDs.
- **Registration failures:** that hotkey is disabled, the failure shows in the status bar, and it isn't retried until restart or a config change.
- Hotkeys are unregistered on exit *(→ fix plan #17)*.

**Targeting** (hotkeys):
- Sync mode on → all monitors.
- Otherwise, the first of:
  1. the hovered slider,
  2. the slider being dragged,
  3. the monitor under the mouse cursor,
  4. the first monitor.

**Step:** ± `step_size` in normalized space, clamped to 0–1, never wraps.

**Cycle Presets:**
- Moves to the first preset greater than the current value, wrapping to the first preset.
- An empty preset list does nothing (no error).
- Applying a preset sets an absolute value.

### 5.4 Sync mode ("Lock")

- Toggled from the View menu. Not saved; it's off after a restart.
- Moving any slider applies the same **delta** to all monitors, in normalized space.
- **Independent saturation:** a monitor that hits 0 or 1 stops while the others keep moving.
- **No reverse compensation:** reversing direction moves on from the current values; lost offsets are not restored.

### 5.5 Persistence

- Saved per **identified** monitor in `monitor_states`:
  - `brightness_value`,
  - `normalized_value`,
  - `last_set_by_app`,
  - `write_confirmed`,
  - `timestamp` (Unix seconds),
  - `last_seen_device_path`.
- Save the value **written to the hardware** (`Monitor::written_value`), not the slider's value.
  - Successful write results mark settings dirty, and a one-shot timer (`SAVE_TIMER_ID`, 1 s, re-armed on every result) saves once writes settle.
  - Anything unsaved is also saved on `WM_CLOSE` and `WM_ENDSESSION`.
  - Monitors the app never wrote are not saved, so their earlier saved state survives.

### 5.6 Refresh

- **View → Refresh Monitors** finds the monitors again and resets per-monitor state.
- It should also restore saved values and refresh automatically on `WM_DISPLAYCHANGE` *(→ fix plan #18)*.

---

## 6. Configuration: `brightnessctl.json`

Stored **next to the EXE** (`Settings::default_path`), whatever the working directory. It's created with defaults on first run. Saves are atomic: the app writes `brightnessctl.json.tmp`, then renames it over the real file.

| Field | Type | Default | Notes |
|---|---|---|---|
| `theme` | `"Auto"` \| `"Light"` \| `"Dark"` | `"Auto"` | |
| `monitor_states` | map `"MFG:code:serial"` → state | `{}` | See §5.5 |
| `presets` | `f32[]` in 0–1 | `[0.0, 0.25, 0.5, 0.75, 1.0]` | Cycled in this order |
| `hotkeys` | map `Action` → `{ vkey, modifiers }` | See §5.3 | `modifiers`: Alt = 1, Ctrl = 2, Shift = 4, Win = 8 |
| `step_size` | `f32` in (0, 1] | `0.1` | Hotkey step size |

**Missing fields** get their defaults (`#[serde(default)]` on the struct).

**Out-of-range values** are fixed after loading:
- presets are clamped to 0–1, and non-finite presets are dropped,
- an invalid `step_size` falls back to the default.

**Broken file:**
- It is renamed to `brightnessctl.json.bak`, replacing any older backup.
- Defaults are used and written to a fresh config.
- The parse error (line and column) is shown in the status bar.
- The user's file is **never** overwritten: if the rename fails, nothing is written.

---

## 7. Build, test, CI, release

```powershell
cargo build              # debug
cargo test               # unit tests (some create real windows / enumerate monitors)
cargo clippy --all-targets
cargo build --release    # target\release\brightnessctl.exe, must stay < 1 MB
```

**CI** (`.github/workflows/ci.yml`) runs on `windows-latest` on pushes and PRs to `main`:
- `cargo check --locked`,
- `cargo test --locked`,
- `cargo build --release --locked`.

Fix plan #22 adds `cargo clippy --all-targets --locked -- -D warnings`.

**Release** (`.github/workflows/release.yml`) runs on a `v*` tag or manual trigger. It builds and packages:
- `brightnessctl-<tag>-windows-x86_64.zip` (EXE + default `brightnessctl.json` + README + LICENSE),
- the standalone EXE,
- SHA-256 checksums,

then publishes a GitHub Release with generated notes.

### Test hardware

| Machine | Displays | Use for |
|---|---|---|
| Desktop | Gigabyte `GBT2402` + Acer `ACR0B96`, both external DDC/CI | DDC/CI and sync mode |
| Laptop | Acer Aspire built-in panel | WMI, high-DPI scaling, hybrid-graphics device loss |

Useful checks:

```powershell
cargo test -- --ignored --nocapture --test-threads=1   # read-only hardware tests: identities, WMI matching, backend probe
Get-PnpDevice -Class Monitor -PresentOnly | Select InstanceId, FriendlyName
Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorBrightness
```

- Win+Ctrl+Shift+B restarts the graphics driver, which is useful for testing device-loss handling.

---

## 8. Development workflow

- Work on a branch, with **one commit per logical change**.
- Use conventional commits: `feat`, `fix`, `refactor`, `test`, `docs`, `style`, `chore`, e.g. `fix(monitors): match EDID by device instance ID`.
- Write unit tests alongside changes for any logic that can be tested on its own. Move logic out of Win32 handlers into pure functions so it can be tested.
- Mark tests that need real monitors with `#[ignore]`.
- Before committing, `cargo build`, `cargo test` and `cargo clippy` must all pass with no warnings.
- Check the release size after feature or dependency changes.
- Keep `docs/fix-plan.md` checkboxes up to date, and update this file when rules or architecture change.

### Code style

- **Readability:** clear over clever.
- **Consistency:** follow existing patterns and naming.
- **Simplicity:** prefer the simple solution; split complex problems into smaller parts.
- **Maintainability:** keep coupling low and dependencies few.
- **Comments:** comment sparingly, and explain *why* rather than *what*. Keep docs in sync with code.

---

## 9. History (summary of retired `conductor/` tracks)

| Track | Date | Delivered | Not delivered / known gaps |
|---|---|---|---|
| core_refinement | 2025-12-21 | Win32 window and DPI init, monitor enumeration, Direct2D/DirectWrite setup, basic hit-testing | Rendering doesn't handle DPI correctly (→ #3) |
| fix_warnings | 2025-12-21 | Removed unused fields (`hmonitor`, `hot_monitor`) | `hmonitor` was later re-added for cursor-based hotkey targeting |
| ui_polish | 2025-12-21 | Settings JSON, native menu, Light/Dark/Auto themes, responsive layout, status bar, hover/active slider states | Window position not saved (optional); status messages don't auto-clear; no color transitions |
| monitor_identity | 2025-12-22 | EDID identity and composite key, persistence model, rate limit and backoff, Smart Restore, sync mode, tooltips, status labels | EDID lookup never matched, so saving and restoring never worked (→ #1); connection-type label (HDMI/DP/USB-C) not built; observed min/max recorded but unused; manual checks for phases 1–3 never done |
| enhanced_input | 2025-12-22 | Global hotkeys, mouse wheel, typed values, preset cycling, hotkey failure indicator | Focus loss doesn't commit a typed value (→ #10); wheel and hotkey writes can be dropped (→ #6); promised tests for wheel and typed values missing (→ #10, #23) |

## 10. Roadmap (after the fix plan)

- System tray icon for quick access.
- Master slider: all monitors in unison. Sync mode partly covers this.
- Best-effort connection-type label (HDMI/DP/USB-C), never blocking control.
- Remember window position and size.
- Auto-clear status messages (one-shot timer).

**Out of scope:** complex animations, custom-drawn title bar, dark menu bar.
