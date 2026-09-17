# Context Memory Refinement Architecture Audit (Day 21 — Step 1)

## Executive Summary

As ContextSplit completes Day 20 (Context Snapshots), the platform has established robust infrastructure for real-time signal capture, multi-factor evidence scoring, task extraction with hysteresis, workset tiering, state freshness evaluation, and point-in-time snapshot reconstruction.

Day 21 introduces **Context Memory Refinement**, addressing the critical transition from **ephemeral short-term working state** to **stabilized, long-lived contextual memory**.

### Core Goal:
Determine what information in a workspace should become **LONG-LIVED contextual memory** (retained across days, weeks, and repeated context switches) versus what must remain **ephemeral working state** (decayed, pruned, or forgotten).

### Primary Architectural Invariant:
**Do not build a second independent intelligence system or separate database.**
Long-lived contextual memory must be a principled consolidation and refinement layer built strictly upon existing ContextSplit subsystems:
* Context Evidence & Scoring
* Context Workset & State
* Context Snapshots & Timeline
* Task Candidates & Transitions
* Context Memberships & User Feedback

---

## 1. Existing Memory Architecture

ContextSplit currently manages memory across several layered representations spanning SQLite persistence and in-memory derived projections:

```
┌────────────────────────────────────────────────────────────────────────┐
│                       PERSISTENCE LAYER (SQLite)                       │
├────────────────────────────────┬───────────────────────────────────────┤
│ Ground-Truth Entities          │ items, workspaces, sessions           │
│ Confirmed & Suggested Links    │ context_memberships                   │
│ Semantic Graph & Vectors       │ item_relationships, item_embeddings   │
│ Transition History             │ context_transitions                   │
│ Task Detection & History       │ task_candidates, task_evidence        │
│ Point-in-Time Snapshots        │ context_snapshots (bounded retention) │
│ Inbox Attention States         │ inbox_item_states                     │
└────────────────────────────────┴───────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                   DERIVED REASONING & WORKING MEMORY                   │
├────────────────────────────────┬───────────────────────────────────────┤
│ ContextEvidence / Scoring      │ Multi-factor weights, signal diversity│
│ ContextWorkset (`workset.rs`)  │ Primary, Supporting, Reference, Recent│
│ ContextState (`context_state`) │ Active, RecentlyActive, Paused, Dorm. │
│ ContextMemory (`memory.rs`)    │ Episodic milestones, task streaks     │
│ ContextSnapshot (`snapshot.rs`)│ Departure state, resume narrative     │
└────────────────────────────────┴───────────────────────────────────────┘
```

### Existing Memory Components:
1. **`ContextMemory` (`src-tauri/src/context_engine/memory.rs`)**:
   * Unified model representing active workspace identity, decoupled `current_task`, `recent_items`, `recent_tasks`, `activity_sequence` (milestones), and human-readable `memory_statement`.
   * Strict epistemic provenance tagging: `Observed`, `Inferred`, or `Confirmed`.
2. **`ContextWorkset` (`src-tauri/src/context_engine/workset.rs`)**:
   * Working set partitioner into 5 relevance tiers: `Primary`, `Supporting`, `Reference`, `Recent`, `Related`.
   * Noise suppression (`is_generic_noise`, `is_accidental_capture`), deduplication, and exponential temporal decay.
3. **`ContextState` & `ResumeReadiness` (`src-tauri/src/context_engine/context_state.rs`)**:
   * Conservative state classifier (`Active`, `RecentlyActive`, `Paused`, `Dormant`, `Uncertain`, `InsufficientEvidence`), `StateFreshness` (`Fresh`, `Recent`, `Stale`, `VeryStale`), and `ResumeReadiness` rating (`Ready`, `MostlyReady`, `Partial`, `Insufficient`).
4. **`ContextSnapshot` & History (`src-tauri/src/context_engine/snapshot.rs`, `snapshot_history.rs`)**:
   * Point-in-time capture of departure states when context transitions or session boundaries occur.
   * Compares snapshots across sessions to answer "What changed?" without fabricating transitions.

---

## 2. Reusable Systems & Capabilities

The long-lived memory refinement engine directly reuses existing infrastructure without requiring new database engines or duplicate storage:

