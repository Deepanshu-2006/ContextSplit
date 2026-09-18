# Context Reconstruction Engine: Architecture Audit (Day 22 — Step 1)

## Executive Summary

ContextSplit has built 25 sophisticated context-aware subsystems spanning telemetry observation, signal scoring, evidence calibration, candidate discovery, task extraction, activity segmentation, transition tracking, working memory, long-lived important memories, workset prioritization, state freshness, resume readiness, point-in-time snapshots, and chronological timelines.

As we enter **Day 22 (Context Reconstruction Engine)**, our primary architectural objective is **orchestration, not reinvention**. We must not create another independent Context Engine. Instead, the Context Reconstruction Engine must act as the **definitive orchestrator** that queries authoritative sources, resolves contradictions, handles stale information, enforces privacy guardrails, and synthesizes a high-fidelity, actionable reconstruction of any workspace or context moment.

This audit provides a comprehensive analysis of the existing architecture, identifies authoritative data stores, delineates derived vs. persisted information, highlights duplicate logic and potential state contradictions, and specifies the exact pipeline and missing components for Day 22.

---

## 1. System Inventory: 25 Existing Subsystems

The ContextSplit codebase currently contains the following 25 components that contribute to context understanding and reconstruction:

| # | Subsystem | Module | Primary Responsibility | Reconstruction Role |
| :--- | :--- | :--- | :--- | :--- |
| 1 | **Context Engine** | `engine.rs`, `providers.rs` | Active window/URL/app telemetry capture | Ground-truth observational feed |
| 2 | **Context Evidence** | `evidence.rs`, `scoring.rs` | Multi-source evidence fusion & scoring | Signal accumulation & grounding |
| 3 | **Context Confidence** | `calibration.rs`, `normalization.rs` | Anti-jitter confidence calibration | Confidence scoring & tiering |
| 4 | **Context Discovery** | `discovery.rs` | Emergent cluster & candidate detection | Distinguishes workspaces from unformed clusters |
| 5 | **Context Membership** | `db::models`, `context_memberships` | Item-workspace associations & overrides | Authoritative association boundary |
| 6 | **Context Sessions** | `sessions`, `db::operations` | Temporal grouping of continuous work | Session duration & boundary framing |
| 7 | **Context Transitions** | `transitions.rs` | Switch/Return/Start/End detection | Departure and entry boundary identification |
| 8 | **Activity Segments** | `segments.rs` | Continuous focus bursts per workspace | Chronological activity segmentation |
| 9 | **Activity Summaries** | `summary.rs` | Aggregated summaries per time window | Session-level narrative summaries |
| 10 | **Task Candidates** | `task_extractor.rs` | Verb+Subject grounded task extraction | Identifies active & recent task intent |
| 11 | **Task Evidence** | `task_evidence.rs` | Signal collection for candidate tasks | Task confidence & grounding validation |
| 12 | **Task Transitions** | `task_transitions.rs` | Task start/switch/end lifecycle | Task progression & milestone markers |
| 13 | **Context Memory** | `memory.rs`, `memory_query.rs` | Unified working & episodic memory | Working memory state & activity sequences |
| 14 | **Context Workset** | `workset.rs` | Primary/Supporting/Reference item tiers | High-signal working asset reconstruction |
| 15 | **Context State** | `context_state.rs` | Freshness, meaningful activity, state value | Core "Where did I leave off?" state |
| 16 | **Resume Readiness** | `context_state.rs` | Readiness rating (Ready/Partial/Insufficient) | Actionable resumption evaluation |
| 17 | **Context Snapshots** | `snapshot.rs` | Point-in-time departure snapshots | Static departure checkpoint reconstruction |
| 18 | **Snapshot History** | `snapshot.rs` | Chronological snapshot timeline per context | Historical state progression |
| 19 | **Snapshot Comparison** | `snapshot.rs` | Delta between departure & current state | Absence diffing & change detection |
| 20 | **Important Memory** | `important_memory.rs` | Long-lived contextual memory items | Enduring context facts & recurring anchors |
| 21 | **Memory Importance** | `memory_importance.rs` | Importance scoring (Critical/High/Medium/Low) | Memory retention & hierarchy tiering |
| 22 | **Memory Decay** | `memory_decay.rs` | Half-life temporal decay & anchor protection | Stale state demotion & decay protection |
| 23 | **Memory Quality Guard**| `important_memory.rs`, `workset.rs`| Anti-noise, deduplication, generic filtering | Garbage suppression & quality control |
| 24 | **Context Overview** | `overview.rs` | Compact workspace metadata & stats | High-level workspace profile |
| 25 | **Context Timeline & Inbox** | `timeline.rs`, `inbox_state.rs` | Chronological activity feed & inbox triage | Temporal event stream & unassigned triage |

