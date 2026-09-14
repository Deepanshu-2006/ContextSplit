# Context Workset Architecture Audit (Day 18 — Step 1)

**Status:** Completed  
**Date:** September 2026  
**Target Subsystems:**
- `context_engine/` ([`memory.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory.rs), [`memory_query.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory_query.rs), [`task_evidence.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_evidence.rs), [`task_extractor.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_extractor.rs), [`task_ranking.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_ranking.rs), [`task_transitions.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_transitions.rs), [`transitions.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/transitions.rs), [`segments.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/segments.rs), [`summary.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/summary.rs), [`work_sequence.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/work_sequence.rs), [`attention.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/attention.rs), [`relationships.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/relationships.rs), [`temporal.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/temporal.rs), [`scoring.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/scoring.rs), [`engine.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/engine.rs))
- `db/` ([`models.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs), [`operations.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/operations.rs), [`mod.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/mod.rs))
- Frontend (`src/`) ([`src/services/contextMemory.ts`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/services/contextMemory.ts), [`src/services/db.ts`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/services/db.ts), [`src/store/workspaceStore.ts`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/store/workspaceStore.ts), [`src/store/inboxStore.ts`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/store/inboxStore.ts), [`src/components/workspace/ContextOverviewPanel.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/workspace/ContextOverviewPanel.tsx), [`src/components/timeline/ContextTimelineDrawer.tsx`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src/components/timeline/ContextTimelineDrawer.tsx))  
**Commit Reference:** `docs: audit context workset architecture`

---

## 1. Executive Summary

ContextSplit has systematically constructed the foundation of local contextual intelligence over Days 1 through 17:
- **Days 1–11 (Capture, Storage & Semantic Grounding):** Items, Workspaces, Context Signals, Sessions, Vector Embeddings, Item Relationships, Context Candidates, Memberships, and Calibrated Confidence.
- **Days 12–15 (Context Dynamics & Temporal Evolution):** Context Transitions (`ContextStart`, `ContextSwitch`, `ContextReturn`, `ContextEnd`, `AmbiguousTransition`), Activity Segments (`ContextActivitySegment`), Temporal Activity Summaries (`ContextActivitySummaryService`), and Context Timelines.
- **Day 16 (Task Understanding):** Deterministic Task Evidence Gathering, Verb-Subject Candidate Extraction, Multi-Factor Decoupled Task Ranking, and Task Transitions.
- **Day 17 (Context Memory & Work Sequences):** Epistemically honest Working Memory (`ContextMemory`), Cross-Context Chronological Work Sequences (`RecentWorkSequenceService`), and Historical Memory Queries (`get_today_memory`, `get_yesterday_memory`).

### The Objective of Day 18: Context Workset / Working State Reconstruction
Day 18 introduces the **Context Workset layer**. When a user resumes work in a context (e.g., switches to workspace "TripWise" or resumes work after a weekend), they should not be faced with a static, unranked dump of dozens of accumulated items.

Instead, ContextSplit must reconstruct the **active working state (Workset)**:
1. **What was I specifically working on?** (The active task and goal from Context Memory).
2. **What items are core to this active state right now?** (Primary working documents, recently referenced links, highly active tools).
3. **What items are peripheral/secondary references?** (Background research, older context items, related assets).
4. **What items from the Inbox or recent sessions should be suggested for inclusion?** (High-confidence suggested items not yet filed).
5. **What was the immediate sequence before the pause/switch?** (Recent detour artifacts, last modified files).

> [!IMPORTANT]
> **Cardinal Architectural Rule for Day 18:**
> **Do NOT create a second parallel context-ranking system.**
> The Workset layer is a pure consumer and aggregator. It must ingest the outputs of the existing `ContextEngine`, `TaskEngine`, `ItemRelationships`, `ContextMemory`, `AttentionEngine`, and `TemporalDecay` systems to reconstruct working state deterministically without duplicating data or creating competing scoring engines.

```
┌───────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                 DAY 18 CONTEXT WORKSET LAYER OVERVIEW                                 │
├───────────────────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                                       │
│   ┌───────────────────────────────────────────────────────────────────────────────────────────────┐   │
│   │                                EXISTING UPSTREAM INTELLIGENCE                                 │   │
│   │                                                                                               │   │
│   │  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────────────┐  │   │
│   │  │  ContextEngine   │  │    TaskEngine    │  │  ContextMemory   │  │   ItemRelationships   │  │   │
│   │  │  - Memberships   │  │  - Extractor     │  │  - Active Task   │  │   - Graph Strength    │  │   │
│   │  │  - Multi-scoring │  │  - Ranker        │  │  - Provenance    │  │   - Semantic Sim      │  │   │
│   │  │  - Calibration   │  │  - Transitions   │  │  - Activity Seq  │  │   - Temporal Co-occur │  │   │
│   │  └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘  └───────────┬───────────┘  │   │
│   └───────────┼─────────────────────┼─────────────────────┼────────────────────────┼──────────────┘   │
│               │                     │                     │                        │                  │
│               ▼                     ▼                     ▼                        ▼                  │
│   ┌───────────────────────────────────────────────────────────────────────────────────────────────┐   │
│   │                        CONTEXT WORKSET RECONSTRUCTION ENGINE (DAY 18)                         │   │
│   │                                                                                               │   │
│   │  - Reconstructs active working state for a context / workspace                                │   │
│   │  - Classifies items into actionable workset tiers: Primary, Reference, Suggested, Recent      │   │
│   │  - Attaches active task grounding & provenance from ContextMemory                             │   │
│   │  - Computes composite Workset Relevance Score (Task Alignment + Recency + Relational Density) │   │
│   │  - Provides clean, lightweight DTOs to consumer UI surfaces                                   │   │
│   └───────────────────────────────────────────────┬───────────────────────────────────────────────┘   │
│                                                   │                                                   │
│                                                   ▼                                                   │
│   ┌───────────────────────────────────────────────────────────────────────────────────────────────┐   │
│   │                                    CLIENT CONSUMER SURFACES                                   │   │
│   │                                                                                               │   │
│   │  ┌─────────────────────────┐  ┌──────────────────────────┐  ┌──────────────────────────────┐  │   │
│   │  │   Active Workset View   │  │    Resume Work Banner    │  │   Focused Canvas Filter      │  │   │
│   │  │   (Prioritized Cards)   │  │    (TopBar / Drawer)     │  │   (Hide Stale Clutter)       │  │   │
│   │  └─────────────────────────┘  └──────────────────────────┘  └──────────────────────────────┘  │   │
│   └───────────────────────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                                       │
└───────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Answers to the 10 Specific Audit Questions

### 1. How items are associated with contexts
Items in ContextSplit are associated with contexts through two complementary mechanisms:
- **Direct Physical Assignment (Confirmed Workspace):**
  - Stored directly on `items.workspace_id: Option<String>` in SQLite ([`src-tauri/src/db/models.rs:17`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs#L17)).
  - `workspace_id = None` denotes an item residing in the unorganized Inbox.
  - `workspace_id = Some(uuid)` denotes confirmed membership in that Workspace.
- **Context Membership Join Table (`context_memberships`):**
  - Stored in table `context_memberships` ([`src-tauri/src/db/models.rs:238-249`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs#L238-L249)) with columns `(id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at)`.
  - Allows many-to-one or many-to-many contextual associations with explicit confidence ratings and provenance.
- **Context Candidate Clusters (`context_candidates` & `context_candidate_items`):**
  - Emergent, unconfirmed context groupings discovered from unorganized inbox items ([`src-tauri/src/db/models.rs:98-120`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs#L98-L120)).

### 2. How confirmed vs suggested membership is represented
ContextSplit enforces strict physical and epistemic isolation between confirmed and suggested items:
- **Confirmed Membership (`membership_type: "confirmed"`):**
  - `item.workspace_id` is updated to the workspace UUID.
  - `context_memberships.confidence = 1.0`, `source = "user"` (or `"confirmed"`).
  - Item appears in the workspace's standard file list (`list_items(Some(workspace_id))`).
- **Suggested Membership (`membership_type: "suggested"`):**
  - `item.workspace_id` remains `None` (item stays safely in the Inbox without involuntary file moves).
  - `context_memberships` holds a record with `membership_type = "suggested"`, `confidence = [0.0, 1.0]`, and `source = "context_engine"` or `"context_discovery"`.
  - In the Inbox, the item's state is `InboxState::Suggested` or `InboxState::NeedsReview` ([`src-tauri/src/db/models.rs:551-573`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs#L551-L573)).
- **Dismissed Membership (`membership_type: "dismissed"`):**
  - `context_memberships.confidence = 0.0`, `source = "user"`. Suppresses future suggestions of that item for that workspace.

### 3. How item relationships are represented
Item relationships are stored in the `item_relationships` SQLite table ([`src-tauri/src/db/models.rs:49-57`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs#L49-L57)) and calculated in [`src-tauri/src/context_engine/relationships.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/relationships.rs):
- **Schema:** `(id, item_a_id, item_b_id, relationship_type, strength, metadata, created_at)` with a unique constraint on `(item_a_id, item_b_id, relationship_type)` where `item_a_id < item_b_id` lexicographically.
- **Relationship Types & Base Strengths:**
  - `same_session`: Captured during the same active user session ($+0.4$).
  - `temporal_proximity`: Captured within $\le 5$ minutes of each other ($+0.2$).
  - `same_source_app`: Captured from the same foreground application ($+0.3$).
  - `same_domain`: URL items originating from the same web domain ($+0.3$).
  - `same_workspace`: Assigned to the same workspace ($+0.2$).
  - `semantic_similarity`: Vector cosine similarity $\ge 0.85$ (strength up to $1.0$).
- **Graph Traversal:** [`src-tauri/src/context_engine/graph.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/graph.rs) provides clustering, centrality, and neighborhood queries over these edges.

### 4. How task relevance is represented
Task relevance is captured through the Task Engine pipeline:
- **Task Candidates:** Table `task_candidates` ([`src-tauri/src/db/models.rs:920-930`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/db/models.rs#L920-L930)) represents inferred goals with `title`, `context_id`, `confidence`, `status` (`Detected`, `Active`, `Inactive`, `Uncertain`, `Dismissed`), `first_detected_at`, and `last_active_at`.
- **Decoupled Scoring:** [`src-tauri/src/context_engine/task_ranking.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/task_ranking.rs) calculates task relevance using an 8-factor ranker:
  1. Direct item support count (number of items mentioning or containing the task terms)
  2. Temporal recency & burstiness
  3. Relational density (item-to-item connections)
  4. App & window title coherence
  5. Domain alignment
  6. Semantic similarity to context profile
  7. Stability across session streaks
  8. Historical user confirmation / dismissal signals
- **Item-to-Task Linkage:** Inferred tasks in `ContextMemory` and `TaskEvidenceSet` track explicit `item_ids: Vec<String>` that directly support the task hypothesis.

### 5. How recent activity is retrieved
Recent activity is aggregated deterministically through multiple layers:
- **Activity Segments (`ContextActivitySegment`):** [`src-tauri/src/context_engine/segments.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/segments.rs) groups chronological items, telemetry signals, and transitions into temporal work sessions (using a 15-minute inactivity boundary).
- **Activity Summaries (`ContextActivitySummaryService`):** [`src-tauri/src/context_engine/summary.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/summary.rs) transforms segments into human-friendly duration estimates, top applications, and domains.
- **Work Sequences (`RecentWorkSequenceService`):** [`src-tauri/src/context_engine/work_sequence.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/work_sequence.rs) reconstructs cross-context chronological daily milestones ("Today", "Yesterday", "Sep 12").
- **Database Indexing:** Queries in `db/operations.rs` utilize indexed `created_at`, `detected_at`, and `last_activity_at` timestamps bounded by `LIMIT` clauses.

### 6. How context transitions identify the user's latest context
Context transitions track the chronological flow of user attention:
- **Table `context_transitions`:** Records every shift with `(from_context_id, to_context_id, transition_type, confidence, detected_at, trigger_item_id)`.
- **Transition Types:**
  - `ContextStart`: Initial activation of a context.
  - `ContextSwitch`: Clean transition from Workspace A to Workspace B.
  - `ContextReturn`: Resuming Workspace A after a brief detour to Workspace B or Inbox.
  - `ContextEnd`: Explicit conclusion/session termination.
  - `AmbiguousTransition`: Shift with low confidence or competing candidates.
- **Latest Context Resolution:** `ContextMemoryQueryService::get_last_active_context()` finds the most recently touched workspace using `COALESCE(last_activity_at, updated_at) DESC`, cross-referenced with recent transition records to flag returns or switches.

### 7. How ContextMemory identifies the current/recent task
[`src-tauri/src/context_engine/memory.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/memory.rs) reconstructs the working task state via `ContextMemoryBuilder`:
- Retrieves recent items, signals, and active `task_candidates` for the context.
- Evaluates candidate confidence against the decoupled threshold ($\text{confidence} \ge 0.45$, qualitative band $\ge \text{Medium}$).
- Filters out generic labels ("Working", "Browsing web") via `is_generic_task_name()`.
- If a valid candidate exists, sets `current_task = InferredTaskState` with provenance (`Observed`, `Inferred`, `Confirmed`), supporting `item_ids`, and why-reasons.
- If evidence is weak or absent, sets `current_task` cleanly to `"Unknown"` with zero hallucinated task names.

### 8. Whether there is already a notion of important/relevant items
Yes, multiple subsystems calculate item relevance in specific contexts:
- **Attention Engine (`ItemAttention`):** [`src-tauri/src/context_engine/attention.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/attention.rs) scores inbox items for user attention based on urgency, candidate emergence, and ambiguity.
- **Candidate Relevance (`ContextCandidateItem.relevance_score`):** Represents how central an item is to a discovered cluster ($0.0 \dots 1.0$).
- **Memory Item References (`MemoryItemReference`):** Lightweight prioritized summaries attached to `ContextMemory` and `HistoricalContextMemoryResponse.important_captured_items`.
- **Temporal Weighting (`TemporalDecayConfig`):** [`src-tauri/src/context_engine/temporal.rs`](file:///Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/context_engine/temporal.rs) calculates exponential half-life decay on older signals.
- **Search Scoring:** `search::search_inbox_items` combines BM25 text match, cosine embedding similarity, and recency.
- *Gap:* There is currently no unified **Context Workset** construct that aggregates and categorizes items into **Primary Working Items**, **Secondary References**, **Suggested Additions**, and **Recent Detour Artifacts** for a specific context session.

### 9. Whether items have enough metadata to rank contextual relevance
**Yes, abundantly.** Inspecting `items`, `context_signals`, `item_relationships`, and `item_embeddings`:
- **Core Attributes:** `id`, `workspace_id`, `session_id`, `item_type` (`text`, `url`, `image`, `file`), `content`, `title`, `metadata`, `created_at`, `updated_at`.
- **Rich Capture Metadata:** JSON `metadata` stores `app_name`, `window_title`, `process_name`, `domain`, `original_url`, `filename`, `file_path`, `captured_via`.
- **Relational Edges:** Precomputed explicit edges in `item_relationships` with strengths and semantic similarity scores.
- **Temporal Signals:** Millisecond-precision timestamps across `items`, `context_signals`, `sessions`, and `context_transitions`.
- **Epistemic Tracking:** First-class `ContextMembership` and `InboxItemState` tables tracking user feedback, confirmations, and dismissals.
- *Conclusion:* **No schema mutations or new database tables are required** for basic item metadata. All required signals are already captured.

### 10. Existing APIs/services that should be reused
The Workset layer can directly reuse and orchestrate:
| Subsystem | Existing Rust Service | Existing IPC / Frontend Method |
| :--- | :--- | :--- |
| **Context Working Memory** | `ContextMemoryBuilder`, `ContextMemory` (`memory.rs`) | `get_current_memory`, `get_context_memory` |
| **Task Understanding** | `TaskEvidenceCollector`, `TaskCandidateRanker` | `get_last_active_task` |
| **Temporal Segments** | `ContextActivitySegmenter` (`segments.rs`) | Internal segment pipeline |
| **Work Sequences** | `RecentWorkSequenceService` (`work_sequence.rs`) | `get_recent_work_sequence` |
| **Item Relationships** | `get_item_relationships`, `generate_relationships` | `get_item_relationships` |
| **Context Scoring** | `ContextEngine::evaluate_item`, `score_item` | `evaluate_item_context` |
| **Temporal Decay** | `apply_temporal_decay` (`temporal.rs`) | `get_temporal_decay_config` |
| **Attention Engine** | `evaluate_inbox_item_attention` (`attention.rs`) | `evaluate_inbox_item_attention` |
| **Database Operations** | `list_items`, `get_item`, `list_workspace_memberships` | Standard `db.ts` methods |

---

## 3. Reusable Architecture Matrix

The following diagram illustrates how the Workset layer integrates with existing services without creating parallel pipelines:

```
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                  EXISTING SERVICE REUSE MATRIX                                  │
├────────────────────────────────┬───────────────────────────────┬────────────────────────────────┤
│ Existing Service               │ Reusable Output                │ Workset Consumption            │
├────────────────────────────────┼───────────────────────────────┼────────────────────────────────┤
│ `ContextMemoryBuilder`         │ Active task, goal title,      │ Defines active workset header, │
│ (`context_engine/memory.rs`)   │ why-reasons, provenance,      │ task hypothesis, and primary   │
│                                │ activity state                │ task grounding                 │
├────────────────────────────────┼───────────────────────────────┼────────────────────────────────┤
│ `TaskCandidateRanker`          │ `RankedTaskCandidate`,        │ Identifies items directly      │
│ (`context_engine/task_*.rs`)   │ supporting item IDs           │ linked to the active task      │
├────────────────────────────────┼───────────────────────────────┼────────────────────────────────┤
│ `ContextActivitySegmenter`     │ Chronological bursts of       │ Extracts most recently active  │
│ (`context_engine/segments.rs`) │ items in active session       │ items and detour items         │
├────────────────────────────────┼───────────────────────────────┼────────────────────────────────┤
│ `item_relationships`           │ Relational edges (session,    │ Computes graph centrality and  │
│ (`context_engine/rel...rs`)    │ app, domain, semantic)        │ secondary reference relevance  │
├────────────────────────────────┼───────────────────────────────┼────────────────────────────────┤
│ `ContextEngine::score_item`    │ Calibrated workspace scores,  │ Identifies high-confidence     │
│ (`context_engine/scoring.rs`)  │ suggested memberships         │ unfiled inbox suggestions      │
├────────────────────────────────┼───────────────────────────────┼────────────────────────────────┤
│ `TemporalDecayConfig`          │ Exponential recency weighting │ Decays stale items in long-    │
│ (`context_engine/temporal.rs`) │ based on half-life parameters │ standing large workspaces      │
└────────────────────────────────┴───────────────────────────────┴────────────────────────────────┘
```

---

## 4. Missing Functionality & Architectural Gaps

While all underlying evidence is present, four specific architectural components are missing and must be built in Day 18:

1. **Unified Workset Data Model (`ContextWorkset`):**
   - No unified struct currently exists that represents the prioritized working state of a context (Active Task + Primary Items + Reference Items + Suggested Inbox Items + Detour History).
2. **Workset Classification & Tiering Engine (`ContextWorksetBuilder`):**
   - Need deterministic logic to partition all items associated with a context into distinct semantic tiers:
     - **Primary Working Items (`tier: primary`):** Items directly tied to current task, recently edited/viewed in current segment, high relational density.
     - **Secondary Reference Items (`tier: reference`):** Background context items with high semantic similarity or shared domain/app, but not active in current burst.
     - **Suggested Context Additions (`tier: suggested`):** Inbox items with high engine confidence ($\ge 0.70$) for this workspace, ready for one-click inclusion.
     - **Recent Detour Items (`tier: detour`):** Items touched during a brief interruption before returning to this context.
3. **Composite Workset Item Scoring (Consuming Existing Signals):**
   - A single, normalized $[0.0, 1.0]$ relevance score for ordering items within their tier, calculated by weighting:
     $$\text{WorksetScore} = 0.35 \times \text{TaskAlignment} + 0.30 \times \text{Recency} + 0.20 \times \text{RelationalCentrality} + 0.15 \times \text{SemanticSimilarity}$$
4. **Dedicated Tauri IPC Endpoint & Frontend Service:**
   - Need `get_context_workset(workspace_id: &str)` and `get_active_workset()` in Tauri IPC, exposed through `contextWorksetService` in `src/services/contextWorkset.ts`.

---

## 5. Data Duplication & Parallel-System Risks

> [!WARNING]
> **Anti-Patterns & Risks to Avoid in Day 18:**

1. **Risk 1: Creating a Second Context Classification Engine**
   - *Anti-Pattern:* Re-running raw vector search or TF-IDF heuristics inside the workset module to determine which workspace an item belongs to.
   - *Mitigation:* The Workset layer must **ONLY** consume existing `workspace_id`, `ContextMembership`, and `ContextEngine::score_item` evaluations.
2. **Risk 2: Creating a Second Task Extraction Pipeline**
   - *Anti-Pattern:* Parsing window titles or prompt strings in the workset module to guess what the user is doing.
   - *Mitigation:* The Workset layer must **ONLY** read the active task from `ContextMemory` or `TaskCandidateRanker`.
3. **Risk 3: Mutating Database State on Workset Generation**
   - *Anti-Pattern:* Automatically modifying item timestamps, moving items from Inbox to Workspaces, or inserting dummy rows during workset generation.
   - *Mitigation:* Workset generation is **strictly read-only and deterministic**. It reconstructs the working state in memory on demand.
4. **Risk 4: Duplicating Full Item Payloads in Memory**
   - *Anti-Pattern:* Returning massive base64 image strings or huge embedding vectors in workset responses.
   - *Mitigation:* Use lightweight `WorksetItem` references containing IDs, titles, types, timestamps, provenance, snippets, and scoring metadata.

---

## 6. Recommended Context Workset Architecture

### 6.1 Domain Models (`src-tauri/src/context_engine/workset.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorksetItemTier {
    /// Items directly supporting the active task or actively captured in current session
    Primary,
    /// Background reference materials and older context assets
    Reference,
    /// Unfiled inbox items confidently suggested for this context
    Suggested,
    /// Items captured or visited during a recent detour
    Detour,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorksetItem {
    pub id: String,
    pub title: Option<String>,
    pub item_type: String,
    pub content_snippet: String,
    pub tier: WorksetItemTier,
    pub relevance_score: f64,
    pub provenance: InformationProvenance,
    pub created_at: String,
    pub last_active_at: Option<String>,
    pub app_name: Option<String>,
    pub domain: Option<String>,
    pub reasons: Vec<String>,
    pub is_suggested_addition: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextWorkset {
    pub context_id: String,
    pub context_name: String,
    pub active_task: Option<MemoryTaskReference>,
    pub task_confidence_band: String,
    pub activity_state: ContextMemoryActivityState,
    pub primary_items: Vec<WorksetItem>,
    pub reference_items: Vec<WorksetItem>,
    pub suggested_items: Vec<WorksetItem>,
    pub detour_items: Vec<WorksetItem>,
    pub total_item_count: usize,
    pub last_active_at: Option<String>,
    pub workset_statement: String,
    pub explanation: MemoryExplanation,
    pub generated_at: String,
}
```

### 6.2 Workset Reconstruction Logic Flow

```
                                  WORKSET RECONSTRUCTION FLOW
                                  
    1. Input: workspace_id
             │
             ▼
    2. Fetch ContextMemory for workspace_id
       ├── Extracts: active_task, provenance, activity_state, explanation
       └── Extracts: supporting task item_ids
             │
             ▼
    3. Fetch Context Items & Suggested Memberships
       ├── Confirmed items: list_items(Some(workspace_id))
       └── Suggested items: list_workspace_memberships(workspace_id, Some("suggested"))
             │
             ▼
    4. Fetch Recent Activity Segments & Detour History
       ├── Segments from ContextActivitySegmenter (last 24h)
       └── Detour transitions (ContextReturn trigger items)
             │
             ▼
    5. Multi-Factor Item Scoring & Tier Partitioning
       ├── Primary: Task-supporting items + items in current active segment
       ├── Reference: Remaining confirmed items ranked by relational centrality & recency
       ├── Suggested: Inbox items with membership confidence >= 0.70
       └── Detour: Items captured during recent detour before return
             │
             ▼
    6. Assembly & Epistemic Statement Generation
       └── Returns clean ContextWorkset DTO (cached with 15s TTL)
```

---

## 7. Ranking Considerations

Item ranking within each workset tier must remain strictly multi-factor and deterministic:

$$\text{Relevance} = w_{\text{task}} \cdot S_{\text{task}} + w_{\text{recency}} \cdot S_{\text{recency}} + w_{\text{rel}} \cdot S_{\text{rel}} + w_{\text{sem}} \cdot S_{\text{sem}}$$

- **$S_{\text{task}}$ (Task Alignment):** $1.0$ if item ID is in `active_task.item_ids` or contains task subject/verb terms; $0.0$ otherwise.
- **$S_{\text{recency}}$ (Recency & Session Proximity):** Exponential decay $e^{-\lambda \Delta t}$ where $\Delta t$ is time since last activity in context.
- **$S_{\text{rel}}$ (Relational Centrality):** Normalized sum of relationship edge strengths connected to other items in the current active segment.
- **$S_{\text{sem}}$ (Semantic Similarity):** Cosine similarity to workspace embedding profile.

**Standard Weights:**
- $w_{\text{task}} = 0.35$
- $w_{\text{recency}} = 0.30$
- $w_{\text{rel}} = 0.20$
- $w_{\text{sem}} = 0.15$

---

## 8. UI Implications & UX Integration Points

1. **Workspace Header / TopBar:**
   - Display the active workset statement (e.g., *"Working on Hero animation with 4 active items"*).
   - Show a subtle "Resume Work" banner if returning after an interruption.
2. **Prioritized Working Canvas / Shelf:**
   - Display Primary Items prominently at the top of the workspace view.
   - Collapsible Reference section for older items to keep focus calm and clutter-free.
3. **Suggested Items Shelf:**
   - Display 1-click "Accept into Workset" cards for high-confidence inbox items.
4. **Context Switch / Return Indicators:**
   - When returning to a context, display recent detour items so the user can quickly see what was captured during the break.

---

## 9. Test Requirements

To ensure zero regressions and rock-solid architectural adherence, Day 18 tests must cover:
1. **Unit Tests (`workset.rs`):**
   - Workset generation with empty workspace (clean empty state, no panics).
   - Workset generation with active task (items matching task correctly assigned to `Primary` tier).
   - Tier partitioning isolation (confirmed vs suggested vs reference vs detour).
   - Proper calculation of `RelevanceScore` using existing relationship strengths and recency.
2. **Integration Tests (`workset_e2e_tests.rs`):**
   - Context switch -> detour capture -> context return -> workset reflects detour items.
   - Inbox item with high suggested confidence appears in `suggested_items`.
   - Invalidation of workset cache when new item is created, moved, or updated.
3. **Non-Duplication Verification:**
   - Verify that workset builder invokes `ContextMemory` and does not run independent candidate extraction.
   - Verify that workset builder makes zero modifying mutations to SQLite database tables.

---

## 10. Summary & Next Steps for Day 18

The ContextSplit codebase after Day 17 is in an ideal state for Day 18. All required telemetry, embeddings, relationships, transitions, task inferences, and memory models already exist.

**Day 18 Execution Roadmap:**
- **Step 2:** Implement `ContextWorkset` and `ContextWorksetBuilder` in `src-tauri/src/context_engine/workset.rs`.
- **Step 3:** Register Tauri IPC commands `get_context_workset` and `get_active_workset` in `src-tauri/src/lib.rs`.
- **Step 4:** Build frontend service `src/services/contextWorkset.ts` and update UI stores.
- **Step 5:** Integrate Workset UI components into Workspace view / TopBar.
- **Step 6:** Run full test suite and verify end-to-end working state reconstruction.