| Existing System | Source File | Reusable Capability for Long-Lived Memory |
| :--- | :--- | :--- |
| **Epistemic Provenance** | `memory.rs` | `InformationProvenance::Confirmed`, `Inferred`, `Observed` guarantees transparency and prevents algorithmic hallucination. |
| **Workset Tiering** | `workset.rs` | `Reference` tier (stable project anchors) and `Primary` tier (active focus) cleanly separate durable assets from transient items. |
| **Relationship Graph** | `item_relationships` | Graph centrality and edge strength identify items that interconnect multiple tasks over time. |
| **Task Candidate Ranker** | `task_ranking.rs` | Anti-jitter hysteresis, streak tracking, and confidence bands distinguish persistent projects from momentary activities. |
| **Snapshot History** | `snapshot_history.rs`| Inter-session diffing tracks the historical evolution of tasks and worksets across days and weeks. |
| **Membership Provenance** | `context_memberships` | User-confirmed assignments provide immutable ground-truth anchors for long-lived memory. |
| **Inbox Attention Model** | `inbox_state.rs`, `attention.rs` | Prevents unfiled, low-attention captures from leaking into long-lived workspace memory. |
| **Temporal Decay Engine** | `temporal.rs`, `scoring.rs` | Parameterized exponential decay allows old transient activity to fade while frequently reinforced memory remains strong. |

---

## 3. Auditing the 8 Information Distinctions

A core challenge of contextual intelligence is preventing long-term memory from becoming a cluttered junk drawer. The system must distinctly categorize and handle 8 distinct classes of information:

| Information Category | Current Detection Mechanism | Characteristics & Lifespan | Role in Long-Lived Context Memory |
| :--- | :--- | :--- | :--- |
| **1. Recent Activity** | `sessions`, `context_transitions`, `ContextActivitySegment`, `MeaningfulActivity` | Highly dynamic (minutes to hours); rapidly decays. | **Ephemeral**. Provides live breadcrumbs for immediate resumption, but does not persist as long-term memory unless repeated. |
| **2. Important Contextual Info** | `ContextWorkset` (`Primary`, `Supporting`), high semantic affinity, high graph edge degree | Core to workspace mission; high relevance score (>0.75). | **Consolidated**. Forms the primary candidate pool for long-lived memory consolidation. |
| **3. Temporary Captures** | `inbox_item_states`, `ItemAttention::Low`, unfiled inbox items, low score (<0.35) | Short utility (scratchpads, fleeting lookups, quick pastes). | **Filtered Out**. Must NOT enter long-lived context memory unless confirmed by user or promoted via repeated multi-session access. |
| **4. Frequently Referenced Items** | Recurrent appearances across multiple `sessions` / `ContextActivitySegment`s, high in-degree in `item_relationships`, task streaks | Durable cross-session utility (project documentation, main GitHub repo, design specs). | **Permanent Anchors**. Promoted to long-lived `Reference` memory; immune to rapid decay. |
| **5. User-Confirmed Information** | `context_memberships` (`membership_type = 'confirmed'`, `source = 'user'`), user pins, confirmed tasks | Explicit user intent; 100% confidence ground truth. | **Immutable Memory Core**. Retained permanently until explicitly unassigned or deleted by the user. |
| **6. Inferred Information** | `task_candidates` (`status = 'detected'`), `ContextStateValue::Uncertain`, semantic suggestions | Probabilistic; requires validation; carries confidence band. | **Conditional Memory**. Retained only while corroborated by active evidence; decays if unreinforced; transparently labeled. |
| **7. Historical Information** | Bounded `context_snapshots` (pruned to 20), `sessions` log, `context_transitions` | Past states, previous session timestamps, completed tasks. | **Archival Memory**. Stored as compact bounded snapshots; queryable on demand, but isolated from active working set memory. |
| **8. Current/Recent Task Info** | `task_candidates` (`last_active_at`, `streak_count`), `TaskCandidateRanker` | Active goal (action verb + target subject); high freshness. | **Active Objective**. Drives current workset framing; transitions to historical memory once a new task stabilizes. |

---

## 4. Missing Concepts in the Current Architecture

