# Context Interaction API Contract

This document outlines the `contextInteractionService`, which serves as the clean Context Interaction API layer between the Context Engine (backend) and all future UI surfaces (Main UI, Launcher, Notch, Windows Edge, etc.).

By centralizing these calls, UI surfaces will never directly manipulate database models or duplicate context reasoning logic.

## Core API Contract

The API is exposed via `src/services/contextInteractionService.ts` and maps strictly to existing Tauri backend commands.

### `getActiveContext()`
- **Purpose:** Retrieves the currently active context from the engine.
- **Returns:** Promise resolving to the active context object.

### `getRecentContexts(limit?: number)`
- **Purpose:** Retrieves the user's recent work sequence to populate quick-switch menus (e.g., in the Launcher).
- **Returns:** Promise resolving to a list of recently active contexts.

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
- **Returns:** Promise resolving when complete.

### `pauseContext(contextId: string)`
- **Purpose:** Signals the engine to transition the given context to a paused state.
- **Returns:** Promise resolving when complete.

### `captureItem(item: CaptureRequest)`
- **Purpose:** Captures a new raw item (clipboard, text, file) and passes it through the CapturePipeline.
- **Returns:** Promise resolving with the capture result.

### `assignItemToContext(itemId: string, contextId: string)`
- **Purpose:** Explicitly assigns a captured item to a context (e.g., when a user drops an item onto a specific workspace in the Notch).
- **Returns:** Promise resolving when assigned.

### `searchContexts(query: string)`
- **Purpose:** Performs a universal semantic and text search across the context engine.
- **Returns:** Promise resolving to a list of matching items/contexts.

### `getContextSuggestions(itemId: string)`
- **Purpose:** Retrieves a list of contexts that the Context Engine suggests the item might belong to.
- **Returns:** Promise resolving to a list of suggested memberships.

## Implementation Guidelines
1. **Stateless UI:** UI surfaces (Notch, Launcher) must be stateless consumers of this API.
2. **No Data Duplication:** UI surfaces must not cache context worksets or attempt to manipulate assignments directly in SQLite.
3. **Event-Driven Updates:** Where real-time updates are needed, UIs should combine these APIs with global Tauri event listeners (e.g., `target-state-changed`, `notch-context-insight`).
