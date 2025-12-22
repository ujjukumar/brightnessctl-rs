# Implementation Plan: Monitor Identity and Advanced Control (spec_20251222)

## Phase 1: Robust Monitor Identification & EDID Logic [checkpoint: bf76556]
Goals: Implement Win32 EDID retrieval and the composite key generation logic.

- [x] Task: Win32 - Implement `get_edid_blob` using Registry/SetupAPI
- [x] Task: Logic - Implement `MonitorIdentity` struct with composite key generation (MFG+Code+Serial or Hash)
- [x] Task: TDD - Unit tests for composite key stability, fallback logic, and EDID hash determinism (across simulated reboots/reconnects)
- [ ] Task: Conductor - User Manual Verification 'Phase 1: Monitor Identification' (Protocol in workflow.md)

## Phase 2: Enhanced Persistence & State Management [checkpoint: 8546d71]
Goals: Update `state.rs` and `settings.rs` to support per-monitor metadata and persistence.

- [x] Task: State - Update `AppState` to track per-monitor metadata (normalized values, flags, timestamps)
- [~] Task: Settings - Implement `save_monitor_state` and `load_monitor_state` using the new composite keys
- [ ] Task: TDD - Unit tests for state persistence and retrieval with different monitor configurations
- [ ] Task: Conductor - User Manual Verification 'Phase 2: State Persistence' (Protocol in workflow.md)

## Phase 3: Defensive Hardware Control & Reliability
Goals: Implement rate-limiting, failure backoff, and smart startup logic.

- [~] Task: Brightness - Implement DDC write rate-limiting and failure backoff (disable control after N failures)
- [~] Task: Logic - Implement hardware limit query and clamp caching
- [~] Task: Logic - Implement "Smart Restore" startup logic (gate strictly on `last_set_by_app == true` AND `write_confirmed == true`)
- [ ] Task: TDD - Unit tests for rate-limiting logic, backoff escalation, and smart restore provenance checks
- [ ] Task: Conductor - User Manual Verification 'Phase 3: Hardware Reliability' (Protocol in workflow.md)

## Phase 4: Synchronization & UI Polish
Goals: Implement "Lock Mode" logic and update the UI with metadata and tooltips.

- [ ] Task: Logic - Implement normalized space synchronization for "Lock Mode"
- [ ] Task: UI - Add "Lock/Sync" toggle and update monitor list with connection/DDC labels
- [ ] Task: UI - Implement hover tooltips for detailed monitor information
- [ ] Task: TDD - Unit tests for normalized synchronization math, including mixed min/max ranges, early saturation, and reverse movement
- [ ] Task: Conductor - User Manual Verification 'Phase 4: Synchronization & UI' (Protocol in workflow.md)
