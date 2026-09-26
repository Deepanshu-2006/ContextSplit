# Context Interaction API Contract

This document outlines the `contextInteractionService`, which serves as the clean Context Interaction API layer between the Context Engine (backend) and all future UI surfaces (Main UI, Launcher, Notch, Windows Edge, etc.).

By centralizing these calls, UI surfaces will never directly manipulate database models or duplicate context reasoning logic.

## Core API Contract

The API is exposed via `src/services/contextInteractionService.ts` and maps strictly to existing Tauri backend commands.

### `getActiveContext()`
- **Purpose:** Retrieves the currently active context from the engine.
- **Returns:** Promise resolving to the active context object.

### `getRecentContexts(limitDays?: number)`
- **Purpose:** Retrieves the user's recent work sequence to populate quick-switch menus (e.g., in the Launcher).
- **Backend:** `get_recent_work_sequence` with a `limitDays` window (default 7).
- **Returns:** Promise resolving to the unwrapped `entries` list of the engine's `RecentWorkSequenceResponse`.

### `getContextSummary(contextId: string)`
- **Purpose:** Provides a high-level overview of a context, including basic metadata and aggregated stats.
- **Returns:** Promise resolving to the context overview.

### `getContextWorkset(contextId: string)`
- **Purpose:** Retrieves the current working set (active memories and captures) for a context, allowing a UI to reconstruct the environment.
- **Returns:** Promise resolving to a `ContextWorkset`.

### `getContextState(contextId: string)`
- **Purpose:** Retrieves the freshness and state of a context (e.g., Active, Paused, Dormant).
- **Returns:** Promise resolving to a `ContextState`.

### `getResumeReadiness(contextId: string)`
- **Purpose:** Evaluates whether a context is ready to be resumed, providing a `ResumeReadinessAssessment`. Useful for "One-Click Work Mode".
- **Returns:** Promise resolving to a `ContextResumeState`.

### `resumeContext(contextId: string)`
- **Purpose:** Signals the engine to transition the given context to an active state.
- **Backend:** `create_context_signal` with `signal_type: "user_action"` and payload `{ "action": "resume", "source": "InteractionAPI" }` — lifecycle interpretation belongs to the engine, not the UI layer.
- **Returns:** Promise resolving to the created `ContextSignal`.

### `pauseContext(contextId: string)`
- **Purpose:** Signals the engine to transition the given context to a paused state.
- **Backend:** `create_context_signal` with `signal_type: "user_action"` and payload `{ "action": "pause", "source": "InteractionAPI" }`.
- **Returns:** Promise resolving to the created `ContextSignal`.

### `captureItem(item: CaptureRequest)`
- **Purpose:** Captures a new raw item (clipboard, text, file) and passes it through the CapturePipeline.
- **Backend:** `handle_capture` (routed through `CaptureCore`).
- **Returns:** Promise resolving when the engine accepts the capture.
- **⚠ Phase 1 gap:** the backend command returns `Result<(), String>` — no item ID is handed back to the caller. Surfaces that need the captured item's ID (e.g. to resolve candidate contexts) must resolve it via `searchContexts` until the backend returns a `CaptureResult`.

### `assignItemToContext(itemId: string, contextId: string)`
- **Purpose:** Explicitly assigns a captured item to a context (e.g., when a user drops an item onto a specific workspace in the Notch).
- **Returns:** Promise resolving when assigned.

### `searchContexts(query: string)`
- **Purpose:** Performs a universal semantic and text search across the context engine.
- **Backend:** `search_inbox_items` with an `InboxSearchParams` envelope (`{ params: { query, enableSemantic: true } }`).
- **Returns:** Promise resolving to a list of matching items/contexts.

### `getContextSuggestions(itemId: string)`
- **Purpose:** Retrieves a list of contexts that the Context Engine suggests the item might belong to.
- **Returns:** Promise resolving to a list of suggested memberships.

## Implementation Guidelines
1. **Stateless UI:** UI surfaces (Notch, Launcher) must be stateless consumers of this API.
2. **No Data Duplication:** UI surfaces must not cache context worksets or attempt to manipulate assignments directly in SQLite.
3. **Event-Driven Updates:** Where real-time updates are needed, UIs should combine these APIs with global Tauri event listeners (e.g., `target-state-changed`, `notch-context-insight`).

## Contract Validation
The contract is enforced by `src/services/contextInteractionContract.test.ts`, which verifies:
- All 12 APIs exist and map to existing Context Engine commands with correct envelopes.
- The 8 standardized interaction events (`context.open`, `context.resume`, `context.pause`, `context.capture`, `context.assign`, `context.search`, `context.preview`, `context.mark_important`) are constructible from any platform surface.
- The 6 standardized result states (`SUCCESS`, `PENDING`, `AMBIGUOUS`, `REJECTED`, `ERROR`, `CANCELLED`) are representable.
- **No separate context logic:** the interaction layer performs exactly one `invoke` per API, holds no local state or persistence, and calls only registered engine commands.
