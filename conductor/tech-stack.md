# Technology Stack

## Core Language & Toolchain
- **Rust (Stable):** Chosen for its performance, memory safety, and zero-cost abstractions, essential for a high-performance system utility.
- **Serde:** Used for robust and efficient JSON serialization/deserialization of application settings.
- **MSVC Toolchain:** Required for native Windows integration and compatibility with system libraries.

## Operating System & Platform
- **Windows 11:** The application is built exclusively for Windows 11, utilizing modern system APIs and adhering to its visual standards.

## Graphics & UI Stack
- **Win32 API:** Used for core window management (`CreateWindowExW`), message loop (`GetMessageW`), and DPI awareness.
- **Direct2D:** Utilized for high-performance, hardware-accelerated 2D shape rendering.
- **DirectWrite:** Employed for professional-grade text layout and rendering using the Segoe UI typeface.
- **COM (Component Object Model):** Used for initializing and managing graphics factories and interfaces.

## Display & Brightness APIs
- **Dxva2 (DirectX Video Acceleration):** Used for controlling brightness on internal (laptop) displays.
- **DDC/CI (Display Data Channel / Command Interface):** Used for communication with external monitors to retrieve and set VCP features (specifically VCP code `0x10`).
- **Registry API:** Used for system-wide preference detection, such as Light/Dark mode.

## Build & Optimization
- **Cargo:** The standard Rust build system, configured with a high-optimization release profile to ensure a single EXE output under 1MB.
- **Zero-Dependency Approach:** Avoidance of external UI frameworks (egui, wgpu, etc.) to minimize bloat and ensure instant startup.
