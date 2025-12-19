# Brightness Controller — Implementation Instructions (Rust / Windows 11)

## Objective
Implement a **fast, minimal, Windows-native brightness controller** in Rust.

Constraints:
- Windows 11 only
- Rust (stable, MSVC toolchain)
- egui for UI
- Native Windows APIs for brightness
- Single EXE, optimized for size and startup
- No background polling
- No cross-platform abstractions

The project already:
- Builds and runs
- Displays an egui window
- Initializes COM (Step 7 complete)

You must implement **monitor enumeration + brightness control** and wire it to the UI.

---

## Architecture (MANDATORY)

```

src/
├─ main.rs          # egui app + wiring
├─ win.rs           # COM init (already exists)
├─ monitors.rs      # monitor discovery + handles
├─ brightness.rs    # brightness get/set logic

````

Rules:
- No logic in `main.rs` except orchestration.
- No Windows API calls inside egui code.
- No global mutable state.
- Cache monitor handles once at startup.

---

## Srep 1 to 7 are already complete.

## Step 8 — Monitor Enumeration

### Goal
Enumerate all physical monitors and store handles for later brightness control.

### Requirements
- Use `EnumDisplayMonitors`
- For each `HMONITOR`, obtain physical monitor handles using:
  - `GetNumberOfPhysicalMonitorsFromHMONITOR`
  - `GetPhysicalMonitorsFromHMONITOR`
- Store:
  - `HMONITOR`
  - `HANDLE` to physical monitor
  - Friendly name (if available)

### Output
Define a struct:
```rust
pub struct Monitor {
    pub hmonitor: HMONITOR,
    pub physical: HANDLE,
}
````

Provide:

```rust
pub fn enumerate_monitors() -> Vec<Monitor>
```

### Rules

* Enumeration happens **once**.
* No polling.
* Free physical monitor handles on drop.

---

## Step 9 — Brightness Control (Core Logic)

### Split logic by display type

#### Internal Displays (Laptop Panels)

Use:

* `GetMonitorBrightness`
* `SetMonitorBrightness`

#### External Displays

Use **DDC/CI**:

* `GetVCPFeatureAndVCPFeatureReply`
* `SetVCPFeature`
* Brightness VCP code: `0x10`

### API

In `brightness.rs` define:

```rust
pub fn get_brightness(monitor: &Monitor) -> Option<u32>
pub fn set_brightness(monitor: &Monitor, value: u32) -> bool
```

### Rules

* Brightness range normalized to **0–100**
* Convert to native ranges internally
* No WMI unless Dxva2 fails
* No retries, no loops

---

## Step 10 — UI Wiring (egui)

### UI Requirements

* Single window
* One brightness slider per monitor
* Slider range: 0–100
* Change brightness **only when slider value changes**
* No timers
* No async
* No background threads

### Flow

* On app init:

  * Enumerate monitors
  * Query initial brightness
* In `update()`:

  * Render sliders
  * On user change → call `set_brightness`

---

## Step 11 — Error Handling Rules

* Silent failure is acceptable
* Do not panic on API failure
* Log errors only in debug builds
* Never crash the app due to a bad monitor

---

## Step 12 — Build Constraints

### Cargo.toml (already set, do not modify)

```toml
[profile.release]
lto = true
panic = "abort"
strip = true
codegen-units = 1
```

### Target

* `cargo build --release`
* Resulting EXE should be ~1–3 MB

---

## Hard Prohibitions

DO NOT:

* Use WMI polling
* Use background threads
* Use async runtimes
* Use WinUI / XAML / MAUI / Avalonia
* Add cross-platform crates
* Add configuration files
* Add telemetry or logging frameworks

---

## Definition of Done

* App launches instantly
* Sliders adjust brightness correctly
* External monitors work via DDC/CI
* Laptop panel works
* No CPU usage while idle
* Single EXE, no dependencies

The compiler is the authority.
