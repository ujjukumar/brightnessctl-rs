# Plan: Fix Compiler Warnings

## Phase 1: Remove Unused Fields

- [x] Task: Remove `hmonitor` field from `Monitor` struct and update usages
    - [ ] Remove `hmonitor` from `src/state.rs`
    - [ ] Update `src/monitors.rs` where `Monitor` is instantiated
    - [ ] Verify if `hmonitor` was used in `src/brightness.rs` or other files (though the warning implies it wasn't read, it might be written to).
- [x] Task: Remove `hot_monitor` field from `AppState` struct and update usages
    - [ ] Remove `hot_monitor` from `src/state.rs`
    - [ ] Update `AppState` initialization in `src/main.rs` (or wherever it occurs).
- [x] Task: Verify Fixes
    - [ ] Run `cargo check` to confirm zero warnings.
    - [ ] Run `cargo build` to ensure successful compilation.
- [x] Task: Conductor - User Manual Verification 'Phase 1: Remove Unused Fields' (Protocol in workflow.md)
