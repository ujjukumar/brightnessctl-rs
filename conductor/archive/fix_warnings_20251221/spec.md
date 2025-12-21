# Specification: Fix Compiler Warnings

## Overview
This track focuses on improving code quality and maintainability by resolving compiler warnings identified by `cargo check`. Specifically, it addresses "dead code" warnings related to unused struct fields.

## Functional Requirements
- Remove the `hmonitor` field from the `Monitor` struct in `src/state.rs`.
- Remove the `hot_monitor` field from the `AppState` struct in `src/state.rs`.
- Ensure all references to these fields in the codebase are removed or updated to maintain compilation.

## Non-Functional Requirements
- **Zero Warnings:** The project must compile cleanly without any warnings from `cargo check`.
- **No Regressions:** Removing these fields must not break existing functionality (e.g., monitor enumeration or brightness control).

## Acceptance Criteria
- `cargo check` returns 0 warnings.
- The application builds and runs as expected.
- Source code is cleaner and adheres to the "remove unused code" principle.

## Out of Scope
- Adding new features.
- Refactoring other parts of the codebase not related to these specific warnings.