While Day 17–20 created memory snapshots and working sets, three critical architectural links are missing to achieve **long-lived memory refinement**:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        MISSING MEMORY CONCEPTS                         │
├───────────────────────────────┬────────────────────────────────────────┤
│ 1. Memory Consolidation       │ No automated pipeline to promote       │
│    Pipeline                   │ high-frequency items/tasks from short- │
│                               │ term worksets into durable memory.     │
├───────────────────────────────┼────────────────────────────────────────┤
│ 2. Dual-Horizon Decay         │ Current decay treats all items         │
│    (Fast vs Slow)             │ identically. Core anchors need slow    │
│                               │ decay, while transient items need fast │
│                               │ decay.                                 │
├───────────────────────────────┼────────────────────────────────────────┤
│ 3. Explicit User Memory       │ User cannot explicitly "pin as key     │
│    Controls                   │ reference" or "forget from memory"     │
│                               │ without unassigning item entirely.     │
└───────────────────────────────┴────────────────────────────────────────┘
```

### Detailed Gap Analysis:
1. **Memory Consolidation Pipeline**:
   * Currently, memory is computed fresh on each query or saved as a discrete departure snapshot.
   * *Missing*: A consolidation mechanism that evaluates: "Has this item appeared across ≥ 3 distinct sessions over ≥ 2 days with high engagement?" If yes, promote to **Long-Lived Context Anchor**.
2. **Dual-Horizon Temporal Model**:
   * Current decay applies uniform half-life across all unpinned items.
   * *Missing*: Epistemic-aware decay:
     * *Transient working items* (browser tabs, recent snippets): Fast half-life (1–3 days).
     * *Consolidated reference anchors* (primary repository, design tokens, project briefs): Slow half-life (30–90 days).
     * *User-confirmed items*: Infinite half-life (zero decay).
3. **User Memory Curation Controls**:
   * Current controls allow assigning items to workspaces or deleting them.
   * *Missing*: Context-specific memory commands:
     * "Pin as key context memory" (force long-lived retention).
     * "Remove from context memory" (suppress from resume state without deleting the underlying item).
     * "Clear context memory history" (reset snapshots without wiping items).

---

## 5. Duplication Risks & Mitigation Rules

To strictly obey the requirement to **avoid creating a second independent intelligence system**, the following anti-duplication rules are enforced:

| Risk Area | Threat | Mitigation Rule |
| :--- | :--- | :--- |
| **Item Storage** | Duplicating full item bodies or text payloads in memory tables. | **References Only**: Memory models store only `item_id`, `title`, `item_type`, `provenance`, and normalized metadata. Full content is fetched from `items` table via foreign keys. |
| **Task Definitions** | Duplicating task parsing and evidence extraction in memory. | **Reuse `task_candidates`**: Memory references point to existing `task_candidate_id` and normalized title. |
| **Vector Storage** | Creating a second vector store for "long-term memory". | **Reuse `item_embeddings` & `workspace_embeddings`**: Use existing embeddings table and similarity search. |
| **State Duplication** | Storing redundant state calculations in multiple background caches. | **Single Source of Truth**: `ContextState` and `ContextWorkset` remain the authoritative projectors; long-lived memory stores consolidated indices, not divergent state trees. |

---

## 6. Retention Concerns & Bounding Strategy

Unbounded memory growth degrades performance, bloats SQLite databases, and violates user privacy.

### Retention Rules:
1. **Snapshot Retention**:
   * Maintained at maximum **20 snapshots per workspace** (FIFO auto-pruning implemented in Day 20 Migration v23).
   * Snapshot pruning never deletes underlying items, signals, or task records.
2. **Long-Lived Memory Anchors**:
   * Hard cap of **top 10 consolidated memory anchors** per workspace (most frequently referenced and user-confirmed items).
   * Dynamic ranking based on: `MemoryScore = (UserConfirmed ? 1.0 : 0.6) * FrequencyWeight * GraphCentrality * RecencyFactor`.
3. **Session & Transition History**:
   * Bounded query windows (e.g. last 30 days for overview timelines).
   * Older records archived or indexed with composite timestamps for `O(log N)` retrieval.

---

## 7. Memory Importance Rules

To objectively compute what deserves long-lived memory status, the engine evaluates four quantitative gates:

```
                    ┌───────────────────────────────┐
                    │       Captured Item/Task      │
                    └───────────────┬───────────────┘
                                    │
                                    ▼
                     [ Gate 1: Provenance Check ]
                    /                           \
         User Confirmed?                     Inferred / Observed?
               │                                       │
               ▼                                       ▼
       [ Permanent Anchor ]                [ Gate 2: Noise Filter ]
        • Weight = 1.0                      • Exclude generic apps/domains
        • Zero decay                        • Exclude unassigned inbox items
                                                       │
                                                       ▼
                                            [ Gate 3: Engagement Gate ]
                                            • Cross-session count ≥ 2?
                                            • Workset primary tier ≥ 2x?
                                            • Graph edge degree ≥ 3?
                                            /                           \
                                          YES                            NO
                                           │                              │
                                           ▼                              ▼
                             [ Long-Lived Memory Anchor ]       [ Ephemeral Workset ]
                             • 60-day slow decay floor          • 3-day fast decay
                             • Included in Resume state         • Discarded after pause