---

## 2. Answers to the 10 Core Architectural Questions

### Q1: How can a context currently be reconstructed?
Currently, context reconstruction is performed through fragmented entry points depending on the consumer:
1. **Live State**: `ContextStateBuilder::build()` queries `items`, `task_candidates`, and `context_transitions` to compute live `ContextStateValue`, `StateFreshness`, `MeaningfulActivity`, and `ResumeReadiness`.
2. **Point-in-Time Snapshot**: `ContextSnapshotBuilder::build()` assembles a static snapshot at a specific departure timestamp using `ContextWorksetBuilder` and `ContextMemoryBuilder`.
3. **Working Assets**: `ContextWorksetBuilder::build()` queries workspace items, scores them via temporal proximity, relationship graphs, and task affinity, and assigns tiers (`Primary`, `Supporting`, `Reference`, `Recent`).
4. **Long-Lived Anchors**: `ImportantMemoryService::query_memories()` retrieves user-confirmed facts, recurring references, and persistent milestone memories with decay adjustments.
5. **Historical Timeline**: `db::get_context_timeline()` merges transitions, item captures, sessions, and user actions into a grouped chronological event log.

*Limitation*: There is currently no single orchestrator that unifies live state, historical departure snapshots, long-lived memories, active worksets, and delta-diffs into a singular, cohesive **Reconstruction Narrative**.

### Q2: Which services already provide reconstruction capabilities?
- `ContextStateBuilder` (`context_state.rs`): Reconstructs current operational state and resume readiness.
- `ContextSnapshotBuilder` (`snapshot.rs`): Reconstructs point-in-time departure checkpoints.
- `ContextWorksetBuilder` (`workset.rs`): Reconstructs the 5–8 high-signal working documents/URLs.
- `ImportantMemoryService` (`important_memory.rs`): Reconstructs enduring contextual facts and recurring reference anchors.
- `ContextMemoryBuilder` (`memory.rs`): Reconstructs episodic activity sequences and task streaks.
- `TaskExtractor` & `TaskRankingService` (`task_extractor.rs`, `task_ranking.rs`): Reconstructs active and recent tasks.

### Q3: Which data sources are authoritative?
The authoritative sources of truth are strictly the persisted SQLite tables:
1. `workspaces`: Canonical workspace identity, lifecycle state (`active`, `dormant`, `archived`), and timestamps.
2. `items`: Canonical capture records (`id`, `workspace_id`, `item_type`, `content`, `title`, `metadata`, `created_at`).
3. `context_memberships`: Canonical item-to-workspace mapping and explicit user overrides (`confirmed`, `suggested`, `dismissed`).
4. `sessions`: Canonical recording sessions (`id`, `workspace_id`, `started_at`, `ended_at`, `status`).
5. `context_transitions`: Canonical context shift boundaries (`from_workspace_id`, `to_workspace_id`, `transition_type`, `detected_at`).
6. `task_candidates`: Grounded task representations with confidence scores and status (`active`, `completed`, `dismissed`).
7. `task_transitions`: Canonical task lifecycle events.
8. `important_memories`: Persisted long-lived memory items and user-pinned facts.
9. `item_relationships`: Persisted graph edges and co-occurrence weights.

### Q4: Which information is derived?
Derived information is computed on-demand from authoritative data and must not become a separate source of truth:
- `ContextStateValue` (`Active`, `RecentlyActive`, `Paused`, `Dormant`, `Uncertain`, `InsufficientEvidence`)
- `StateFreshness` (`Fresh`, `Recent`, `Stale`, `VeryStale`)
- `ResumeReadiness` & `ResumeReadinessScore` (`Ready`, `MostlyReady`, `Partial`, `Insufficient`)
- `WorksetItemRelevance` tiers (`Primary`, `Supporting`, `Reference`, `Recent`, `Related`)
- `CompositeRelevanceScore` & `EvidenceScore`
- `MemoryImportanceScore` & `DecayedImportanceScore`
- `SnapshotComparison` (deltas between departure and current state)
- `HumanReadableResumeStatement` and natural language summaries

