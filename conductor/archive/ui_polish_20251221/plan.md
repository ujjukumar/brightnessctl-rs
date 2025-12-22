# Plan: UI Polish & Feature Expansion

## Phase 1: Settings & Persistence [checkpoint: f752d03]

- [x] Task: Implement Settings infrastructure f752d03
    - [x] Create `src/settings.rs` to handle JSON serialization/deserialization of `Settings` struct (Theme preference, last window size, etc.). f752d03
    - [x] Update `AppState` in `src/state.rs` to include a `Settings` instance. f752d03
    - [x] Write tests for settings loading/saving to ensure portability. f752d03
- [x] Task: Integrate Settings with Application Startup f752d03
    - [x] Update `main.rs` to load settings on boot. f752d03
    - [x] Ensure the application creates a default `brightnessctl.json` if none exists. f752d03
- [x] Task: Conductor - User Manual Verification 'Phase 1: Settings & Persistence' (Protocol in workflow.md) f752d03

## Phase 2: Native Menu Bar & Theming Logic [checkpoint: a1253bd]

- [x] Task: Implement Native Win32 Menu a1253bd
    - [x] Create menu resources/logic in `src/window.rs` or a new `src/menu.rs`. a1253bd
    - [x] Add menu items for File, View, Theme (Auto/Light/Dark), and Help. a1253bd
    - [x] Handle `WM_COMMAND` in `wnd_proc` to react to menu selections. a1253bd
- [x] Task: Implement Theming Engine a1253bd
    - [x] Define `ColorPalette` for Light and Dark modes in `src/render.rs` or `src/state.rs`. a1253bd
    - [x] Implement system theme detection (Auto mode) using Windows registry or APIs. a1253bd
    - [x] Update `AppState` to reflect the active theme colors. a1253bd
- [x] Task: Conductor - User Manual Verification 'Phase 2: Native Menu Bar & Theming Logic' (Protocol in workflow.md) a1253bd

## Phase 3: Responsive Layout & Status Bar [checkpoint: 8d0fc46]

- [x] Task: Implement Responsive Rendering 8d0fc46
    - [x] Refactor `render.rs` to calculate layout dynamically based on current window `RECT`. 8d0fc46
    - [x] Ensure sliders and labels stretch or align correctly on resize. 8d0fc46
- [x] Task: Implement Status Bar 8d0fc46
    - [x] Add `status_message` field to `AppState`. 8d0fc46
    - [x] Update `render.rs` to draw a dedicated status bar area at the bottom. 8d0fc46
    - [x] Implement a basic "Status Manager" to clear messages after a timeout (if feasible without timers/threads, perhaps via message loop timestamps). 8d0fc46
- [x] Task: Conductor - User Manual Verification 'Phase 3: Responsive Layout & Status Bar' (Protocol in workflow.md) 8d0fc46

## Phase 4: Custom Polished Controls & Interaction [checkpoint: 248cf66]

- [x] Task: Enhance Sliders with State Awareness 248cf66
    - [x] Update `AppState` or a new input state to track "Hovered" and "Active" monitor indices. 248cf66
    - [x] Update `main.rs` (`WM_MOUSEMOVE`, `WM_MOUSELEAVE`) to track which slider is hovered. 248cf66
    - [x] Update `render.rs` to draw sliders differently based on their state (Normal, Hover, Dragging). 248cf66
- [x] Task: Implement Visual Polish & Transitions 248cf66
    - [x] Add subtle color interpolation for hover states (smooth transitions). 248cf66
    - [x] Refine Segoe UI typography and spacing (8/16/24px grid) across all elements. 248cf66
- [x] Task: Final Verification & Optimization 248cf66
    - [x] Run `cargo check` and `cargo test` to ensure stability. 248cf66
    - [x] Profile release build to confirm <1MB size and instant startup. 248cf66
- [x] Task: Conductor - User Manual Verification 'Phase 4: Custom Polished Controls & Interaction' (Protocol in workflow.md) 248cf66
