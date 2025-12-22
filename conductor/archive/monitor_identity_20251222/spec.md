# Specification: Monitor Identity and Advanced Control (spec_20251222)

## Overview
This track transforms the application from a generic brightness controller into a robust, context-aware utility. It introduces persistent monitor identity via robust EDID keys, enhanced per-monitor state persistence, defensive hardware control, and a "Sync" mode for multi-monitor harmony.

## Functional Requirements

### 1. Robust Monitor Identification
- **EDID Integration:** Use Win32 Registry/SetupAPI to retrieve EDID blobs.
- **Composite Key Logic:**
    1. Primary: Manufacturer ID + Product Code + Serial Number.
    2. Fallback: If Serial is missing/zero/duplicate -> Hash of full 128-byte EDID.
    3. Failure: If EDID unavailable -> Mark as "unidentified" (no persistence).

### 2. Enhanced Persistence Model
- **Storage:** Persist the following data per identified monitor:
    - `brightness_value` (u32): Raw hardware value.
    - `normalized_value` (f32): 0.0–1.0 representation.
    - `last_set_by_app` (bool): True if the last change originated from this app.
    - `write_confirmed` (bool): True if a readback confirmed the write.
    - `timestamp` (u64): Time of last update.
    - `last_seen_device_path` (string): Metadata for debugging/verification.

### 3. Advanced Hardware Control
- **Limit Awareness:** Query hardware min/max but treat them as advisory.
- **Clamp Caching:** Cache observed effective min/max values based on successful write/readback loops.
- **Reliability:**
    - **Rate Limiting:** Throttle DDC writes to prevent monitor firmware saturation.
    - **Backoff:** Implement a failure backoff strategy after repeated timeouts.
- **Startup Logic:** On startup, do NOT blindly overwrite. Only restore if `last_set_by_app == true` AND `write_confirmed == true`, otherwise adopt current hardware state.

### 4. Synchronization Features ("Lock Mode")
- **UI:** Toggle button to "Lock All Sliders".
- **State:** Lock state is non-persistent (resets on app restart).
- **Behavior:**
    - Operations occur in **normalized space (0.0 - 1.0)**.
    - **Independent Saturation:** If one monitor hits its limit (0.0 or 1.0), others continue moving until they reach their own limits.
    - **No Reverse Compensation:** Changing direction simply moves from the current position; no "catching up" logic.

### 5. UI/UX Enhancements
- **Metadata:** Display connection type (HDMI/DP/USB-C) and DDC status via subtle labels.
- **Tooltips:** Show detailed info (OS-derived connection class, full hardware ID, range) on hover.
- **Caveat:** Connection type is "best-effort" (acknowledged ambiguity with USB-C/MST). Functionality is never gated by this data.

## Non-Functional Requirements
- **Performance:** EDID parsing and hash generation cached at startup.
- **Zero-Dependency:** Use a lightweight crate for EDID parsing *internal logic only*, keeping IO native Win32.

## Acceptance Criteria
- [ ] Monitors are identified by a robust composite key (MFG+Code+Serial or EDID Hash).
- [ ] Monitor state is persisted with full metadata (timestamp, ownership flags).
- [ ] App respects external brightness changes (no overwrite unless app owns the state).
- [ ] "Lock" mode scales in normalized space and handles differential saturation correctly.
- [ ] DDC writes are rate-limited and robust against timeouts.
- [ ] Connection type displayed as best-effort info, never blocking control.