### Q5: Which information is persisted?
- Workspaces, items, and explicit membership states.
- Session start/end intervals and activity boundaries.
- Context transitions and task transitions.
- Important contextual memories (with provenance, importance band, and pinning flags).
- Relationship graph edges (`item_a_id`, `item_b_id`, `relationship_type`, `strength`).
- Inbox item attention states and dismissal counters.

### Q6: Where does duplicate logic exist?
1. **Noise and Generic Filtering**: Duplicate checks for generic window titles (`"Untitled"`, `"New Tab"`, `"Google Chrome"`, `"Home"`) exist across `workset.rs`, `task_extractor.rs`, and `important_memory.rs`.
2. **Temporal Decay Calculations**: Exponential half-life decay math is independently implemented in `memory_decay.rs`, `context_state.rs`, `workset.rs`, and `scoring.rs`.
3. **Metadata Sanitization & App Parsing**: App name and web domain extraction from JSON strings is repeated across `timeline.rs`, `snapshot.rs`, `overview.rs`, and `memory.rs`.
4. **Epistemic Classification**: Logic determining whether an attribute is `Observed`, `Inferred`, or `Confirmed` is separately handled in `memory.rs`, `important_memory.rs`, and `snapshot.rs`.

*Recommendation for Day 22*: Centralize common utilities into shared modules (`context_engine::normalization`, `context_engine::memory_decay`, `context_engine::quality`).

### Q7: Where can contradictory state occur?
1. **Membership vs. Relationship Conflict**: An item is explicitly dismissed by the user from Workspace A, but strong graph relationships or semantic co-occurrence rank it as `Primary` in Workspace A's workset.
   *Resolution*: Explicit `context_memberships.status = 'dismissed'` must strictly hard-veto item inclusion.
2. **Task State Desynchronization**: A task candidate is marked `completed` in `task_candidates`, but `context_state` or `memory` still infers it as `ActiveTask` from recent capture co-occurrence.
   *Resolution*: Authoritative task status (`completed`, `dismissed`) must override observational task inference.
3. **Activity vs. Idle Contradiction**: Background sync items or automated captures timestamped recently make a context appear `Fresh`, while actual user foreground activity has been dormant for weeks.
   *Resolution*: Meaningful user activity must be gated on foreground window focus and explicit interaction, excluding background noise.
4. **Epistemic Priority Clash**: An algorithm infers context $X$ with 95% confidence, while user-pinned fact states context $Y$.
   *Resolution*: `Confirmed` user facts strictly supersede `Inferred` and `Observed` state ($P_{\text{confirmed}} > P_{\text{inferred}} > P_{\text{observed}}$).

### Q8: How is stale information currently handled?
- **Exponential Half-Life Decay**: `MemoryDecayService` applies exponential decay to memory importance based on hours elapsed since last activity.
- **Freshness Tiering**: `ContextState` classifies elapsed idle time into `Fresh` (< 2 hours), `Recent` (< 24 hours), `Stale` (< 7 days), and `VeryStale` (>= 7 days).
- **Decay Immunity for Anchors**: User-confirmed facts (`ConfirmedContextFact`) and recurring multi-session references have minimum decay floors ($S_{\text{floor}} \ge 0.50$).
- **Dormancy Transition**: Workspaces without activity for > 14 days are automatically treated as `Dormant`, downgrading live memory queries while preserving historical departure snapshots.

### Q9: How is confidence calculated?
Confidence is computed through multi-source signal fusion:
$$\text{Confidence} = \min\left(1.0, \sum_{i=1}^n w_i \cdot s_i - \text{Penalties}\right) \cdot (1 - \text{Ambiguity})$$
Where:
- $w_i \cdot s_i$: Weighted evidence signals (active app affinity, domain match, window title grounded keywords, graph proximity, temporal co-occurrence).
- $\text{Penalties}$: Historical rejection penalty ($\text{dismissal\_count} \times 0.15$) + Contradiction penalty (competing workspace overlap).
- $\text{Ambiguity}$: Ratio of second-highest candidate score to primary candidate score.
- Epistemic bands: `Confirmed` = 1.00, `Inferred` = [0.00, 0.95], `Observed` = [0.50, 0.90].

