# brightnessctl

**brightnessctl** is a lightweight, native Windows 11 utility for controlling display brightness. Designed for power users who demand efficiency, it provides precise control over both internal screens and external DDC/CI monitors without the bloat of modern UI frameworks.

## ✨ Key Features

*   **Zero Bloat:** Built with raw Win32 APIs, Direct2D, and DirectWrite. No Electron, no Qt, no .NET.
*   **Ultralight:** Release binary is **< 1 MB**.
*   **Instant Startup:** Launches immediately with no splash screens or loading delays.
*   **Zero Idle CPU:** Completely dormant when not interacting with the user.
*   **Native Look & Feel:** Seamlessly integrates with Windows 11 aesthetics (Segoe UI, system colors, light/dark mode awareness).
*   **Universal Support:** Controls:
    *   **Internal Displays:** Via standard Windows brightness APIs.
    *   **External Monitors:** Via DDC/CI (VCP code `0x10`).
*   **Global Hotkeys:** Configurable system-wide shortcuts for brightness adjustments.

## 🛠️ Build Instructions

### Prerequisites
*   Windows 10 or 11
*   [Rust](https://www.rust-lang.org/tools/install) (Stable MSVC toolchain)

### Steps

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/yourusername/brightnessctl.git
    cd brightnessctl
    ```

2.  **Build for release:**
    ```bash
    cargo build --release
    ```
    *Note: Release mode includes specific optimizations (`lto = "fat"`, `strip = "symbols"`) to ensure the small binary size.*

3.  **Run:**
    The executable will be located at `target/release/brightnessctl.exe`.

## 🎮 Usage

1.  Launch `brightnessctl.exe`.
2.  A window will appear listing all detected monitors.
3.  Use the sliders to adjust brightness for each display.
4.  (Optional) Use global hotkeys to adjust brightness without focusing the app.

## ⚙️ Configuration

A `brightnessctl.json` file is automatically generated in the same directory as the executable. You can edit this file to customize:

*   **Theme:** `"Auto"` (System), `"Light"`, or `"Dark"`.
*   **Hotkeys:** Customize modifier keys and virtual key codes for:
    *   `StepUp` (Increase brightness)
    *   `StepDown` (Decrease brightness)
    *   `CyclePresets` (Switch between configured brightness levels)
*   **Presets:** Define your preferred brightness percentages (e.g., `[0.0, 0.5, 1.0]`).
*   **Step Size:** Adjust the increment for step up/down actions.

## 🏗️ Architecture

This project strictly adheres to a "no-framework" philosophy to maximize performance:

*   **Language:** Rust 2024
*   **Windowing:** Win32 API (`CreateWindowExW`, `GetMessageW`)
*   **Rendering:** Direct2D (Shapes) + DirectWrite (Text)
*   **Monitor Control:** `Dxva2` & `High Level Monitor Configuration API`
*   **State Management:** Pure data structures, no polling loops.

## 📝 License

[Insert License Here]
