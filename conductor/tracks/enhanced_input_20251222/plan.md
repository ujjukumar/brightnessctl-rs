# Plan: Enhanced Input & Hotkeys

## Phase 1: Foundation & Action System [checkpoint: c1539f5]
**Goal:** Implement the data-driven action system and normalized arithmetic required for all input methods.

- [x] Task: Define `Action` and `Command` Enums in `state.rs` or a new `actions.rs`.
- [x] Task: Implement `Targeting` logic (Lock Mode vs Hover vs Primary Monitor).
- [x] Task: Implement normalized space (0.0-1.0) step arithmetic with clamping and idempotency checks.
- [x] Task: Unit tests for targeting rules and normalized arithmetic.
- [x] Task: Conductor - User Manual Verification 'Phase 1: Foundation & Action System' (Protocol in workflow.md)

## Phase 2: Configuration & Metadata [checkpoint: 1d9ab79]
**Goal:** Update settings to support hotkeys and ensure presets are handled correctly.

- [x] Task: Update `Settings` struct in `settings.rs` to include hotkey bindings and ensure preset order is preserved.
- [x] Task: Implement logic to load/save hotkey bindings from `brightnessctl.json`.
- [x] Task: Unit tests for settings serialization and preset preservation.
- [x] Task: Conductor - User Manual Verification 'Phase 2: Configuration & Metadata' (Protocol in workflow.md)

## Phase 3: Global Hotkeys [checkpoint: 7523bc3]
**Goal:** Implement system-wide hotkey registration and handling.

- [x] Task: Implement `HotkeyManager` to wrap Win32 `RegisterHotKey` and `UnregisterHotKey`.
- [x] Task: Implement registration failure handling and surface status to `AppState`.
- [x] Task: Integrate `WM_HOTKEY` into the main message loop in `window.rs`.
- [x] Task: Map hotkey events to the action system.
- [x] Task: Unit tests for hotkey registration logic and action mapping.
- [x] Task: Conductor - User Manual Verification 'Phase 3: Global Hotkeys' (Protocol in workflow.md)

## Phase 4: Mouse Wheel Support [checkpoint: 7632ed8]
**Goal:** Add hover-based scroll support for sliders.

- [x] Task: Implement hover detection in `window.rs` or `render.rs` to identify the topmost slider.
- [x] Task: Handle `WM_MOUSEWHEEL` and map deltas to normalized space (0.0-1.0).
- [x] Task: Implement coarse vs fine (`Shift`) increments.
- [x] Task: Ensure wheel events are rate-limited and use the provenance-aware write path.
- [x] Task: Unit tests for wheel delta mapping and rate limiting.
- [x] Task: Conductor - User Manual Verification 'Phase 4: Mouse Wheel Support' (Protocol in workflow.md)

## Phase 5: Numeric Entry [checkpoint: 06c3e8e]
**Goal:** Implement inline editing for percentage labels.

- [x] Task: Add `editing_monitor` state to `AppState` to track which label is being edited.
- [x] Task: Implement UI transition from label to text input on click.
- [x] Task: Handle `WM_CHAR`, `VK_RETURN`, `VK_ESCAPE`, and focus loss (`WM_KILLFOCUS`).
- [x] Task: Implement parsing (0-100 -> 0.0-1.0) and single-write commit logic.
- [x] Task: Unit tests for numeric parsing, clamping, and commit behavior.
- [x] Task: Conductor - User Manual Verification 'Phase 5: Numeric Entry' (Protocol in workflow.md)

## Phase 6: Preset Cycling [checkpoint: 6cc35d3]
**Goal:** Implement the preset cycling action.

- [x] Task: Implement `cycle_presets` logic in `brightness.rs` or `state.rs`.
- [x] Task: Handle empty or invalid preset configurations gracefully.
- [x] Task: Connect preset cycling to the hotkey action.
- [x] Task: Unit tests for cycling order and edge cases.
- [x] Task: Conductor - User Manual Verification 'Phase 6: Preset Cycling' (Protocol in workflow.md)

## Phase 7: UI Polish & Final Integration
**Goal:** Finalize failure indicators and ensure consistency across all input paths.

- [~] Task: Implement UI indicator (e.g., tooltip or icon) for hotkey registration failures.
- [ ] Task: Verify all input paths (Keys, Wheel, Numeric) share the same idempotent write pipeline.
- [ ] Task: Final end-to-end manual verification of all features.
- [ ] Task: Conductor - User Manual Verification 'Phase 7: UI Polish & Final Integration' (Protocol in workflow.md)
