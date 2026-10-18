# brightnessctl

 [![Rust](https://img.shields.io/badge/Rust-2024%20Edition-DEA584.svg?logo=rust&logoColor=white)](https://www.rust-lang.org/)
 [![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011-0078D6.svg?logo=windows&logoColor=white)](https://www.microsoft.com/windows)
 [![Binary Size](https://img.shields.io/badge/Binary%20Size-%3C%201%20MB-brightgreen.svg)]()
 [![GitHub Release](https://img.shields.io/github/v/release/ujjukumar/brightnessctl-rs?logo=github)](https://github.com/ujjukumar/brightnessctl-rs/releases)
 [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

 **brightnessctl** is an ultra-lightweight, native Windows 11 utility for controlling display brightness. Designed for power users and minimalists who demand efficiency, it provides precise control over
  both internal laptop screens and external DDC/CI desktop monitors without the overhead of heavy UI frameworks or runtimes.

 ---

 ## 📥 Download

 Pre-built, optimized binaries are available on the [Releases](https://github.com/ujjukumar/brightnessctl-rs/releases) page:

 1. Head to [Latest Release](https://github.com/ujjukumar/brightnessctl-rs/releases/latest).
 2. Download the standalone `brightnessctl.exe`, or the `brightnessctl-vX.Y.Z-windows-x86_64.zip` bundle (EXE, README and license). `checksums.txt` has the SHA-256 of both.
 3. Run `brightnessctl.exe`. No installation required!

 ---

 ## ✨ Key Features

 *   **Zero Bloat:** Built with raw Win32 APIs, Direct2D, and DirectWrite. No Electron, no Qt, no WebViews, no .NET runtime.
 *   **Ultralight:** Release binary is **< 1 MB** (about 270 KB) thanks to aggressive link-time optimization (`lto = "fat"`, `opt-level = "z"`, stripped symbols).
 *   **Instant Startup:** The window opens immediately. Monitors are detected in the background, so a slow monitor never holds up the UI.
 *   **Zero Idle CPU:** Nothing runs while you aren't using it: no polling, no periodic timers.
 *   **Native Windows Look & Feel:** Segoe UI, native title bar and menu, light/dark mode that follows Windows (title bar included), and sharp text at any display scaling.
 *   **Universal Display Support:**
     *   **Internal Displays:** Built-in laptop panels are controlled through WMI (`WmiMonitorBrightness`).
     *   **External Displays:** DDC/CI monitors are controlled through the Windows monitor API, falling back to raw VCP code `0x10`.
 *   **Remembers Brightness:** Each monitor is recognized by its EDID, and the brightness the app last set is restored at startup.
 *   **Global Hotkeys:** Configurable system-wide shortcuts for background brightness adjustments.

 ---

 ## 🛠️ Build Instructions

 ### Prerequisites
 *   Windows 10 or 11 (x64)
 *   [Rust](https://www.rust-lang.org/tools/install) (Stable MSVC toolchain)

 ### Steps

 1.  **Clone the repository:**
     ```bash
     git clone https://github.com/ujjukumar/brightnessctl-rs.git
     cd brightnessctl-rs
     ```

 2.  **Build for release:**
     ```bash
     cargo build --release
     ```

 3.  **Run:**
     The standalone executable will be located at:
     ```bash
     .\target\release\brightnessctl.exe
     ```

 ---

 ## 🎮 Usage

 1.  Launch `brightnessctl.exe`. Each detected display gets a slider, which shows `Detecting…` for a moment while the monitor is probed. Only one copy runs at a time: launching it again brings the open window forward.
 2.  Adjust brightness:
     *   **Drag** or click a slider.
     *   **Scroll** over a slider: 5% per notch, or 1% while holding `Shift`.
     *   **Click the percentage** to type a value. `Enter` or clicking elsewhere applies it; `Esc` cancels.
 3.  **View → Sync All Monitors (Lock)** moves every monitor by the same amount.
 4.  **View → Refresh Monitors** finds the displays again. This also happens automatically when a display is connected or disconnected.
 5.  Global hotkeys work without focusing the window: `Ctrl+Alt+Up` / `Ctrl+Alt+Down` step the brightness and `Ctrl+Alt+P` cycles presets. They act on the monitor under the mouse cursor, or on every monitor in sync mode.

 ---

 ## ⚙️ Configuration

 Settings live in `%APPDATA%\brightnessctl\brightnessctl.json`, which is created with defaults on first run. A `brightnessctl.json` that an older version left next to `brightnessctl.exe` is moved there automatically. Edit the file while the app is closed, since the app rewrites it when it saves. You can customize:

 *   **Theme:** `"Auto"` (follows Windows), `"Light"`, or `"Dark"`. Also available from the **Theme** menu.
 *   **Hotkeys:** a virtual key code (`vkey`) and `modifiers` (`Alt = 1`, `Ctrl = 2`, `Shift = 4`, `Win = 8`, added together) for:
     *   `StepUp` (Increase brightness)
     *   `StepDown` (Decrease brightness)
     *   `CyclePresets` (Switch between configured levels)
 *   **Presets:** Brightness levels from 0 to 1, cycled in order (default `[0.0, 0.25, 0.5, 0.75, 1.0]`).
 *   **Step Size:** The hotkey step (default `0.1` / 10%).

 `monitor_states` is written by the app to remember each monitor's brightness. Missing fields fall back to their defaults and out-of-range values are corrected. If the file can't be parsed, it is renamed to `brightnessctl.json.bak` in the same folder, defaults are used, and the status bar shows the line and column of the error.

 ---

 ## 🏗️ Architecture

 This project strictly adheres to a "no-framework" philosophy to maximize performance and minimize resource usage:

 *   **Language:** Rust 2024 Edition
 *   **Windowing & Event Loop:** Native Win32 API (`CreateWindowExW`, `GetMessageW`)
 *   **Hardware Acceleration:** Direct2D (geometry & shapes) + DirectWrite (typography), Per-Monitor DPI aware
 *   **Monitor Protocol:** `Dxva2` High-Level Monitor Configuration API and VCP `0x10` (DDC/CI); WMI for built-in panels
 *   **Monitor I/O:** One worker thread per monitor, so slow DDC/CI never blocks the window; rapid changes are coalesced to the latest value
 *   **State Management:** Event-driven, no polling; only one-shot timers (delayed save, display-change refresh)

 ---

 ## 🚀 Contributing & Releases

 *   Changes reach `main` only through pull requests. CI must pass first: `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, and a release build that has to stay under 1 MB.
 *   Every merged pull request publishes a new [release](https://github.com/ujjukumar/brightnessctl-rs/releases) automatically. The patch number goes up by default; add the `minor` or `major` label to the pull request to bump that part instead.

 ---

 ## 📝 License

 This project is licensed under the MIT License — see the LICENSE file for details.
