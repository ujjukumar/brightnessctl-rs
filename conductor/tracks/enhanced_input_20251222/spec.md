# Specification: Enhanced Input & Hotkeys

## 1. Overview
This feature track introduces advanced input methods to the brightness controller, aiming to improve accessibility and precision for power users. It includes global hotkeys for brightness control, mouse wheel support on sliders, and direct numeric entry for precise value setting.

## 2. Functional Requirements

### 2.1 Global Hotkeys
*   **Architecture:**
    *   Hotkeys must be data-driven, mapping Enums to Actions.
    *   Key bindings must be stored in the configuration file (`brightnessctl.json`), even if no UI exists to edit them yet.
    *   **Failure Handling:** Registration failures (e.g., conflicts) must be handled gracefully. The conflicting hotkey is disabled, and the failure status must be observable in the UI. **Persistence:** Registration failure state is reflected in runtime state and is not retried implicitly without a config change or application restart.
*   **Default Bindings:**
    *   `Ctrl + Alt + Up`: Step Brightness Up
    *   `Ctrl + Alt + Down`: Step Brightness Down
    *   `Ctrl + Alt + P`: Cycle Presets
*   **Targeting Rules:**
    *   **Lock Mode Active:** Actions (Step, Preset) apply globally (to all monitors).
    *   **Lock Mode Inactive:**
        *   If a monitor is hovered or has focus -> Action applies to that monitor.
        *   Else -> Action applies to the Primary Monitor (deterministic fallback).
*   **Actions:**
    *   **Step Up/Down:** Increases/decreases brightness by a fixed step (defined in config, default 0.1) in **normalized space (0.0–1.0)**.
        *   **Clamping:** Values must clamp to [0.0, 1.0] and must **never wrap**.
    *   **Cycle Presets:** Cycles through the list of presets defined in `brightnessctl.json`.
        *   Order is strictly as defined in the file.
        *   Applying a preset sets an absolute **normalized value (0.0–1.0)**, overriding any previous relative step offsets.
        *   **Empty Config:** If the preset list is empty or invalid, the Cycle Presets action is a **no-op** and must not error.

### 2.2 Mouse Wheel Support
*   **Interaction Model:**
    *   **Hover-to-Scroll:** Scrolling the mouse wheel while hovering over a slider adjusts its value. Focus is not required.
    *   **Conflict Resolution:** If multiple sliders overlap (edge cases), only the topmost hovered control receives wheel input.
*   **Precision:**
    *   **Coarse:** Standard scroll increments brightness by a coarse step (e.g., 0.05 or 0.1).
    *   **Fine:** Holding `Shift` while scrolling increments brightness by a fine step (e.g., 0.01).
*   **Constraints:**
    *   Wheel delta maps to **normalized space (0.0–1.0)**, not raw DDC units.
    *   Values are clamped at 0.0 and 1.0; wrapping is strictly forbidden.
    *   **Rate Limiting:** Wheel-generated writes must be rate-limited identically to drag events to prevent flooding the DDC/CI bus.
    *   **Modifiers:** Only `Shift` is supported for fine control. No other modifiers (Ctrl/Alt) are interpreted to avoid conflicts with global hotkeys.

### 2.3 Numeric Entry
*   **Interaction:**
    *   **Inline Edit:** Clicking the percentage text label (displayed as 0–100%) turns it into an editable text field.
    *   **Commit:** Pressing `Enter` or losing focus (blur) commits the value.
    *   **Cancel:** Pressing `Esc` reverts the change.
*   **Hardware Write Timing:**
    *   Commit triggers **exactly one write**.
    *   No intermediate writes occur during typing.
    *   The commit path utilizes the same rate-limited pipeline as other inputs.
*   **Validation & Formatting:**
    *   Input must be numeric.
    *   Values are parsed from 0–100 integer input to **0.0–1.0 normalized float**.
    *   Values are clamped between 0.0 and 1.0.
    *   Invalid input reverts to the previous value.
    *   **Display Normalization:** After commit or revert, the label must re-render as a **canonical integer percentage** (no decimals).

### 2.4 General Constraints
*   **Normalization:** All internal calculations and storage use **normalized space (0.0–1.0)**. Percentage (0–100%) is for **presentation only**.
*   **Idempotency:** Repeated actions (like setting the same brightness) should be handled efficiently (no-op if value hasn't changed). This applies to **all input paths**: hotkeys, wheel, numeric entry, and preset application.
*   **Provenance:** All brightness changes must use the established "Smart Restore" / provenance-aware write path to ensure consistency.

## 3. Non-Functional Requirements
*   **Performance:** Hotkey handling and mouse wheel events must be processed with minimal latency.
*   **Reliability:** Global hooks must be cleaned up properly on application exit.
*   **Compatibility:** Must work on Windows 11 using native Win32 APIs.

## 4. Acceptance Criteria
*   [ ] Global hotkeys change brightness based on targeting rules (Lock/Hover/Primary).
*   [ ] Hotkey actions clamp values at 0.0/1.0 and do not wrap.
*   [ ] Hotkey registration failures are visibly indicated and state is persisted for the session.
*   [ ] Mouse wheel adjusts slider when hovering (topmost only), with `Shift` for fine control.
*   [ ] Mouse wheel inputs operate in normalized space, are clamped, and are rate-limited.
*   [ ] Numeric entry commits exactly one write, only on Enter/Blur, and reverts on Esc.
*   [ ] Numeric label always reformats to integer percentage after commit/revert.
*   [ ] Presets cycle in exact config order; empty config results in no-op.
*   [ ] All input methods (Keys, Wheel, Numeric) are idempotent and use the provenance-aware write path.
