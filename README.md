# brightnessctl

A tiny native Windows utility for controlling display brightness — one slider per monitor, for internal panels and DDC/CI desktop displays alike. A single ~290 KB `.exe`, no installer, no runtime.

## Download

Grab `brightnessctl.exe` from the [latest release](https://github.com/ujjukumar/brightnessctl-rs/releases/latest) and run it. Every release ships that one file.

## Usage

1.  Run `brightnessctl.exe`. Each detected display gets a slider; a slow monitor shows `Detecting…` while it is probed. Only one copy runs at a time — launching it again brings the open window forward.
2.  Adjust brightness:
    *   **Drag** or click a slider.
    *   **Scroll** over a slider: 5% per notch, or 1% while holding `Shift`.
    *   **Click the percentage** to type a value. `Enter` or clicking elsewhere applies it; `Esc` cancels.
3.  The controls under the sliders:
    *   **Auto**, **Light**, **Dark**, **Glass** pick the theme.
    *   **Sync** moves every display together; the status bar shows `SYNC ACTIVE` while it is on.
    *   **Picture…** shows a picture (PNG, JPG, BMP or GIF) behind the sliders; **Remove**, which appears only while a picture is set, clears it again.
4.  Global hotkeys work without focusing the window: `Ctrl+Alt+Up` / `Ctrl+Alt+Down` step the brightness and `Ctrl+Alt+P` cycles presets. They act on the display under the mouse cursor, or on every display in sync mode.

## Build

```bash
cargo build --release
```

The executable lands in `target\release\brightnessctl.exe`. Rust stable with the MSVC toolchain, Windows 10 or 11.

## Configuration

Settings live in `%APPDATA%\brightnessctl\brightnessctl.json`, created with defaults on first run. Edit it while the app is closed, since the app rewrites it when it saves.

*   **`theme`:** `"Auto"` (follows Windows), `"Light"`, `"Dark"` or `"Glass"`.
*   **`background_image`:** the picture's full path, or `null`. **`background_dim`** fades it from `0.0` (full strength) to `1.0` (hidden), default `0.6`.
*   **`hotkeys`:** a virtual key code (`vkey`) and `modifiers` (`Alt = 1`, `Ctrl = 2`, `Shift = 4`, `Win = 8`, added together) for `StepUp`, `StepDown` and `CyclePresets`.
*   **`presets`:** brightness levels cycled in order (default `[0.0, 0.25, 0.5, 0.75, 1.0]`). **`step_size`** is the hotkey step (default `0.1`).

`monitor_states` is written by the app to remember each display's brightness. A file that can't be parsed is renamed to `brightnessctl.json.bak`, defaults are used, and the status bar shows where the error is.

## License

MIT — see [LICENSE](LICENSE).
