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
 2. Download the `brightnessctl-vX.Y.Z-windows-x86_64.zip` bundle (includes default config & docs) or grab the standalone `brightnessctl.exe`.
 3. Run `brightnessctl.exe`. No installation required!

 ---

 ## ✨ Key Features

 *   **Zero Bloat:** Built with raw Win32 APIs, Direct2D, and DirectWrite. No Electron, no Qt, no WebViews, no .NET runtime.
 *   **Ultralight:** Release binary is **< 1 MB** thanks to aggressive link-time optimization (`lto = "fat"`, `opt-level = "z"`, stripped symbols).
 *   **Instant Startup:** Launches immediately with no splash screens or initialization lag.
 *   **Zero Idle CPU:** Completely dormant when not interacting with the user.
 *   **Native Windows Look & Feel:** Seamlessly integrates with Windows 11 aesthetics (Segoe UI, system colors, automatic light/dark mode awareness).
 *   **Universal Display Support:**
     *   **Internal Displays:** Managed via standard Windows brightness APIs.
     *   **External Displays:** Managed via hardware DDC/CI (VCP code `0x10`).
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

 1.  Launch `brightnessctl.exe`.
 2.  A clean native window will appear listing all detected displays.
 3.  Use the sliders to adjust brightness for individual monitors in real time.
 4.  *(Optional)* Use global hotkeys to adjust brightness on the fly without focusing the application window.

 ---

 ## ⚙️ Configuration

 A `brightnessctl.json` configuration file is automatically created in the same directory as the executable on first run. You can customize:

 *   **Theme:** `"Auto"` (follows Windows system theme), `"Light"`, or `"Dark"`.
 *   **Hotkeys:** Customize modifier keys and virtual key codes for:
     *   `StepUp` (Increase brightness)
     *   `StepDown` (Decrease brightness)
     *   `CyclePresets` (Switch between configured levels)
 *   **Presets:** Define your preferred brightness percentages (e.g., `[0.10, 0.50, 1.00]`).
 *   **Step Size:** Adjust the increment step for hotkey actions (default `0.05` / 5%).

 ---

 ## 🏗️ Architecture

 This project strictly adheres to a "no-framework" philosophy to maximize performance and minimize resource usage:

 *   **Language:** Rust 2024 Edition
 *   **Windowing & Event Loop:** Native Win32 API (`CreateWindowExW`, `GetMessageW`)
 *   **Hardware Acceleration:** Direct2D (geometry & shapes) + DirectWrite (typography)
 *   **Monitor Protocol:** `Dxva2` & High-Level Monitor Configuration API (DDC/CI)
 *   **State Management:** Pure event-driven data structures without polling timers

 ---

 ## 📝 License

 This project is licensed under the MIT License — see the LICENSE file for details.
