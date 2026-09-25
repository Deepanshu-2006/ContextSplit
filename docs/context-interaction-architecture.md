# Context Interaction Architecture

## Vision
ContextSplit is transitioning from a standalone application into a cross-platform productivity system. The underlying Context Engine (built during Phase 1) serves as the **single source of truth** for all contextual logic, memory, and state. 

This document defines the architectural guidelines for exposing the Context Engine to multiple interaction surfaces (Phase 2 and beyond) without duplicating state or business logic.

## Supported Interaction Surfaces
1. **Main ContextSplit Application** - The primary management interface.
2. **Context Launcher** - Quick context switching and intent execution.
3. **macOS Notch** - The current drop-target and ambient awareness surface.
4. **Windows Edge/Floating UI** - The Windows equivalent of the notch.
5. **Contextual Action Interface** - In-context task execution.
6. **Universal Search** - Semantic retrieval across all contexts.
7. **Future Automation** - Headless interactions and triggers.

---

## Architectural Principles
1. **Single Source of Truth:** All surfaces must consume state directly from the Context Engine via the existing Tauri commands. No surface is permitted to maintain an independent cache of the working set, memory, or context state.
2. **Event-Driven Reactivity:** Surfaces should listen for global Tauri events emitted by the backend (e.g., `notch-context-insight`, `target-state-changed`) rather than relying on polling.
3. **Stateless UI:** The interaction surfaces are purely presentation layers. If an action occurs (like dragging a file to the notch), the UI merely forwards the payload to the backend and renders the resulting state.

---

## Exposed Context APIs

The interaction layer will build upon the following existing Tauri commands exposed by the Context Engine:

### 1. Context APIs & State APIs
Used by the Launcher and edge surfaces to understand what the user is currently doing.
- `get_last_active_context`
- `get_last_active_task`
- `get_context_state`
- `get_resume_state`
- `evaluate_resume_readiness`
- `get_context_overview`

### 2. Context Reconstruction & Workset APIs
Used to load the minimal set of relevant items required to resume a task across surfaces.
- `get_context_reconstruction`
- `reconstruct_context_cold`
- `get_context_workset`
- `get_active_workset`
- `get_recent_work_sequence`

### 3. Context Assignment APIs
Used by drop targets (Notch / Windows Edge UI) and the Launcher to route new captures.
- `create_context_membership`
- `confirm_context_membership`
- `dismiss_context_membership`
- `bulk_assign_inbox_items_to_workspace`

### 4. Task APIs
Used by the universal search and launcher to identify emerging contexts.
- `list_context_candidates`
- `get_context_candidate`
- `list_all_task_candidates`
- `create_workspace_from_candidate`

### 5. Memory APIs
Used by the Copilot and Feed surfaces to show temporal context history.
- `get_current_memory`
- `get_context_memory`
- `get_recent_context_memories`
- `get_today_memory`
- `get_yesterday_memory`
- `get_historical_memory_for_date`
- `get_context_important_memories`

### 6. Feedback & Lifecycle APIs
Used by interactive components allowing users to steer the engine's reasoning.
- `submit_workset_feedback`
- `submit_memory_feedback`
- `submit_context_memory_control`
- `explain_workspace_lifecycle`

### 7. Snapshot APIs
Used to diff state over time or rollback.
- `get_context_snapshot`
- `get_context_snapshot_history`
- `get_context_snapshot_diff`

---

## Next Steps
As we move into **Phase 2 (Context Launcher)** and **Phase 4 (Interactive Edge Layer)**, the primary task is wiring these existing Tauri commands into the specific surface boundaries. 

*No new database models or context derivations should be built for these UI layers.*