### Q10: How does user feedback affect reconstruction?
- **Confirmations**: User assigning an item, confirming a suggestion, or pinning a memory item sets provenance to `Confirmed`, locks confidence to 1.0, resets decay, and creates a permanent anchor.
- **Dismissals**: User dismissing a suggested workspace or candidate creates a suppression record, decrements candidate ranking, and applies an irreversible penalty to similar future inferences.
- **Edits & Manual Actions**: Manual workspace renaming, task completion, or item archiving immediately updates SQLite and invalidates cached reconstruction models.

---

## 3. Authoritative vs. Derived Data Matrix

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      PERSISTENT AUTHORITATIVE TIER                      │
│                                                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────────┐   │
│  │  workspaces  │  │    items     │  │     context_memberships      │   │
│  └──────┬───────┘  └──────┬───────┘  └──────────────┬───────────────┘   │
│         │                 │                         │                   │
│  ┌──────┴───────┐  ┌──────┴───────┐  ┌──────────────┴───────────────┐   │
│  │   sessions   │  │task_candidates│ │     context_transitions      │   │
│  └──────┬───────┘  └──────┬───────┘  └──────────────┬───────────────┘   │
│         │                 │                         │                   │
│  ┌──────┴─────────────────┴─────────────────────────┴───────────────┐   │
│  │        important_memories  •  item_relationships                 │   │
│  └──────────────────────────────────┬───────────────────────────────┘   │
└─────────────────────────────────────┼───────────────────────────────────┘
                                      │ Queries & Signals
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                  CONTEXT RECONSTRUCTION ORCHESTRATOR                    │
│                                                                         │
│  • Contradiction Resolution • Noise Veto • Epistemic Provenance Lock    │
│  • Multi-Tier Workset Rank  • State & Freshness Math • Narrative Synthesizer│
└─────────────────────────────────────┬───────────────────────────────────┘
                                      │ Projections
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                       DERIVED PROJECTION TIER                           │
│                                                                         │
│  ┌───────────────────────┐  ┌───────────────────────┐  ┌─────────────┐ │
│  │   ContextReconstruction │  │    ContextWorkset     │  │ ContextState│ │
│  │  (Unified Resume Model)│  │ (Primary/Ref Tiers)   │  │ & Freshness │ │
│  └───────────────────────┘  └───────────────────────┘  └─────────────┘ │
│  ┌───────────────────────┐  ┌───────────────────────┐  ┌─────────────┐ │
│  │    ResumeReadiness    │  │  SnapshotComparison   │  │TimelineFeed │ │
│  └───────────────────────┘  └───────────────────────┘  └─────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Context Reconstruction Pipeline

The **Context Reconstruction Engine** orchestrates a deterministic 6-stage pipeline to reconstruct any workspace:

```
                      RECONSTRUCTION PIPELINE
                      
  Stage 1: Identity & Authorization Gate
     │     • Validate workspace existence and lifecycle state
     │     • Load user-confirmed facts and pinned anchors
     ▼
  Stage 2: Temporal & Activity Boundary Resolution
     │     • Determine active/last session interval
     │     • Identify last ContextTransition (departure point)
     │     • Calculate idle duration and freshness tier
     ▼
  Stage 3: Working Asset Synthesis (Workset)
     │     • Fetch confirmed workspace items
     │     • Filter noise and enforce dismissal vetoes
     │     • Score items (recency, frequency, graph centrality)
     │     • Tier items into Primary, Supporting, Reference, Recent
     ▼
  Stage 4: Task & Intent Reconstruction
     │     • Fetch authoritative task candidates
     │     • Resolve active task vs completed/dismissed tasks
     │     • Attach grounded evidence (app, URL, window keywords)
     ▼
  Stage 5: State & Resume Readiness Evaluation
     │     • Compute ContextStateValue & StateFreshness
     │     • Assess ResumeReadiness (Ready, MostlyReady, Partial, Insufficient)
     │     • Generate actionable missing-context explanations
     ▼
  Stage 6: Unified Reconstruction Assembly
           • Synthesize human-readable resume statement
           • Assemble lightweight references (zero heavy payload duplication)
           • Attach epistemic disclaimer
           • Return ContextReconstruction
```

---

## 5. Missing Components for Day 22

To complete the Context Reconstruction Engine without duplicate systems, we must implement:

