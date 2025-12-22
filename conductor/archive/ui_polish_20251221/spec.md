# Specification: UI Polish & Feature Expansion

## Overview
This track aims to elevate the `brightnessctl` application from a raw prototype to a professional-grade utility. The focus is on implementing a polished, minimalist user interface using Direct2D, while adding essential application infrastructure: a standard menu bar, a robust settings system (with theme support), and responsive UI behaviors. All changes must respect the strict constraints of binary size (<1MB) and runtime efficiency.

## Functional Requirements

### 1. User Interface Overhaul
- **Style:** "Polished Minimalist". Clean lines, high contrast, Segoe UI typography.
- **Rendering:**
    - Custom-drawn sliders with distinct "Normal", "Hover", and "Active/Dragging" visual states.
    - Subtle smooth transitions (e.g., color fades on hover) if feasible without bloat.
    - **Responsive Layout:** Content must resize gracefully when the window dimensions change.
- **Status Bar:** A dedicated area at the bottom of the window to display transient status messages (e.g., "Saved", "Monitor 1 Active").

### 2. Application Menus
- **Implementation:** Native Win32 Menu Bar (using `CreateMenu`/`InsertMenuItem`).
- **Structure:**
    - **File:** `Exit`
    - **View:** `Refresh Monitors`
    - **Theme:** `Auto` (Default), `Light`, `Dark`
    - **Help:** `About`

### 3. Settings & Persistence
- **Storage:** Portable JSON file (`brightnessctl.json`) stored alongside the executable.
- **Persisted Data:**
    - Selected Theme preference (Auto/Light/Dark).
    - Window position/size (optional, but good for polish).
    - Last used brightness values (optional, to restore state).
- **Behavior:** Settings load on startup and save on change/exit.

### 4. Theming Engine
- **Modes:**
    - **Light:** Standard Windows light colors.
    - **Dark:** Standard Windows dark colors.
    - **Auto:** Detects system preference via `registry` or `SystemParametersInfo`.
- **Live Update:** Changing the theme in the menu updates the UI immediately without restarting.

## Non-Functional Requirements
- **Performance:** No noticeable lag during resizing or slider dragging.
- **Binary Size:** Maintain release size < 1MB.
- **CPU Usage:** 0% when idle (rendering only on `WM_PAINT` or input).

## Acceptance Criteria
- [ ] Application has a native Menu Bar with functional items.
- [ ] UI sliders react to hover and click with distinct visual changes.
- [ ] Resizing the window correctly adjusts the layout of sliders.
- [ ] Changing the theme (Light/Dark/Auto) instantly updates colors.
- [ ] Settings (Theme preference) persist across application restarts via a local JSON file.
- [ ] Status bar displays relevant information.

## Out of Scope
- System Tray integration (saved for a future track).
- Complex animations (e.g., spring physics).
- Master brightness slider (synchronizing all monitors).
