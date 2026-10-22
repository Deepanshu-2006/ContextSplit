# Day 35 — Context Workset Integration: Architectural Audit

**Milestone:** Phase 2 — Context Launcher (Day 35: Context Workset Integration)  
**Date:** October 22, 2026  
**Document:** [`docs/day-35-workset-audit.md`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/docs/day-35-workset-audit.md)  
**Status:** **AUDITED & VERIFIED (READ-ONLY)**  

---

## Executive Summary

On Day 35 of ContextSplit Phase 2, an architectural audit was conducted to investigate how a selected context connects to its relevant items, activities, and tasks, and to determine how the Context Launcher can seamlessly integrate with the Context Workset layer.

Following the achievements of Day 33 (Quick Actions system) and Day 34 (Context Organization, Pinning, Renaming, and Disambiguation), Day 35 focuses on **Context Workset Integration**: transforming the launcher preview from a passive summary into an active, actionable working set that grounds the user's focus when inspecting or resuming a context.

### Key Audit Findings

1. **Robust Backend Engine Already Exists:** The Phase 1 Rust backend possesses a fully implemented, mathematically calibrated `ContextWorksetService` and `ContextWorksetBuilder` ([`src-tauri/src/context_engine/workset.rs`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/context_engine/workset.rs)) with **46/46 unit and integration tests passing**. It scores items across 10 distinct signals, applies temporal decay, hysteresis inertia, deduplication, and noise suppression, and categorizes items into 5 distinct relevance tiers (`Primary`, `Supporting`, `Reference`, `Recent`, `Related`).
2. **Indexed & Bounded Queries:** Database schema migrations 1 through 26 ([`src-tauri/src/db/mod.rs`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/db/mod.rs)) provide dedicated composite indexes on `(workspace_id, created_at DESC)`, `(workspace_id, membership_type, confidence DESC)`, and `item_relationships(item_a_id/item_b_id)`. Workset queries are strictly bounded (`LIMIT 50`, `LIMIT 30`, `LIMIT 100`), ensuring zero unbounded full-table scans.
3. **Frontend Integration Gap (Actionability):** In the Context Launcher preview pane ([`src/components/launcher/ContextLauncher.tsx`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx)), workset items are currently rendered as static display rows without click handlers (`onClick` is absent on the row itself), without keyboard traversal into items, and without visual tier grouping (all items are presented in a flat list capped at 6 items).
4. **Canvas / Workspace Desynchronization:** Resuming a context via `selectWorkspace(id)` in [`src/store/workspaceStore.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/store/workspaceStore.ts) performs a raw query `db.listItems(id)` (`SELECT * FROM items WHERE workspace_id = ?`). It completely bypasses the `ContextWorksetService`, discarding relevance scoring, task alignment, and suggested inbox additions.

---

## 1. Architectural Subsystem Inspection

### 1.1 Context Launcher & Context Selection Flow

- **Selection Trigger:** When a user navigates contexts in the launcher list via arrow keys, mouse clicks, or filter tabs, `selectContextById(contextId)` is dispatched in [`src/store/launcherStore.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/store/launcherStore.ts#L690-L860).
- **Asynchronous Hydration:** Debounced by 40ms, `selectContextById` issues three concurrent backend queries via `Promise.allSettled`:
  1. `contextInteractionService.getContextState(contextId)`: Returns operational lifecycle state, freshness display, and last meaningful activity.
  2. `contextInteractionService.getContextWorkset(contextId)`: Returns `ContextWorkset` with ranked `items: WorksetItemReference[]`, `task_title`, `task_id`, `confidence`, and `recent_activity`.
  3. `contextInteractionService.getResumeReadiness(contextId)`: Evaluates whether the context is ready to continue work (`ready`, `mostly_ready`, `partial`, `insufficient`).
- **Preview Model Mapping:** Maps the top 6 items (`worksetData.items.slice(0, 6)`) into `LauncherPreviewItem[]`. Items with `relevance === 'primary'` are flagged as `isImportant`.
- **Presentation in UI:** [`src/components/launcher/ContextLauncher.tsx`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1425-L1450) renders the preview pane containing context header, status badges, active task banner, "Working Set" section with `itemCount` and `worksetItems`, primary action button, and secondary action popover menu (`⌥M`).

### 1.2 Context & Item Models

- **Context / Workspace Entity:**
  - SQLite Table: `workspaces` ([`src-tauri/src/db/mod.rs:41-48`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/db/mod.rs#L41-L48), Migrations 14 & 26).
  - Columns: `id TEXT PRIMARY KEY`, `name TEXT NOT NULL`, `created_at TEXT NOT NULL`, `updated_at TEXT NOT NULL`, `lifecycle_state TEXT NOT NULL DEFAULT 'active'`, `last_activity_at TEXT`, `archived_at TEXT`, `is_pinned INTEGER NOT NULL DEFAULT 0`.
  - Rust Struct: `Workspace` ([`src-tauri/src/db/models.rs:3-14`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/db/models.rs#L3-L14)).
  - Client Interface: `Workspace` in [`src/services/db.ts:1-12`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/services/db.ts#L1-L12).
- **Item Entity:**
  - SQLite Table: `items` ([`src-tauri/src/db/mod.rs:54-66`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/db/mod.rs#L54-L66), Migration 2).
  - Columns: `id TEXT PRIMARY KEY`, `workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE`, `session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL`, `item_type TEXT NOT NULL`, `content TEXT NOT NULL`, `title TEXT`, `metadata TEXT NOT NULL DEFAULT '{}'`, `created_at TEXT NOT NULL`, `updated_at TEXT NOT NULL`.
  - Supported Item Types: `"text"`, `"url"`, `"file"`, `"image"`, `"clipboard"`.
  - Item Metadata Payload: JSON string storing `app_name`, `domain`, `window_title`, `original_url`, `file_path`, `pos_x`, `pos_y`.

### 1.3 ContextMembership & Contextual Relationships

- **Context Memberships Table (`context_memberships`):**
  - Migration 12 & 24–25 ([`src-tauri/src/db/mod.rs:246-288`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/db/mod.rs#L246-L288)).
  - Schema: `(id TEXT PRIMARY KEY, item_id TEXT NOT NULL, workspace_id TEXT NOT NULL, membership_type TEXT NOT NULL, confidence REAL NOT NULL, source TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, UNIQUE(item_id, workspace_id))`.
  - Membership Types:
    - `"confirmed"`: Definitive assignment (`confidence = 1.0`, `source = "user"` or `"confirmed"`).
    - `"suggested"`: Statistical/semantic inference (`confidence >= 0.60`, `source = "context_engine"` or `"context_discovery"`).
    - `"dismissed"`: Explicit user rejection (`source = "user"`).
  - Multi-Context Association: The schema enforces `UNIQUE(item_id, workspace_id)`. An item **CAN** have multiple records across distinct workspaces in `context_memberships`.
- **Item Relationships Graph (`item_relationships`):**
  - Migration 5 ([`src-tauri/src/db/mod.rs:114-128`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/db/mod.rs#L114-L128)).
  - Schema: `(id TEXT PRIMARY KEY, item_a_id TEXT NOT NULL, item_b_id TEXT NOT NULL, relationship_type TEXT NOT NULL, strength REAL NOT NULL, metadata TEXT, created_at TEXT NOT NULL, UNIQUE(item_a_id, item_b_id, relationship_type))`.
  - Edge Types: `"references"`, `"documents"`, `"executes"`, `"semantic_similarity"`, `"temporal_co_occurrence"`, `"shared_session"`, `"domain_co_occurrence"`.
- **Context Candidates & Candidate Items:**
  - `context_candidates` and `context_candidate_items` (Migration 11) track emergent clusters discovered from unassigned Inbox items before any workspace is confirmed.

### 1.4 Context Engine Scoring, Confidence, and Relevance Logic

Derived deterministically in [`src-tauri/src/context_engine/workset.rs:1154-1540`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/context_engine/workset.rs#L1154-L1540):
- **10 Multi-Factor Signals:**
  1. `s_task` (up to +0.40): Task connection (exact task item +0.40, subject match +0.35, relationship to task items $\ge 0.70$ +0.25, term overlap +0.15).
  2. `s_context` (+0.25 confirmed base, +0.15 for repository or documentation URLs).
  3. `s_recency` (+0.25 $\times$ temporal weight via exponential half-life decay).
  4. `s_membership` (+0.15 confirmed priority bonus).
  5. `s_sem` (+0.15 $\times$ max semantic similarity edge).
  6. `s_rel` (+0.15 $\times$ average relationship edge strength).
  7. `s_repeat` (+0.20 for items referenced across $\ge 2$ segments or multi-session).
  8. `s_active` (+0.15 for items in the most recent activity segment).
  9. `s_feedback` (+0.25 for explicit user confirmation / keep_important).
  10. `s_usefulness` (+0.05 non-empty, -0.20 empty/ephemeral).
- **Stability Hysteresis & Anti-Jitter:**
  - Items in the previous workset retain an inertia bonus (`+0.12` base, `+0.05` extra for previous primary items; reduced to `+0.05` on task transitions).
  - Weak new evidence (< 0.50 composite score without repeated observations) is dampened by factor `0.85`.
- **Relevance Tiers:**
  - `Primary`: Direct active task documents, components, or files being worked on.
  - `Supporting`: Specifications, mocks, libraries directly referencing or referenced by the active task.
  - `Reference`: Repositories, documentation links, permanent workspace assets.
  - `Recent`: Items captured or interacted with in the latest active burst without specific task binding.
  - `Related`: High-confidence suggested additions from the Inbox or graph-connected neighbors.
- **Quality Filter & Noise Suppression (`ContextWorksetQualityFilter`):**
  - Deduplication: Collapses duplicate URL parameters, app+title matches, or snippet hashes.
  - Generic noise suppression: Excludes generic titles ("untitled", "new tab", "search...") unless grounded in the active task.
  - Accidental capture filter: Suppresses numeric OTP/2FA codes and empty clipboard snippets.
  - Exclusion veto: Items in `dismissed_memberships` or with user rejection signals are strictly excluded.

### 1.5 Context Resume & Quick Action Flows

- **Context Resume Execution:**
  - Implemented in [`src/services/quickActionsService.ts:127-185`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/services/quickActionsService.ts#L127-L185).
  - If context is archived, automatically calls `db.unarchiveWorkspace(cleanContextId)`.
  - Intercepts insufficient readiness unless explicitly forced by user.
  - Emits `user_action` resume signal to `context_signals` via `contextInteractionService.resumeContext(cleanContextId)`.
  - Dispatches `useWorkspaceStore.getState().selectWorkspace(cleanContextId)`.
- **Item Quick Actions:**
  - `item.mark_important` / `item.unmark_important`: Calls `contextWorksetService.controlContextMemory(contextId, itemId, 'keep_important' | 'remove_important')`. Delegates to backend `submit_context_memory_control`.
  - `item.remove_from_context`: Calls `contextWorksetService.controlContextMemory(contextId, itemId, 'remove_from_context')`. Suppresses item from context workset without physical deletion.
  - `item.delete`: Explicitly disabled (`isSupported: false`) in launcher to prevent accidental data loss.

### 1.6 Tasks, Sessions, Timelines & Activity Records

- **Tasks:** Extracted verb-subject pairs stored in SQLite table `task_candidates` (Migration 21) with columns `(id, context_id, title, confidence, status, first_detected_at, last_active_at)`.
- **Sessions:** Tracked in SQLite table `sessions` (Migration 4) with `(id, started_at, last_activity_at, ended_at, status)`. Orphaned sessions reconciled on startup.
- **Activity Segments:** Computed dynamically by `ContextActivitySegmenter` ([`src-tauri/src/context_engine/segments.rs`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/context_engine/segments.rs)) grouping item references, session IDs, and active apps.
- **Context Transitions:** Persisted in SQLite table `context_transitions` (Migration 20) with `(id, from_context_id, to_context_id, transition_type, confidence, detected_at, session_id, trigger_item_id)`.
- **Context Snapshots:** Persisted in SQLite table `context_snapshots` (Migration 23) capturing periodic serialized representations of context state.

---

## 2. Capabilities Classification

| Capability | Status | Evidence & Actual Code Location |
|---|---|---|
| **Context Workset Derivation Engine** | `IMPLEMENTED` | [`src-tauri/src/context_engine/workset.rs:668-1540`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/context_engine/workset.rs#L668-L1540). 10-signal scoring, stability hysteresis, quality gate. 46/46 Rust tests pass. |
| **Workset Quality Gate & Noise Suppression** | `IMPLEMENTED` | [`src-tauri/src/context_engine/workset.rs:1544-1680`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/context_engine/workset.rs#L1544-L1680). Filters ephemeral 2FA/OTP, generic search tabs, and duplicate captures. |
| **In-Memory Workset Caching** | `IMPLEMENTED` | [`src-tauri/src/context_engine/workset.rs:565-666`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/context_engine/workset.rs#L565-L666). `ContextWorksetCache` with 30s TTL and selective invalidation. |
| **Workset Database Schema & Indexes** | `IMPLEMENTED` | [`src-tauri/src/db/mod.rs:514-579`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/db/mod.rs#L514-L579). Migrations 24–26 provide composite indexes on workspace items and memberships. |
| **Tauri Workset IPC Commands** | `IMPLEMENTED` | [`src-tauri/src/lib.rs:1147-1193`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/lib.rs#L1147-L1193). `get_context_workset`, `get_active_workset`, `submit_workset_feedback`, `submit_context_memory_control`. |
| **Frontend Workset Client Service** | `IMPLEMENTED` | [`src/services/contextWorkset.ts:1-186`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/services/contextWorkset.ts#L1-L186). Typed DTOs, Tauri invocations, relevance partition helpers. |
| **Launcher Preview Workset Hydration** | `IMPLEMENTED` | [`src/store/launcherStore.ts:714-758`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/store/launcherStore.ts#L714-L758). Fetches workset concurrently on context selection and maps top 6 items. |
| **Launcher Workset Item Actions (Star / Remove)** | `IMPLEMENTED` | [`src/services/quickActionsService.ts:464-628`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/services/quickActionsService.ts#L464-L628). `item.mark_important`, `item.unmark_important`, `item.remove_from_context`. |
| **Context Resume with Readiness Guard** | `IMPLEMENTED` | [`src/services/quickActionsService.ts:127-185`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/services/quickActionsService.ts#L127-L185). Intercepts low readiness, supports force override, restores archived workspaces. |
| **Launcher Workset Item Interactivity (Click to Open / Copy)** | `NOT_IMPLEMENTED` | [`src/components/launcher/ContextLauncher.tsx:1784-1860`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1784-L1860). `WorksetPreviewRow` has no `onClick` on the item; clicking does not open URLs, copy text, or view files. |
| **Workset Relevance Tier Visual Grouping in Preview** | `NOT_IMPLEMENTED` | [`src/components/launcher/ContextLauncher.tsx:1435-1449`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1435-L1449). Renders a flat slice of 6 items without categorizing into Primary vs Reference vs Suggested. |
| **Keyboard Traversal into Workset Items** | `NOT_IMPLEMENTED` | [`src/components/launcher/ContextLauncher.tsx:320-455`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L320-L455). Keyboard navigation is confined to context list; no keybinding steps focus into workset items. |
| **Suggested Inbox Additions Acceptance Flow in Launcher** | `NOT_IMPLEMENTED` | Suggested items appear with default styling; no quick "Accept into Context" button in launcher preview. |
| **Canvas / Workspace Workset Hydration on Resume** | `PARTIALLY_IMPLEMENTED` | [`src/store/workspaceStore.ts:127-143`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/store/workspaceStore.ts#L127-L143). Resuming calls `db.listItems` (raw SQLite query) rather than presenting prioritized workset items. |
| **Full Regression Test Consistency (Day 33/34 Drift)** | `BROKEN` | Vitest run shows 10 test files failing assertion expectations due to Day 34 additions (`isPinned`, `context.rename`, `context.pin` modifying expected array lengths). |

---

## 3. Detailed Answers to Audit Questions

### 1. How are items currently associated with contexts?

Items in ContextSplit connect to contexts through three distinct mechanisms across the database and engine:

1. **Direct Physical Workspace Assignment (`items.workspace_id`):**
   - Stored in SQLite column `items.workspace_id: Option<String>` ([`src-tauri/src/db/models.rs:17`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs#L17)).
   - `workspace_id = NULL` or `'inbox'` designates an item residing in the global, unorganized Inbox.
   - `workspace_id = Some(uuid)` represents explicit, confirmed assignment to that specific workspace.
2. **Context Membership Join Table (`context_memberships`):**
   - Stored in table `context_memberships` ([`src-tauri/src/db/models.rs:240-251`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs#L240-L251)) with columns `(id, item_id, workspace_id, membership_type, confidence, source)`.
   - Associates items with workspaces with explicit epistemic certainty (`confirmed`, `suggested`, `dismissed`), numerical confidence, and provenance (`user`, `context_engine`, `context_discovery`).
3. **Context Candidate Clusters (`context_candidates` & `context_candidate_items`):**
   - Unconfirmed groups of unassigned inbox items discovered algorithmically by semantic, temporal, and relational co-occurrence.

In `ContextWorksetBuilder::from_db` ([`src-tauri/src/context_engine/workset.rs:799-860`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/workset.rs#L799-L860)), candidate items for a context workset are gathered by combining:
- Physical items (`items.workspace_id = ?1`)
- Confirmed memberships (`context_memberships.workspace_id = ?1 AND membership_type = 'confirmed'`)
- High-confidence suggested inbox items (`context_memberships.workspace_id = ?1 AND membership_type = 'suggested' AND confidence >= 0.60`)

---

### 2. Can an item belong to multiple contexts?

**Yes contextually, but No physically:**
- **Physical Placement Layer:** An item can have at most **one** physical location (`items.workspace_id`). It is either unassigned in the Inbox or assigned to exactly one workspace.
- **Contextual Membership Layer (`context_memberships`):** An item **can belong to multiple contexts simultaneously**. The table constraint is `UNIQUE(item_id, workspace_id)`, not `UNIQUE(item_id)`. An item may be a confirmed member in Project A while being a high-confidence suggested reference in Project B.
- **Relational Graph Layer (`item_relationships`):** Items maintain graph edges across workspace boundaries (`semantic_similarity`, `references`, `temporal_co_occurrence`).
- **Isolation Protection:** The `ContextWorksetQualityFilter` prevents cross-context leakage by requiring direct task connection, high relationship strength ($\ge 0.40$), or explicit membership before an item enters a workset.

---

### 3. How are physical workspace placement and contextual association represented?

- **Physical Workspace Placement:** Represented by `items.workspace_id: Option<String>`. Moving an item physically (e.g. via `workspaceStore.moveItemToWorkspace`) executes an `UPDATE items SET workspace_id = ? WHERE id = ?`. This represents organizational storage.
- **Contextual Association:** Represented epistemically by `context_memberships`. An item can be physically located in the Inbox (`items.workspace_id IS NULL`) while simultaneously having a contextual association (`membership_type = 'suggested'`) to a workspace.
- **Non-Destructive Workset Actions:** When a user removes an item from a context workset via `item.remove_from_context`, ContextSplit **never deletes the item**. Instead, it emits a `user_feedback` signal and updates `context_memberships.membership_type = 'dismissed'`. The underlying item remains intact in the database and discoverable elsewhere.

---

### 4. Which relationships represent actual evidence, and which represent user-confirmed associations?

#### Actual Evidence (Observed System & Algorithmic Telemetry):
1. `context_signals`: Unfiltered background activity logs capturing active window titles, application names, browser URLs, session IDs, and timestamps.
2. `item_relationships`: Computed graph edges (`references`, `documents`, `semantic_similarity`, `temporal_co_occurrence`, `shared_session`) with quantitative `strength: f64`.
3. `context_transitions`: Detected context switches (`ContextStart`, `ContextSwitch`, `ContextReturn`, `ContextEnd`) identified by activity boundary analysis.
4. `context_activity_segments`: Continuous temporal clusters of work involving specific items and applications.
5. `task_candidates`: Extracted verb-subject task phrases identified by lexical and regex patterns from active window titles and captures.
6. `context_memberships` (`membership_type = 'suggested'`): Inferred candidate associations generated by the Context Engine.

#### User-Confirmed Associations (Authoritative Human Ground Truth):
1. `items.workspace_id`: Explicit manual assignment of an item to a workspace.
2. `context_memberships` (`membership_type = 'confirmed'`, `source = 'user'`): User acceptance of an item association (confidence = 1.0).
3. `context_memberships` (`membership_type = 'dismissed'`): Explicit user dismissal.
4. `context_signals` (`signal_type IN ('user_action', 'user_feedback')`): User operations including `resume`, `pause`, `keep_important`, `remove_from_context`.
5. `workspaces.name`, `workspaces.is_pinned`, `workspaces.lifecycle_state`: User-confirmed workspace identity, navigational pinning, and archival state.

---

### 5. Can the application retrieve relevant items for a selected context without scanning unrelated data unnecessarily?

**Yes, completely.** The database queries are targeted, bounded, and indexed:
1. `items WHERE workspace_id = ?1 ORDER BY created_at DESC LIMIT 50`: Backed by index `idx_items_workspace_created (workspace_id, created_at DESC)`.
2. `context_memberships WHERE workspace_id = ?1 AND membership_type = 'confirmed' ... LIMIT 50`: Backed by index `idx_context_memberships_ws_type_conf (workspace_id, membership_type, confidence DESC)`.
3. `context_memberships WHERE workspace_id = ?1 AND membership_type = 'suggested' AND confidence >= 0.60 ... LIMIT 30`: Backed by index `idx_context_memberships_ws_type_conf`.
4. `item_relationships WHERE item_a_id IN (...) OR item_b_id IN (...) ... LIMIT 100`: Backed by indexes `idx_item_relationships_item_a` and `idx_item_relationships_item_b`.
5. `context_signals WHERE workspace_id = ?1 ... LIMIT 50`: Backed by index `idx_context_signals_workspace_created`.
6. **In-Memory Cache:** `ContextWorksetCache` ([`src-tauri/src/context_engine/workset.rs:565-666`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/workset.rs#L565-L666)) caches derived worksets with a 30-second TTL, serving repeated preview requests in < 1ms without querying SQLite.

---

### 6. Which existing APIs can safely provide the data needed to resume work?

| Operation / Surface | Authoritative Existing API | Safety & Side-Effect Characteristics |
|---|---|---|
| **Retrieve Working Set** | `contextInteractionService.getContextWorkset(contextId)` / `invoke('get_context_workset')` | Read-only. Reconstructs ranked items, relevance tiers, active task, and confidence without mutating DB state. |
| **Evaluate Readiness** | `contextInteractionService.getResumeReadiness(contextId)` / `invoke('get_resume_state')` | Read-only. Returns readiness band (`ready`, `mostly_ready`, `partial`, `insufficient`) and human-readable explanation. |
| **Check Operational State** | `contextInteractionService.getContextState(contextId)` / `invoke('get_context_state')` | Read-only. Returns lifecycle state, freshness category, and last meaningful activity. |
| **Signal Context Resume** | `contextInteractionService.resumeContext(contextId)` / `invoke('create_context_signal')` | Non-destructive. Emits standardized `user_action` signal (`action: 'resume'`) into `context_signals`. |
| **Unarchive if Inactive** | `db.unarchiveWorkspace(contextId)` / `invoke('unarchive_workspace')` | Mutates only `workspaces.lifecycle_state = 'active'`. Required before resuming archived contexts. |
| **Control Item Memory** | `contextWorksetService.controlContextMemory(contextId, itemId, action)` / `invoke('submit_context_memory_control')` | Emits `user_feedback` signal; never deletes underlying item records. |

---

### 7. What information is missing to create a useful workset?

While the backend engine produces complete `ContextWorkset` data, several UX and integration elements are missing to make the workset truly useful for resuming work:

1. **Item Launching & Actionability:** In `WorksetPreviewRow`, clicking an item does nothing. Users cannot open URLs in a browser, copy text snippets to clipboard, or reveal files.
2. **Relevance Tier Visual Distinctions:** The launcher preview renders items in a single flat list. Users cannot visually differentiate between a **Primary working document** (what they were editing) and a **Reference link** (background docs) or an **Inbox suggestion**.
3. **Keyboard Traversal into Workset:** Keyboard users can navigate the context list on the left, but cannot press `Tab` or `ArrowRight` to focus and operate on the workset items.
4. **Accepting Suggested Additions:** Suggested inbox items have no one-click "Accept into Context" action in the preview pane.
5. **Post-Resume Canvas Hydration:** When a context is resumed, the main application view loads items via unranked `db.listItems`. It does not surface the active workset tiers or highlight the primary task items on the canvas.

---

### 8. What is the smallest useful workset that can be implemented using the current architecture?

The smallest, highest-impact useful workset that fits seamlessly into the current architecture consists of:

1. **Direct Item Launching in Launcher Preview:**
   - Clicking a workset item row opens URLs in the external default browser (via Tauri opener or window.open), copies text/note snippets to the clipboard with visual confirmation, or opens file attachments.
2. **Relevance Tier Grouping in Preview:**
   - Visually group preview items into 3 clean sections:
     - **Primary Focus:** The core active task items (e.g. document/file being edited).
     - **References:** Supporting links, repository URLs, and documentation.
     - **Suggested from Inbox (if any):** High-confidence suggested additions with a quick `+` (Accept) button.
3. **Keyboard Navigation into Preview Items:**
   - Pressing `ArrowRight` or `Tab` from the context list moves focus into the workset items list. Arrow keys traverse items; pressing `Enter` launches the item, and pressing `Space` or `Star` toggles memory importance.
4. **Workset Summary Badge on Resume:**
   - Resuming a context passes the active workset into `workspaceStore` so the canvas displays a compact "Resumed: [Task Title]" banner or focuses on the primary working items.

---

## 4. Implementation Plan for Day 35

### Phase 1: Interactive Workset Item Launching
- Add click handlers to [`WorksetPreviewRow`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1658):
  - URLs: Open in external default browser (`open(url)` or `window.open`).
  - Text / Notes: Copy snippet to clipboard with gentle temporary checkmark feedback.
  - Files / Paths: Open or reveal file if path is valid.
- Add accessible `tabIndex={0}` and keyboard `Enter` / `Space` activation on item rows.

### Phase 2: Tier Grouping & Suggested Addition Controls
- In `launcherStore.ts`, group `worksetItems` into:
  - `primaryItems`: Items with `relevance === 'primary'` or `relevance === 'supporting'`.
  - `referenceItems`: Items with `relevance === 'reference'` or `relevance === 'recent'`.
  - `suggestedItems`: Items with `is_suggested_addition === true`.
- Update `ContextLauncher.tsx` preview pane to render categorized subsections with clean headers: **Primary Working Set**, **References & Assets**, and **Suggested Additions**.
- Add an "Accept" (`+`) action for suggested additions that calls `contextInteractionService.assignItemToContext(itemId, contextId)` and refreshes the preview.

### Phase 3: Keyboard Traversal across List and Workset
- Implement roving focus from the context list to the preview pane via `ArrowRight` or `Tab`.
- Allow `ArrowDown` / `ArrowUp` within the preview items, `Enter` to open, `⌥P` or `S` to toggle importance, and `Escape` to step focus back to the context list.

### Phase 4: Alignment with Canvas & Test Hardening
- Reconcile test assertion expectations in `quickActionsUI.test.ts`, `quickActionsRegression.test.ts`, and `launcherStore.test.ts` that were affected by Day 34 pinning/renaming additions.
- Add dedicated regression tests for workset item launching, tier grouping, and keyboard interaction.
- Verify TypeScript (`npx tsc --noEmit`), Vite build (`npm run build`), and Rust test suite (`cargo test`).