1. **Unified Reconstruction Data Model (`ContextReconstruction`)**:
   A canonical, serializable projection containing:
   - Workspace metadata (`id`, `name`, `lifecycle_state`).
   - Active task reconstruction (`task_title`, `action_verb`, `target_subject`, `confidence`, `provenance`).
   - Active workset (`primary_items`, `supporting_items`, `reference_items`, `recent_items`).
   - Operational state (`state_value`, `freshness`, `idle_duration_seconds`, `last_active_at`).
   - Resume readiness assessment (`rating`, `score`, `readiness_factors`, `unmet_criteria`).
   - Human-readable resume brief (e.g., *"Resuming work on 'API Integration' in TripWise. Last active 2 hours ago in VS Code with 3 key documents."*).
   - Departure snapshot comparison (what changed while away).
   - Standard epistemic disclaimer.

2. **Reconstruction Engine Service (`ContextReconstructionService`)**:
   - The central orchestrator that coordinates `ContextStateBuilder`, `ContextWorksetBuilder`, `ContextSnapshotBuilder`, `ImportantMemoryService`, and `TaskExtractor`.
   - Caching layer with atomic invalidation on item/workspace updates.

3. **Tauri IPC Command & Frontend Bridge**:
   - `get_workspace_reconstruction(workspace_id: String) -> Result<ContextReconstruction>`
   - `get_workspace_reconstruction_diff(workspace_id: String) -> Result<ReconstructionDelta>`

4. **Contradiction Resolution Guard**:
   - Explicit veto engine ensuring dismissed items, completed tasks, and deleted workspaces cannot leak into reconstruction outputs.

---

## 6. Contradiction & Stale State Handling

| Conflict Scenario | Detection Mechanism | Resolution Policy |
| :--- | :--- | :--- |
| **Dismissed item ranked high** | Check `context_memberships.status == 'dismissed'` | **Hard Veto**: Item completely excluded from workset. |
| **Completed task shown as active** | Check `task_candidates.status == 'completed'` | **Status Override**: Task marked completed; fallback to next active task or generic state. |
| **Background sync vs. User idle** | Compare event type (`item_captured` vs `window_focus`) | **Gated Recency**: Background sync does not reset user idle freshness timers. |
| **Inferred vs Confirmed Workspace** | Check `context_memberships.provenance` | **Epistemic Precedence**: User-confirmed workspace always overrides classifier suggestion. |
| **Stale task after long absence** | Elapsed time > 7 days (`VeryStale`) | **Degrade with Caution**: Active task flagged as *"Historical focus — verify if still active"*. |

---

## 7. Performance, Concurrency & Privacy Considerations

### Performance Targets
- **Reconstruction Latency**: $< 25\text{ ms}$ for cold query; $< 5\text{ ms}$ for cached query.
- **Payload Size**: Compact JSON $< 20\text{ KB}$ per workspace (lightweight IDs and snippets only, no full document blobs).
- **Query Optimization**: Indexed single-round SQL queries with `LIMIT` clauses on item and relationship joins.

### Concurrency & Thread Safety
- Read-only execution using SQLite shared read connections.
- Thread-safe caching with `Arc<RwLock<ReconstructionCache>>` and TTL-based invalidation (60 seconds or invalidation on write events).

### Privacy & Local-First Guardrails
- 100% on-device local execution; zero telemetry or network calls.
- Content snippets scrubbed of sensitive auth tokens and query parameters via `privacy::scrub_private_data`.
- Clear, standardized epistemic disclaimer attached to all reconstructed outputs.

---

## 8. Testing Strategy

The Day 22 implementation will be verified through a robust test suite:

1. **Unit Tests (`src-tauri/src/context_engine/reconstruction_tests.rs`)**:
   - Pipeline stage tests (identity gate, workset assembly, task resolution, readiness scoring).
   - Contradiction resolution tests (dismissed item veto, completed task override).
   - Freshness & decay tests (Fresh, Recent, Stale, VeryStale transitions).
   - Narrative synthesis tests (correct grammar, accurate elapsed times, no robotic system strings).

2. **Integration & E2E Tests**:
   - Multi-session workspace lifecycle (create -> work -> switch away -> return -> reconstruct).
   - Cache invalidation upon item capture or workspace rename.
   - IPC command validation through Tauri invocation mocks.

---

## Conclusion

The Context Reconstruction Engine represents the capstone orchestration layer of ContextSplit's contextual architecture. By unifying existing robust components (`ContextState`, `ContextWorkset`, `ContextMemory`, `ImportantMemory`, `TaskExtractor`, `ContextSnapshots`) behind a single deterministic orchestrator, we ensure **defensible, explainable, and instantaneous workspace resumption** with zero architectural bloat.
