# Brightness Control — Native Windows Implementation Instructions

## Objective
Build a **native, professional, Windows 11 brightness controller** with:
- Instant startup
- < 1 MB release binary
- Native Windows look (not “custom themed”)
- Zero background CPU usage
- No framework bloat

Target audience: power users.  
Tone: system utility, not consumer app.

---

## Technology Constraints (NON-NEGOTIABLE)

- Language: **Rust (stable, MSVC toolchain)**
- Platform: **Windows 11 only**
- UI: **Raw Win32 + Direct2D + DirectWrite**
- APIs: **Dxva2 + DDC/CI**
- Build: `cargo build --release`
- Output: **single EXE**

DO NOT use:
- egui, wgpu, winit
- WinUI, XAML, MAUI, WPF
- Qt, GTK
- Electron / WebView
- Cross-platform UI crates
- Async runtimes

---

## Architecture (MANDATORY)

```

src/
├─ main.rs              # App entry + message loop
├─ win.rs               # COM + DPI init
├─ window.rs            # Win32 window creation
├─ render.rs            # Direct2D / DirectWrite renderer
├─ monitors.rs          # Monitor enumeration
├─ brightness.rs        # Brightness get/set logic
└─ state.rs             # App state (pure data)

````

Rules:
- UI rendering is stateless; state lives in `state.rs`
- No global mutable state
- No polling loops
- No threads

---

## Step 1 — Window Creation

Create a **bordered, resizable Win32 window** using:
- `CreateWindowExW`
- Standard message loop (`GetMessageW`)

Requirements:
- Proper DPI awareness (`SetProcessDpiAwarenessContext`)
- Dark background
- System title bar (do NOT custom-draw it)

---

## Step 2 — Rendering Stack

Initialize:
- `ID2D1Factory`
- `ID2D1HwndRenderTarget`
- `IDWriteFactory`

Use:
- Direct2D for shapes
- DirectWrite for text

NO GDI rendering except for fallback.

---

## Step 3 — Visual Design Rules

This is NOT subjective.

### Layout
- Vertical stack
- Fixed-width content column
- Consistent spacing: 8 / 16 / 24 px only

### Typography
- Font: **Segoe UI**
- Title: 20–22 px
- Body: 13–14 px
- Labels: 12 px, subdued color

### Color
- Background: near-black neutral
- Foreground: off-white
- Accent: single muted blue or system accent
- No gradients
- No shadows

### Controls
- Custom-drawn sliders
- Large hit targets
- Subtle hover feedback
- Numeric percentage aligned right

This should resemble **a Windows system utility**, not a “styled app”.

---

## Step 4 — Monitor Enumeration

Implement in `monitors.rs`.

- Use `EnumDisplayMonitors`
- For each `HMONITOR`:
  - Retrieve physical monitors using:
    - `GetNumberOfPhysicalMonitorsFromHMONITOR`
    - `GetPhysicalMonitorsFromHMONITOR`
- Store:
```rust
pub struct Monitor {
    pub hmonitor: HMONITOR,
    pub physical: HANDLE,
    pub name: String,
}
````

Enumerate **once at startup**.

Free handles on drop.

---

## Step 5 — Brightness Control

Implement in `brightness.rs`.

### Internal displays

* `GetMonitorBrightness`
* `SetMonitorBrightness`

### External displays

* DDC/CI:

  * `GetVCPFeatureAndVCPFeatureReply`
  * `SetVCPFeature`
  * VCP code `0x10`

Expose:

```rust
pub fn get_brightness(m: &Monitor) -> Option<u32>
pub fn set_brightness(m: &Monitor, value: u32) -> bool
```

Normalize brightness to **0–100**.

DO NOT poll.
DO NOT use WMI unless Dxva2 fails.

---

## Step 6 — Input Handling

* Mouse:

  * Hit-testing sliders
  * Click + drag
* Keyboard:

  * Optional: arrow keys for fine control

Update brightness **only on value change**.

---

## Step 7 — App State

`state.rs` holds:

```rust
pub struct AppState {
    pub monitors: Vec<Monitor>,
    pub brightness: Vec<u32>,
    pub hot_monitor: Option<usize>,
}
```

Rendering reads state.
Input mutates state.
Brightness writes are explicit.

---

## Step 8 — Message Loop Rules

* Render only on:

  * `WM_PAINT`
  * Input events
* No timers
* No background redraws
* CPU usage must be ~0% when idle

---

## Step 9 — Error Handling

* Silent failure is acceptable
* Never crash due to bad monitor
* Log only in debug builds
* No dialogs

This is a utility, not a wizard.

---

## Step 10 — Binary Optimization

Ensure:

```toml
[profile.release]
opt-level = "z"
lto = "fat"
panic = "abort"
strip = "symbols"
codegen-units = 1
```

Target:

* **< 1 MB EXE**
* Instant startup

---

## Definition of Done

* Native Windows look
* External + internal monitors work
* One slider per monitor
* No idle CPU usage
* No background threads
* No runtime dependencies
* Professional, restrained UI

If anything feels “framework-like”, it is wrong.

The compiler and Windows APIs are the only authorities.
