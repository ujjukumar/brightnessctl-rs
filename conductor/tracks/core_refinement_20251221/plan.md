# Plan: Refine Core Implementation

## Phase 1: Foundational Infrastructure
- [ ] Task: Complete Win32 Window Class registration and DPI awareness initialization in `win.rs` and `window.rs`.
- [ ] Task: Implement robust monitor enumeration in `monitors.rs` using `EnumDisplayMonitors` and `GetPhysicalMonitorsFromHMONITOR`.
- [ ] Task: Conductor - User Manual Verification 'Foundational Infrastructure' (Protocol in workflow.md)

## Phase 2: Rendering Engine
- [ ] Task: Initialize Direct2D and DirectWrite factories in `render.rs`.
- [ ] Task: Implement the basic vertical stack rendering logic with Segoe UI typography.
- [ ] Task: Conductor - User Manual Verification 'Rendering Engine' (Protocol in workflow.md)

## Phase 3: State & Input Integration
- [ ] Task: Integrate `state.rs` with the message loop to trigger redraws on state changes.
- [ ] Task: Implement basic mouse hit-testing for slider placeholders.
- [ ] Task: Conductor - User Manual Verification 'State & Input Integration' (Protocol in workflow.md)
