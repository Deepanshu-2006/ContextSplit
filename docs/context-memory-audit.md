# Context Memory Architecture Audit (Day 17 — Step 1)

**Status:** Completed  
**Date:** September 2026  
**Target Subsystems:**
- `context_engine/` ([`memory.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory.rs), [`overview.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/overview.rs), [`task_evidence.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_evidence.rs), [`task_extractor.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_extractor.rs), [`task_ranking.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_ranking.rs), [`task_transitions.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_transitions.rs), [`transitions.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/transitions.rs), [`segments.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/segments.rs), [`summary.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/summary.rs), [`timeline.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/timeline.rs), [`engine.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/engine.rs), [`evidence.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/evidence.rs))
- `db/` ([`models.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs), [`operations.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/operations.rs), [`mod.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/mod.rs))
- Frontend (`src/`) ([`src/components/workspace/ContextOverviewPanel.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/workspace/ContextOverviewPanel.tsx), [`src/components/timeline/ContextTimelineDrawer.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/timeline/ContextTimelineDrawer.tsx), [`src/components/layout/TopBar.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/layout/TopBar.tsx), [`src/services/db.ts`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/services/db.ts), [`src/store/workspaceStore.ts`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/store/workspaceStore.ts))  
**Commit Reference:** `docs: audit context memory architecture`

---

## 1. Executive Summary

ContextSplit has progressively developed a sophisticated local intelligence engine across Days 1 through 16:
- **Foundational Capture & Storage (Days 1–11):** Items, Workspaces, Context Signals, Sessions, Vector Embeddings, Item Relationships, Context Candidates, Memberships, and Confidence Calibration.
- **Context Dynamics & Evolution (Days 12–15):** Context Transitions (`ContextStart`, `ContextSwitch`, `ContextReturn`, `ContextEnd`, `AmbiguousTransition`), Activity Segments (`ContextActivitySegment`), Temporal Activity Summaries (`ContextActivitySummaryService`), and Chronological Context Timelines (`get_context_timeline`).
- **Task Understanding & Minimal Consumer UI (Day 16):** Deterministic Task Evidence Collection (`TaskEvidenceCollector`), Verb-Subject Candidate Extraction (`TaskCandidateExtractor`), Multi-Factor Decoupled Task Ranking (`TaskCandidateRanker`), Task Transitions (`TaskTransitionDetector`), Inferred Task Working Memory (`ContextMemory`), and a calm, minimal Inbox redesign.

### The Objective of Day 17: Context Memory
Day 17 aims to consolidate these disparate components into a **coherent Context Memory layer**.
Rather than creating new redundant storage tables or duplicate data pipelines, the Context Memory layer must **DERIVE working memory from existing evidence, items, sessions, tasks, and transitions**.

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                        EXISTING VS. CONSOLIDATED CONTEXT MEMORY                        │
├────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                        │
│  [ CURRENT STATE: Fragmented & On-Demand ]                                             │
│                                                                                        │
│  ┌───────────────────────┐   ┌────────────────────────┐   ┌─────────────────────────┐  │
│  │ ContextTransitionDet  │   │ TaskTransitionDetector │   │ ContextOverviewService  │  │
│  │ In-memory history     │   │ In-memory history      │   │ On-demand extraction    │  │
│  │ (workspace switches)  │   │ (task switches)        │   │ (re-runs pipeline)      │  │
│  └───────────────────────┘   └────────────────────────┘   └─────────────────────────┘  │
│             ▲                             ▲                            ▲               │
│             └─────────────────────────────┼────────────────────────────┘               │
│                                           │ (No unified working memory service)        │
│                                           ▼                                            │
│  ┌──────────────────────────────────────────────────────────────────────────────────┐  │
│  │ ContextMemory (memory.rs) Struct exists, but is NOT wired to IPC or global state │  │
│  └──────────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                        │
│  ────────────────────────────────────────────────────────────────────────────────────  │
│                                                                                        │
│  [ TARGET STATE: Unified Derived Working Memory ]                                      │
│                                                                                        │
│  ┌──────────────────────────────────────────────────────────────────────────────────┐  │
│  │                             UNIFIED CONTEXT MEMORY                               │  │
│  │  - Context Identity: Active Workspace / Confirmed Context                        │  │
│  │  - Active Goal: Confident Inferred Task (or clean "Unknown")                     │  │
│  │  - Decoupled Confidence: Context Confidence vs. Task Confidence                 │  │
│  │  - Working Memory Traces: Recent task history, active duration, session streak   │  │
│  │  - Grounding Evidence: Top apps, active domains, recent items, salient signals    │  │
│  └──────────────────────────────────────────────────────────────────────────────────┘  │
│             ▲                                              ▲                           │
│             │ (derived on-demand or stateful)              │ (queries via IPC)         │
│  ┌──────────────────────────────────────────┐   ┌───────────────────────────────────┐  │
│  │ Reused Core Services:                    │   │ Client Surfaces:                  │  │
│  │ - TaskEvidenceCollector                  │   │ - ContextOverviewPanel            │  │
│  │ - TaskCandidateExtractor & Ranker        │   │ - TopBar ("Now working on")       │  │
│  │ - ContextTransition & TaskTransition     │   │ - ContextTimelineDrawer           │  │
│  │ - ContextActivitySegmentBuilder          │   │ - Inbox Guidance & Filters        │  │
│  └──────────────────────────────────────────┘   └───────────────────────────────────┘  │
│                                                                                        │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Answers to the 12 Specific Audit Questions