```

### Quantitative Memory Scoring Formula:
$$\text{MemoryImportance}(i) = w_p \cdot P(i) + w_f \cdot F(i) + w_g \cdot G(i) + w_r \cdot R(i)$$
Where:
* $P(i)$ = Provenance multiplier (1.0 for Confirmed, 0.7 for Inferred with high confidence, 0.4 for Observed).
* $F(i)$ = Multi-session recurrence frequency factor $[0.0, 1.0]$.
* $G(i)$ = Graph centrality in `item_relationships` $[0.0, 1.0]$.
* $R(i)$ = Temporal decay score $[0.2, 1.0]$ based on last active timestamp.
* $w_p = 0.35, w_f = 0.30, w_g = 0.20, w_r = 0.15$.

---

## 8. User Control & Privacy Requirements

Long-lived memory must operate with complete user transparency and total user agency:

### User Controls:
1. **Explicit Curation**:
   * **Pin / Unpin Memory**: User can mark any item as a permanent workspace anchor.
   * **Forget Item**: User can remove an item from workspace memory without deleting the item from their system.
   * **Reset Workspace Memory**: User can clear all inferred memory, snapshots, and activity sequences for a workspace.
2. **Epistemic Disclosure**:
   * Every memory item in the UI must clearly display its provenance badge:
     * 🟢 **Confirmed** ("You added this to workspace")
     * 🔵 **Frequent Anchor** ("Used across 4 sessions")
     * 🟣 **Inferred** ("Suggested by ContextSplit · 85% confidence")

### Privacy & Sensitive Information Scrubbing:
1. **Zero Secret Retention**:
   * Strict sanitization of tokens, passwords, API keys, basic auth in URLs, and credential patterns before persisting memory statements.
2. **Local-Only Processing**:
   * Memory consolidation, graph traversal, and snapshot comparisons run 100% locally in Rust/SQLite on the user's machine.
3. **Telemetry & Capture Isolation**:
   * Respects `capture_paused` and `enable_context_snapshots` settings unconditionally.

---

## 9. Recommended Architecture for Day 21

The refined contextual memory architecture builds cleanly onto the existing codebase:

```
src-tauri/src/context_engine/
├── memory.rs                  # Core ContextMemory, Epistemic Provenance, References
├── memory_consolidation.rs    # [NEW] Multi-session recurrence & anchor promotion engine
├── memory_query.rs            # Fast unified query interface for UI & Overview
├── workset.rs                 # 5-tier workset partitioner (Primary/Supporting/Reference/Recent)
├── context_state.rs           # State value, freshness, and resume readiness
├── snapshot.rs                # Point-in-time departure snapshots & persistence
├── snapshot_history.rs        # Inter-session diffing & "What changed?" engine
└── memory_e2e_tests.rs        # Multi-session memory integration suite
```

### Consolidation Workflow:
1. **Observation**: Raw signals generate ephemeral workset items during an active session.
2. **Departure**: Context switch triggers snapshot generation (`snapshot.rs`).
3. **Consolidation**: Background memory consolidation analyzes cross-session recurrence, upgrades items meeting Gate 3 into the `Reference` tier with slow decay, and indexes them as **Long-Lived Workspace Anchors**.
4. **Resumption**: When the user returns to a workspace, the Context Overview seamlessly presents:
   * **Where you left off** (last active task + recent primary items from snapshot).
   * **Key Workspace Anchors** (consolidated long-lived references).
   * **What changed** (delta between departure and current state).

---

## 10. Test Requirements

To guarantee rock-solid stability and zero regressions across the 659 existing tests, the Day 21 implementation must fulfill four testing dimensions:

1. **Unit Tests (`memory_consolidation_tests.rs`)**:
   * Test multi-session recurrence calculation across simulated sessions.
   * Test dual-horizon decay (verifying reference anchors resist decay while ephemeral items decay quickly).
   * Test provenance propagation (guaranteeing Confirmed items are never demoted).
2. **Adversarial & Edge Case Tests**:
   * *Rapid task thrashing*: Verify rapid context switching does not spam memory anchors.
   * *Transient capture surge*: 50 scratchpad captures in inbox do not leak into workspace long-term memory.
   * *Accidental noise*: Ephemeral lookups (e.g. checking weather or quick calculation) are filtered out.
3. **User Control & Privacy Tests**:
   * Test "Forget Item from Memory" ensures immediate exclusion from resume state.
   * Test "Pin Item" guarantees permanent long-term retention.
   * Test secret scrubbing removes passwords and auth tokens from memory references.
4. **End-to-End Multi-Day Simulation Tests**:
   * Simulate a multi-day workspace lifecycle (Day 1: Setup -> Day 2: Development -> Day 5: Return after weekend).
   * Verify long-lived memory accurately reconstructs the project anchors, recent task, and clean resume statement without hallucination.

---

## Conclusion & Next Steps

This audit establishes the rigorous architectural foundation for Day 21. By consolidating existing systems (Evidence, Workset, State, Snapshots, Task Engine, Memberships) rather than creating a redundant second intelligence layer, ContextSplit achieves deep, reliable, and privacy-respecting long-lived contextual memory.
