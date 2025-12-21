# Plan: Fix Compiler Warnings

## Phase 1: Remove Unused Fields [checkpoint: e6ed095]

- [x] Task: Remove `hmonitor` field from `Monitor` struct and update usages e6ed095
    - [x] Remove `hmonitor` from `src/state.rs`
    - [x] Update `src/monitors.rs` where `Monitor` is instantiated
    - [x] Verify if `hmonitor` was used in `src/brightness.rs` or other files (though the warning implies it wasn't read, it might be written to).
- [x] Task: Remove `hot_monitor` field from `AppState` struct and update usages e6ed095
    - [x] Remove `hot_monitor` from `src/state.rs`
    - [x] Update `AppState` initialization in `src/main.rs` (or wherever it occurs).
- [x] Task: Verify Fixes e6ed095
    - [x] Run `cargo check` to confirm zero warnings.
    - [x] Run `cargo build` to ensure successful compilation.
- [x] Task: Conductor - User Manual Verification 'Phase 1: Remove Unused Fields' (Protocol in workflow.md) e6ed095