### 1. Where current context is determined
- **In the Engine (Item Evaluation):**
  - [`src-tauri/src/context_engine/engine.rs:ContextEngine::evaluate_item()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/engine.rs) evaluates incoming items against all workspaces using multi-factor scoring ([`scoring.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/scoring.rs)), competition margin ([`competition.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/competition.rs)), and calibrated confidence bands ([`calibration.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/calibration.rs)).
- **In Transition Detection (Dynamic Context):**
  - [`src-tauri/src/context_engine/transitions.rs:ContextTransitionDetector`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/transitions.rs) maintains `current_context: Option<ContextSnapshot>`. When a new item is evaluated, it verifies whether the context continues, switches, or returns from a recent detour.
- **In the Client UI (User Focus):**
  - [`src/store/workspaceStore.ts:useWorkspaceStore`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/store/workspaceStore.ts) tracks `activeWorkspaceId: string | "inbox" | null`. This represents the user's manual navigation focus in the application window.

### 2. Where current task is determined
- **On-Demand in Context Overview:**
  - [`src-tauri/src/context_engine/overview.rs:get_context_overview()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/overview.rs#L123-L236) dynamically runs the task understanding pipeline whenever a workspace overview is requested:
    1. `TaskEvidenceCollector::collect_for_context(conn, workspace_id)` retrieves up to 50 recent items, active signals, domains, and app names.
    2. `TaskCandidateExtractor::extract_from_evidence(&evidence_set, None)` identifies candidate verbs and subjects.
    3. `TaskCandidateRanker::rank_candidates(&candidates, Some(0.90), None)` scores the candidates across 8 dimensions.
    4. If the top candidate meets `confidence >= 0.45` and is non-generic, it is assigned as `overview.current_task` (`InferredTaskOverview`). Otherwise, it cleanly defaults to `"Not clear yet"`.
- **In Working Memory (`ContextMemory`):**
  - [`src-tauri/src/context_engine/memory.rs:ContextMemory`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory.rs#L302) maintains `pub current_task: InferredTaskState`. It has transition handlers (`update_from_ranked_candidate`, `update_from_task_transition`), but `ContextMemory` is currently instantiated only in tests and task extractor callers, not as a persistent singleton in `AppState`.

### 3. Where historical context activity is stored
- **SQLite Database Tables:**
  - `sessions` ([`db/mod.rs:80-87`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/mod.rs#L80-L87)): Stores user sessions with `id`, `started_at`, `last_activity_at`, `ended_at`, `status`.
  - `items` ([`db/mod.rs:41-53`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/mod.rs#L41-L53)): Stores items with `workspace_id`, `session_id`, `created_at`, `updated_at`, and raw `metadata`.
  - `context_signals` ([`db/mod.rs:60-73`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/mod.rs#L60-L73)): Stores raw telemetry signals with `workspace_id`, `item_id`, `signal_type`, `session_id`, `payload`, `created_at`.
  - `context_transitions` ([`db/mod.rs:388-405`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/mod.rs#L388-L405)): Stores explicit context changes (`from_context_id`, `to_context_id`, `transition_type`, `confidence`, `detected_at`, `session_id`, `trigger_item_id`, `metadata`).
  - `workspaces` ([`db/mod.rs:28-35`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/mod.rs#L28-L35), [`db/mod.rs:284-286`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/mod.rs#L284-L286)): Stores `last_activity_at`, `lifecycle_state` (`active`, `dormant`, `archived`), and `archived_at`.
- **Derived Activity Representations:**
  - [`segments.rs:ContextSegmentBuilder`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/segments.rs): Reconstructs temporal bursts of activity (`ContextActivitySegment`) by querying items and transitions.

### 4. Where task history is stored
- **In-Memory History Stacks:**
  - `ContextMemory.task_history: Vec<TaskSnapshot>` ([`memory.rs:312`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory.rs#L312)): Bounded FIFO stack of previous tasks in that context.
  - `TaskTransitionDetector.task_history: VecDeque<TaskSnapshot>` ([`task_transitions.rs:88`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_transitions.rs#L88)): Bounded queue (depth 5) used to recognize `TaskReturn` events.
- **SQLite Database Table:**
  - `task_candidates` table ([`db/mod.rs:433-446`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/mod.rs#L433-L446)): Stores `id`, `context_id`, `title`, `confidence`, `status`, `first_detected_at`, `last_active_at`, `created_at`, `updated_at`.
  - Queried via [`db/operations.rs:list_task_candidates_for_context()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/operations.rs#L1952).
  - *Gap:* There is currently no persistent `task_transitions` table logging timestamped task changes (e.g., switched from Task 1 to Task 2); task transitions are currently transient.

### 5. How context transitions are stored
- **In Database:**
  - Stored in SQLite table `context_transitions` (migration v20 in [`db/mod.rs:388`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/mod.rs#L388)).
  - Model: [`ContextTransition`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs#L562) struct in `db/models.rs`.
  - Operations: [`db/operations.rs:create_context_transition()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/operations.rs#L1675), [`list_context_transitions()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/operations.rs#L1713), and [`list_context_transitions_for_context()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/operations.rs#L1756).
- **In Engine Detection:**
  - Evaluated by [`transitions.rs:ContextTransitionDetector`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/transitions.rs), which records transitions via [`transitions.rs:record_context_transition()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/transitions.rs#L1336).

### 6. How activity segments are represented
- **Stateless Reconstructed Struct:**
  - Represented as [`ContextActivitySegment`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/segments.rs#L26-L41) in `segments.rs`:
    ```rust
    pub struct ContextActivitySegment {
        pub id: String,
        pub context: Option<ContextReference>,
        pub started_at: String,
        pub ended_at: String,
        pub duration_seconds: i64,
        pub duration_formatted: String,
        pub confidence: f64,
        pub session_ids: Vec<String>,
        pub item_references: Vec<SegmentItemReference>,
        pub transition_in: Option<DetectedTransition>,
        pub transition_out: Option<DetectedTransition>,
        pub is_ambiguous: bool,
        pub item_count: usize,
        pub metadata: Value,
    }
    ```
  - Reconstructed on demand by `ContextSegmentBuilder::build_segments()` by clustering items with timestamp deltas $< 1800$ seconds and aligning with recorded `context_transitions`.

### 7. How recent items are retrieved
- **Database Operations:**
  - [`db/operations.rs:list_items(conn, workspace_id)`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/operations.rs#L286): Queries `items` table filtered by `workspace_id` and ordered by `created_at DESC`.
- **Timeline Engine:**
  - [`timeline.rs:get_context_timeline(conn, workspace_id)`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/timeline.rs#L12): Retrieves items with membership joined, maps to `TimelineItemSummary`, and sorts newest first.
- **Overview Service:**
  - [`overview.rs:get_context_overview()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/overview.rs#L113): Takes the top 6 items from `timeline.items`.
- **Task Evidence Collector:**
  - [`task_evidence.rs:TaskEvidenceCollector::collect_for_context()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_evidence.rs#L955): Queries up to 50 recent items within the context time window (default 24 hours).

### 8. How context summaries are currently generated
1. **Statistical Temporal Summary:**
   - [`summary.rs:ContextActivitySummaryService::summarize_workspace_activity()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/summary.rs#L88): Computes total active duration, context switches, returns, longest segment, sequence of shifts, and application breakdown.
2. **Context Overview:**
   - [`overview.rs:get_context_overview()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/overview.rs#L39): Aggregates workspace lifecycle state, item count, recent activity, top 5 applications, top 5 domains, recent items, chronological timeline events, and the inferred current task.
3. **Plain-Text Working Memory Summary:**
   - [`memory.rs:ContextMemory::format_summary()`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory.rs#L720): Formats the active memory into a clean, human-readable text block:
     ```text
     Context:
     Portfolio

     Current task:
     Hero animation

     Task confidence:
     High

     Task evidence:
     - recent Figma activity
     - related React items
     ```

### 9. Which existing services can be reused
The codebase has rich, mature services ready for direct reuse:
- **`TaskEvidenceCollector` ([`task_evidence.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_evidence.rs)):** Gathers and normalizes signals, URLs, window titles, and items for any workspace without LLM dependencies.
- **`TaskCandidateExtractor` ([`task_extractor.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_extractor.rs)):** Deterministic rule-based extractor producing concise verb-noun task candidates.
- **`TaskCandidateRanker` ([`task_ranking.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_ranking.rs)):** Multi-factor scoring engine evaluating 8 dimensions with ambiguity detection.
- **`ContextMemory` ([`memory.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory.rs)):** Ready-made working memory struct with decoupled task/context confidence, streak counting, and history tracking.
- **`TaskTransitionDetector` ([`task_transitions.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_transitions.rs)):** Detects `TaskStart`, `TaskSwitch`, `TaskReturn`, and `TaskEnd`.
- **`ContextTransitionDetector` ([`transitions.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/transitions.rs)):** Evaluates context switches and returns.
- **`ContextSegmentBuilder` ([`segments.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/segments.rs)):** Builds continuous bursts of contextual focus.
- **`ContextActivitySummaryService` ([`summary.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/summary.rs)):** Calculates statistical time rollups and sequence strings.
- **`get_context_timeline` ([`timeline.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/timeline.rs)):** Chronological event aggregation.
- **`get_context_overview` ([`overview.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/overview.rs)):** Context summary builder.

### 10. Which information is currently duplicated
- **Domain Extraction:** Implemented independently in:
  - `overview.rs:extract_domain_from_url()`
  - `task_extractor.rs:extract_domain_from_url()`
  - `timeline.rs:Url::parse()`
- **App Name Sanitization:** Implemented independently in:
  - `overview.rs:normalize_app_name()`
  - `task_extractor.rs:normalize_app_name()`
  - `evidence.rs:normalize_app_name()`
- **Task Inference Pipeline Execution:**
  - `overview.rs` re-implements a manual pipeline:
    `TaskEvidenceCollector -> TaskCandidateExtractor -> TaskCandidateRanker -> format InferredTaskOverview`
    instead of calling or leveraging `ContextMemory`.
- **Recent Item Representation:** Duplicated across:
  - `TimelineItemSummary` (`timeline.rs`)
  - `SegmentItemReference` (`segments.rs`)
  - `InferredTaskOverview.recent_activity` (`overview.rs`)
- **Confidence Band Categorization:**
  - `transitions.rs` uses `High | Medium | Low | None`.
  - `memory.rs` uses `High | Medium | Low | Insufficient`.
  - `calibration.rs` uses calibrated bands for items (`high`, `moderate`, `weak`, `none`).

### 11. Whether any current UI already attempts to show "what you're working on"
- **Yes, in [`src/components/workspace/ContextOverviewPanel.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/workspace/ContextOverviewPanel.tsx#L285-L392):**
  - Section title: `"You're working on"` + Workspace Name + `"Confirmed Context"` badge.
  - Sub-section: `"Current task"` with `"Inferred"` badge and `"Task confidence"` badge (`High`, `Medium`, or `Low`).
  - Task title (rendered cleanly or in italics as `"Not clear yet"` if insufficient).
  - Human explanation block: e.g., `"Recently working with Figma and Safari resources related to the hero animation."`
  - `"Recent activity"` list with bullets of top 3 recent item titles.
  - `"Task evidence"` pills showing grounding sources.
- **In [`src/components/layout/TopBar.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/layout/TopBar.tsx):**
  - Shows buttons to open "Overview" and "Timeline" for the active workspace, but does **not** yet display the active context or task directly in the bar.
- **In [`src/components/sidebar/Sidebar.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/sidebar/Sidebar.tsx):**
  - Highlights the selected workspace, but has no active task memory indicator.

### 12. How Context Overview currently obtains its data
1. **User Action:** The user opens the overview drawer by clicking "Overview" in `TopBar` or a workspace action button.
2. **Frontend State & DB Call:**
   - [`src/components/workspace/ContextOverviewPanel.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/workspace/ContextOverviewPanel.tsx#L55) calls `db.getContextOverview(wsId)`.
   - [`src/services/db.ts:1090`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/services/db.ts#L1090) invokes Tauri command: `invoke("get_context_overview", { workspaceId })`.
3. **Tauri IPC Command:**
   - [`src-tauri/src/lib.rs:641`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/lib.rs#L641) receives `get_context_overview`, acquires `db_conn` lock, and calls `crate::context_engine::overview::get_context_overview(&conn, workspace_id)`.
4. **Backend Assembly:**
   - Reads `workspaces` table for lifecycle state and timestamps.
   - Calls `timeline::get_context_timeline` to fetch all workspace items and sessions.
   - Aggregates top 5 apps and top 5 domains from items and `context_signals`.
   - Takes top 6 recent items.
   - Collects task evidence using `TaskEvidenceCollector`, extracts candidates using `TaskCandidateExtractor`, ranks candidates using `TaskCandidateRanker`, and packages `InferredTaskOverview`.

---

## 3. Existing Architecture Inventory

### Database Tables (`src-tauri/src/db/mod.rs`)

| Table Name | Migration Version | Purpose in Context Memory | Reusability |
| :--- | :--- | :--- | :--- |
| `workspaces` | v1, v14 | Context identity container, lifecycle state (`active`, `dormant`, `archived`), `last_activity_at`. | Authoritative context entity |
| `items` | v2, v4 | Captured items with `workspace_id`, `session_id`, `created_at`, `metadata`. | Primary evidence source |
| `sessions` | v4 | Continuous user sessions with `started_at`, `last_activity_at`, `ended_at`, `status`. | Temporal boundary grounding |
| `context_signals` | v3 | Raw background events (app focus, window titles, URLs, timestamps). | Empirical observation evidence |
| `item_relationships` | v5 | Local graph edges (semantic similarity, temporal proximity, same app/domain). | Task cluster grounding |
| `context_memberships`| v12 | Confirmed, suggested, or dismissed item-to-workspace links. | Context boundary grounding |
| `context_transitions`| v20 | Recorded context transitions (`from_context_id`, `to_context_id`, `transition_type`, `detected_at`). | Context sequence memory |
| `task_candidates` | v21 | Detected task candidates (`title`, `confidence`, `status`, `last_active_at`). | Prior task candidate evidence |

> [!IMPORTANT]
> **No new database tables are required for Context Memory.**
> Creating redundant tables for items, tasks, or sessions would violate our core principle of deriving memory from existing evidence.

### Rust Services (`src-tauri/src/context_engine/`)

| Service / Struct | File | Primary Responsibility | Reusability in Context Memory |
| :--- | :--- | :--- | :--- |
| `ContextMemory` | [`memory.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory.rs) | Unified working memory model holding context reference, decoupled confidence, active task state, task history, context history. | **Core foundation** to be exposed & integrated |
| `InferredTaskState` | [`memory.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory.rs) | Detailed state of the active inferred task, streak count, evidence lines. | Ready for IPC serialization |
| `TaskEvidenceCollector` | [`task_evidence.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_evidence.rs) | Extracts 11 types of task evidence from recent items, signals, and sessions. | Essential for memory derivation |
| `TaskCandidateExtractor` | [`task_extractor.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_extractor.rs) | Deterministic rule engine for verb-subject task extraction. | Reusable without modification |
| `TaskCandidateRanker` | [`task_ranking.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_ranking.rs) | Scores candidates across 8 dimensions; detects ambiguity states. | Reusable without modification |
| `TaskTransitionDetector` | [`task_transitions.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_transitions.rs) | Tracks task history queue; evaluates switches, returns, and ends. | Reusable for state transitions |
| `ContextTransitionDetector` | [`transitions.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/transitions.rs) | Evaluates context switches, returns, and continuations. | Reusable for context transitions |
| `ContextSegmentBuilder` | [`segments.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/segments.rs) | Reconstructs temporal bursts of activity. | Used for temporal memory |
| `ContextActivitySummaryService`| [`summary.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/summary.rs) | Aggregates time metrics and context usage sequences. | Used for historical memory |
| `get_context_timeline` | [`timeline.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/timeline.rs) | Chronological aggregation of items, sessions, and events. | Reusable for timeline integration |
| `get_context_overview` | [`overview.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/overview.rs) | Workspace overview coordination. | Needs refactoring to use `ContextMemory` |

### React Components & Client Services (`src/`)

| Component / Service | File | Role | Status / Action Needed |
| :--- | :--- | :--- | :--- |
| `ContextOverviewPanel` | [`src/components/workspace/ContextOverviewPanel.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/workspace/ContextOverviewPanel.tsx) | Renders "You're working on" card, task confidence, explanation, recent activity. | Already displays task overview; can connect to unified memory |
| `ContextTimelineDrawer` | [`src/components/timeline/ContextTimelineDrawer.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/timeline/ContextTimelineDrawer.tsx) | Displays chronological events, sessions, transitions. | Working; can link to task blocks |
| `TopBar` | [`src/components/layout/TopBar.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/layout/TopBar.tsx) | Global application header. | Currently only has Overview/Timeline buttons; could display current working context/task |
| `useWorkspaceStore` | [`src/store/workspaceStore.ts`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/store/workspaceStore.ts) | Zustand store for workspaces, active context, overview drawer state. | Holds active selection |
| `db` Service Wrapper | [`src/services/db.ts`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/services/db.ts) | Type definitions and `invoke()` calls. | Missing `getContextMemory` IPC binding |

---

## 4. Missing Pieces & Architectural Gaps

1. **`ContextMemory` is Isolated from the Runtime State:**
   - [`src-tauri/src/context_engine/memory.rs:ContextMemory`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory.rs) has a complete, tested implementation, but is **never instantiated or maintained in `AppState`**, nor exposed via a Tauri command.
   - It functions as an uninvoked library struct rather than an active service.

2. **Stateless Recalculation in `ContextOverview`:**
   - [`overview.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/overview.rs) manually invokes `TaskEvidenceCollector -> TaskCandidateExtractor -> TaskCandidateRanker` every time the user opens the panel.
   - It does not maintain streak counts, previous task history, or task switch transitions.
   - Two consecutive opens of the overview have zero shared memory of prior task inferences.

3. **No Dedicated `get_context_memory` IPC Command:**
   - The frontend cannot query `ContextMemory` directly. It can only query `get_context_overview`, which returns an ad-hoc `InferredTaskOverview`.

4. **Task Transitions are Not Logged:**
   - While context transitions are persisted in `context_transitions` (table v20), task transitions (`task_switch`, `task_return`) exist only in `TaskTransitionDetector`'s in-memory `VecDeque`. If the app restarts, task transition history is lost.

5. **Disconnection Between System Active Context and UI Focus:**
   - `ContextTransitionDetector` tracks the system's inferred active context from items and signals.
   - `useWorkspaceStore` tracks the user's UI selection (`activeWorkspaceId`).
   - There is no unified mechanism to query "What is the user *currently* working on across the entire system right now?" vs "What is the memory state of Workspace X?".

---

## 5. Potential Duplication & Redundancy

| Duplicated Area | Occurrences in Codebase | Recommended Consolidation |
| :--- | :--- | :--- |
| **Domain Parsing** | `overview.rs:extract_domain_from_url`<br>`task_extractor.rs:extract_domain_from_url`<br>`timeline.rs:Url::parse` | Consolidate into a shared utility function in `context_engine/mod.rs` or `normalization.rs`. |
| **App Name Sanitization** | `overview.rs:normalize_app_name`<br>`task_extractor.rs:normalize_app_name`<br>`evidence.rs:normalize_app_name` | Consolidate into a single shared helper. |
| **Task Inference Execution** | `overview.rs:get_context_overview` (manual chain)<br>`memory.rs:update_from_ranked_candidate` | Refactor `overview.rs` to derive its task data directly from a `ContextMemory` builder/query. |
| **Recent Activity Item Strings** | `TimelineItemSummary` (`timeline.rs`)<br>`InferredTaskOverview.recent_activity` (`overview.rs`)<br>`SegmentItemReference` (`segments.rs`) | Reuse `TimelineItemSummary` as the standard item projection. |
| **Confidence Band Logic** | `transitions.rs` (4 levels)<br>`memory.rs` (4 levels)<br>`calibration.rs` (4 levels) | Standardize on the qualitative bands defined in `memory.rs` (`High`, `Medium`, `Low`, `Insufficient`). |

---

## 6. Recommended Context Memory Architecture

### Core Design Principles
1. **Derive, Do Not Duplicate:** Working memory must be derived dynamically from existing evidence, items, sessions, and transitions. Do not create duplicate tables for items, tasks, or sessions.
2. **Context Identity Invariance:** Context identity (e.g. Workspace "Portfolio") is authoritative and never overwritten by task names.
3. **Decoupled Confidence:** Task confidence is calculated separately from context confidence. If evidence is weak, task title cleanly defaults to `"Unknown"`.
4. **Fast & Deterministic:** Derivation runs locally in milliseconds with zero mandatory LLM dependency.
5. **Unified Access Point:** Both workspace-scoped memory (`ContextMemory` for Workspace A) and global active memory ("What you are working on right now") use the same data structures.

### Recommended System Architecture

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                        RECOMMENDED CONTEXT MEMORY PIPELINE                             │
├────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                        │
│  1. Evidence Aggregation (Reused)                                                      │
│     TaskEvidenceCollector::collect_for_context(conn, workspace_id)                     │
│     ├── Recent Items (title, snippet, app_name, domain)                                │
│     ├── Foreground Window & Signal Telemetry                                           │
│     ├── Context Transitions (from `context_transitions` table)                         │
│     └── Active Session Context (from `sessions` table)                                 │
│                                                                                        │
│  2. Deterministic Candidate Extraction & Ranking (Reused)                              │
│     TaskCandidateExtractor::extract_from_evidence(&evidence_set, None)                 │
│     TaskCandidateRanker::rank_candidates(&candidates, context_confidence, prev_task)   │
│                                                                                        │
│  3. Working Memory Assembly (Enhanced `ContextMemoryService`)                          │
│     ContextMemory::derive_for_workspace(conn, workspace_id)                            │
│     ├── Set Context Reference & Context Confidence                                     │
│     ├── Apply Top Ranked Task (or default to Unknown if confidence < 0.45)             │
│     ├── Populate Task Evidence & Streak Count                                          │
│     ├── Reconstruct Recent Task History from `task_candidates` & recent activity        │
│     └── Attach Recent Activity Items                                                   │
│                                                                                        │
│  4. IPC Interface & Client Store                                                       │
│     Tauri Command: `get_context_memory(workspace_id?: Option<String>) -> ContextMemory`│
│     Frontend Service: `db.getContextMemory(workspaceId?: string)`                      │
│     Frontend Store: `useContextMemoryStore` or hook in `ContextOverviewPanel` / TopBar │
│                                                                                        │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### Proposed Rust Interface (`src-tauri/src/context_engine/memory.rs`)
Add a constructor/derivation function on `ContextMemory`:
```rust
impl ContextMemory {
    /// Derives the active working memory for a specific workspace from existing database evidence.
    pub fn derive_for_workspace(
        conn: &rusqlite::Connection,
        workspace_id: &str,
    ) -> Result<Self, String> { ... }

    /// Derives the global active working memory across the entire system.
    pub fn derive_active(
        conn: &rusqlite::Connection,
    ) -> Result<Option<Self>, String> { ... }
}
```

### Proposed Tauri Commands (`src-tauri/src/lib.rs`)
```rust
#[tauri::command]
fn get_context_memory(
    workspace_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<crate::context_engine::memory::ContextMemory, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    // If workspace_id provided, derive for that workspace;
    // otherwise derive for currently active context.
    ...
}
```

---

## 7. Test Coverage Gaps & Verification Strategy

### Current Test Coverage
- **`memory.rs` Unit Tests (8 tests):**
  - Summary formatting for confident task (`test_format_summary_confident_portfolio_hero_animation`).
  - Fallback to `"Unknown"` on low confidence (`test_format_summary_insufficient_confidence_falls_back_to_unknown`).
  - Workspace name preservation (`test_do_not_replace_underlying_workspace_name`).
  - Decoupled confidence metrics (`test_decoupled_confidence_metrics`).
  - Task history tracking on switch (`test_task_history_tracking_on_switch`).
  - Context switch archiving both context and task (`test_context_switch_archives_both_context_and_task`).
- **`task_adversarial_tests.rs` (10 tests):**
  - Conflicting signals, generic titles, rapid task flapping, ambiguous switches.

### Identified Test Coverage Gaps
1. **End-to-End Database Derivation Test:** No test currently creates real SQLite items, signals, and sessions, and asserts that `ContextMemory::derive_for_workspace` correctly produces a populated `ContextMemory`.
2. **Overview & Memory Consistency Test:** No test asserts that `get_context_overview` and `ContextMemory` yield the exact same task title, confidence band, and evidence for a given workspace.
3. **Empty / New Context Memory Test:** No test verifies the memory behavior for a freshly created workspace with zero items (must return clean `Unknown` task with zero errors).
4. **IPC Contract & Serialization Test:** No test verifies JSON serialization of `ContextMemory` matching the frontend TypeScript interfaces.
5. **Multi-Session Task Continuity Test:** No test verifies that returning to a workspace after an inactivity gap correctly resumes the previous task if signals remain consistent.

---

## 8. Summary & Next Steps

| Component | Status | Day 17 Action |
| :--- | :--- | :--- |
| **`ContextMemory` struct** | Fully implemented in `memory.rs` | Add `derive_for_workspace` & `derive_active` constructors |
| **`overview.rs`** | Working, but duplicates task extraction | Refactor to consume `ContextMemory` |
| **Tauri IPC** | Missing `get_context_memory` | Add `get_context_memory` command in `lib.rs` |
| **Frontend `db.ts`** | Missing `ContextMemory` types | Add `ContextMemory` interface and `db.getContextMemory` |
| **Frontend UI** | Context Overview has task card; TopBar lacks indicator | Keep Overview intact; optionally add subtle indicator in TopBar |
| **Database Tables** | All required tables exist | **No schema changes required** |

This completes the architecture audit for Day 17 Step 1.
