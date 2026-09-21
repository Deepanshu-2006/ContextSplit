# Context Lifecycle & Session Intelligence: Architecture Audit (Day 24 — Step 1)

## Executive Summary

As ContextSplit advances into **Day 24 (Context Lifecycle & Session Intelligence)**, our primary architectural objective is to establish an explicit, robust, and mathematically grounded **lifecycle state machine for contexts, workspaces, and work sessions**.

Our foundational design axiom for Day 24 is:
> **A context is a living work state, not a static folder.**
> Contexts naturally flow through distinct behavioral phases: initialization, active focus, brief interruptions, intentional pauses, extended dormancy, and eventual resumption or archiving. Conflating global OS telemetry sessions with context-specific focus sessions, or confusing paused work with abandoned projects, leads to degraded resume readiness and context fragmentation.

Over Days 1–23, ContextSplit built 25 sophisticated subsystems—including multi-signal evidence collection, 5-tier confidence calibration, hysteresis-based transition detection, active workset tiering, conservative state evaluation, and anti-contamination quality guards. However, context lifecycle management and session intelligence remain split across low-level database operations, heuristic time gaps, and ad-hoc state derivation.

This audit provides a comprehensive end-to-end evaluation of how sessions and contexts are born, maintained, paused, dormantized, resumed, and closed; systematically answers the 10 core architectural questions; inventories existing reusable subsystems; pinpoints missing lifecycle concepts; outlines failure modes; and defines a rigorous testing strategy for Day 24.

---

## 1. Answers to the 10 Core Architectural Questions

### Q1: How sessions are currently created
- **Database Layer**: In `src-tauri/src/db/operations.rs`, `create_session(conn)` generates a UUID v4, records `started_at = now`, `last_activity_at = now`, `ended_at = None`, and `status = "active"` in the `sessions` table.
- **Capture Engine Interaction**: In `src-tauri/src/context_engine/engine.rs` (`get_or_update_active_session`), every incoming telemetry event checks `operations::get_latest_session(conn)`. If no session exists or the latest session is inactive/expired, a new session is created.
- **Architectural Scope**: The current `sessions` table represents **global OS telemetry capture sessions**, not workspace-specific focus sessions. A single capture session contains activity across multiple applications, windows, and workspaces.

### Q2: How sessions end
- **Inactivity Timeout**: In `ContextEngine::get_or_update_active_session`, if `diff = now - last_activity_at >= inactivity_threshold_secs` (default: 1800s / 30 minutes), the active session is marked ended via `operations::end_session(conn, id)` (`ended_at = now`, `status = "ended"`), and a fresh session is spawned.
- **Explicit Application Teardown**: When the ContextSplit desktop daemon shuts down or telemetry monitoring is stopped, active sessions can be formally closed.
- **Segment-Level Boundary**: In `src-tauri/src/context_engine/segments.rs`, `ContextActivitySegment` closes dynamically when a context transition or temporal boundary is encountered.

### Q3: How context activity starts
- **Telemetry Ingestion**: An OS event (active app, window title, browser URL, clipboard) is processed by `CapturePipeline`.
- **Signal & Item Emission**: `CapturePipeline` creates an `Item` and emits `ContextSignals` (`capture_event`, `active_app`, `window_title`, `browser_domain`).
- **Transition Detection**: `ContextTransitionDetector` in `transitions.rs` evaluates sliding telemetry windows:
  - `ContextStart`: Triggered when activity begins in a context while no previous context was actively tracked.
  - `ContextSwitch`: Triggered when focus decisively shifts from Context A to Context B (requiring consecutive observations $\ge 2$, margin $\ge 0.15$, or fast-track confidence $\ge 0.85$).
  - `ContextReturn`: Triggered when focus returns to a previously active context within 24 hours.
- **Workspace Timestamp Touch**: `operations::touch_workspace_activity(conn, id)` updates `workspaces.last_activity_at = now` and automatically promotes `dormant` workspaces back to `active`.

### Q4: How context activity stops
- **Context Departure (`ContextSwitch`)**: When the user switches focus to another workspace, `ContextTransitionDetector` emits a transition with `from_context_id = Some(A)`. Workspace A is classified into `ContextStateValue::Paused`.
- **Temporal Inactivity**: If no events are captured for Workspace A:
  - $\le 15\text{ min}$: Remains `ContextStateValue::Active`.
  - $15\text{ min} - 24\text{ hours}$: Transitions to `ContextStateValue::RecentlyActive`.
  - $24\text{ hours} - 7\text{ days}$: Transitions to `ContextStateValue::Paused`.
  - $> 7\text{ days}$: Transitions to `ContextStateValue::Dormant`.
- **Task End**: In `task_transitions.rs`, a `TaskTransitionType::TaskEnd` moves the active task to completed/paused, moving `ContextState` to `Paused`.
- **Segment Finalization**: `ActiveSegmentState` in `segments.rs` seals its duration and item list upon transition or timeout.

### Q5: How inactivity is represented
Inactivity is currently modeled across four distinct dimensions:
1. **Telemetry Capture Inactivity**: `inactivity_threshold_secs` (1800s) governs `Session` rollover.
2. **State Freshness (`StateFreshness`)**:
   - `Fresh`: $\le 2\text{ hours}$
   - `Recent`: $2 - 24\text{ hours}$
   - `Stale`: $24\text{ hours} - 7\text{ days}$
   - `VeryStale`: $> 7\text{ days}$
3. **Working State (`ContextStateValue`)**:
   - `Active` ($\le 15\text{ min}$ with active corroboration)
   - `RecentlyActive` ($15\text{ min} - 24\text{ hours}$)
   - `Paused` ($24\text{ hours} - 7\text{ days}$ or explicit switch away)
   - `Dormant` ($> 7\text{ days}$)
4. **Workspace Lifecycle (`workspaces.lifecycle_state`)**: Evaluated via `evaluate_workspace_lifecycle` with an inactivity threshold of 30 days before marking a workspace `"dormant"`.

### Q6: How context switching is detected
- **Hysteresis & Anti-Bounce**: In `transitions.rs`, `ContextTransitionDetector` evaluates incoming evidence against `TransitionStabilityConfig`:
  - Requires `min_consecutive_evidence = 2` observations in the candidate context before confirming a switch (to prevent single-window glances from fragmenting history).
  - Requires `meaningful_score_difference = 0.15` margin over competing candidate workspaces.
  - Requires `min_evidence_agreement_count = 2` agreeing evidence items.
- **Fast-Track Gating**: Transitions can fast-track on item 1 only if confidence $\ge 0.85$ or score $\ge 0.80$ AND corroborated by structural anchors (file path, git repo, project directory, explicit user action).
- **Emission**: Yields `ContextTransition` with `transition_type = ContextSwitch`, `from_context_id`, `to_context_id`, `confidence`, and `detected_at`.

### Q7: How returning to a context is detected
- **Temporal Horizon Check**: When a candidate context matches a previously active context, `transitions.rs` verifies if the departure occurred within `max_return_gap_seconds` (default: 86,400s / 24 hours).
- **Confidence Requirement**: Requires `min_return_confidence = 0.50`.
- **Transition Emission**: Emits `ContextTransitionType::ContextReturn`.
- **Workset & State Integration**:
  - `workset.rs` classifies the return as `WorksetState::DetourReturn`, boosting relevant working items and suppressing temporary detour noise.
  - `context_state.rs` incorporates return signals into resume explanations: *"Returned to 'X' context after temporary detour."*

### Q8: How paused work differs from abandoned work
- **Current State**:
  - **Paused Work**: A context switched away from within 24 hours to 7 days, or where a task ended cleanly. The workset remains intact, `ResumeReadiness` remains `Ready` or `MostlyReady`, and active tasks are preserved.
  - **Abandoned Work**: Currently, ContextSplit relies on time thresholds (`Dormant` $> 7$ days in state, $> 30$ days in workspace table).
- **Existing Limitation**: The system lacks semantic indicators of abandonment (e.g., failed tasks, dangling scratchpads, broken builds, or permanently closed project tabs) versus intentional long-term pauses.

### Q9: How stale contexts are currently represented
- **Scoring & Candidate Suppression**: In `scoring.rs`, `WorkspaceScoringContext::with_lifecycle_state` applies strict scoring penalties:
  - `active`: 1.0 (Full weight)
  - `dormant`: 0.50 (50% score penalty to prevent dormant contexts from stealing current activity)
  - `archived`: 0.0 (Zero score; completely suppressed from automated matching)
- **Memory Decay**: In `memory_decay.rs` and `important_memory.rs`, older memories experience continuous exponential half-life decay, decreasing confidence unless reinforced.
- **UI Freshness**: `StateFreshness::display_wording` renders defensive wording: *"Last active yesterday"*, *"Last active 5 days ago"*.

### Q10: How Context State currently interacts with sessions
- **Indirect Association**: `ContextState` does not own a direct foreign key to `sessions`. Instead, `ContextStateBuilder` reconstructs state by querying `items`, `context_activity_segments`, and `context_transitions`.
- **Segment Bridging**: `ContextActivitySegment` holds a list of `session_ids: Vec<String>`, bridging raw capture sessions with higher-level contextual work periods.
- **Reconstruction Hierarchy**:
  ```
  Global Capture Sessions (telemetry stream)
             │
             ▼
  Context Activity Segments (continuous workspace focus)
             │
             ▼
  Context Workset + Important Memory (active files/docs/tasks)
             │
             ▼
  Context State (holistic answer: "Where did I leave this work?")
             │
             ▼
  Context Reconstruction (unified resume payload)
  ```

---

## 2. Current Lifecycle Architecture

### 2.1 Session Lifecycle vs. Context Lifecycle
A critical distinction in ContextSplit is the decoupling of **Session Lifecycle** (telemetry recording) from **Context Lifecycle** (human workspace focus):

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         GLOBAL OS TELEMETRY STREAM                          │
│                                                                             │
│  ┌─────────────────────── Session 1 ───────────────────────┐   ┌── S2 ──┐   │
│  │ 09:00                                             10:30 │   │ 11:00  │   │
│  └─────────────────────────────────────────────────────────┘   └────────┘   │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                      CONTEXT WORKSPACE FOCUS SEGMENTS                       │
│                                                                             │
│  ┌──────── Workspace A ────────┐  ┌─── Detour ───┐  ┌──── Workspace A ────┐ │
│  │ 09:00                 09:45 │  │ 09:45  09:55 │  │ 09:55         10:30 │ │
│  │ Segment 1 (PayX API)        │  │ (HN / Twitter│  │ Segment 2 (PayX Doc)│ │
│  └─────────────────────────────┘  └──────────────┘  └─────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Workspace Lifecycle State Machine (`workspaces.lifecycle_state`)

```
                ┌────────────────────────────────┐
                │             Active             │◄───────────────┐
                └───────┬────────────────┬───────┘                │
                        │                │                        │
       Inactivity > 30d │                │ User Archive           │ User Touch /
     (or manual action) │                │                        │ Unarchive
                        ▼                ▼                        │
                ┌───────────────┐   ┌───────────────┐             │
                │    Dormant    │   │   Archived    │─────────────┘
                └───────┬───────┘   └───────────────┘
                        │                   ▲
                        │ User Archive      │
                        └───────────────────┘
```

### 2.3 Working State Transition Matrix (`ContextStateValue`)

| Current State | Event / Signal | Elapsed Time | Next State | Confidence |
| :--- | :--- | :--- | :--- | :--- |
| `InsufficientEvidence` | First item/signal captured | $< 15\text{m}$ | `RecentlyActive` / `Active` | $0.50 - 0.80$ |
| `Active` | Continuous captures in context | $< 15\text{m}$ | `Active` | $0.85 - 0.95$ |
| `Active` | Inactivity (no telemetry) | $15\text{m} - 24\text{h}$ | `RecentlyActive` | $0.65 - 0.70$ |
| `Active` | Focus switch to Context B | Immediate | `Paused` | $0.75 - 0.95$ |
| `RecentlyActive` | Switch to Context B | Immediate | `Paused` | $0.75 - 0.95$ |
| `RecentlyActive` | Prolonged inactivity | $24\text{h} - 7\text{d}$ | `Paused` | $0.60 - 0.70$ |
| `Paused` | Extended inactivity | $> 7\text{d}$ | `Dormant` | $0.70 - 0.85$ |
| `Paused` | Return to context ($< 24\text{h}$) | Immediate | `Active` (via `ContextReturn`) | $0.85 - 0.95$ |
| `Dormant` | Reopening / new capture | Immediate | `Active` / `RecentlyActive` | $0.70 - 0.85$ |
| Any State | Conflicting / ambiguous signals | Any | `Uncertain` | $0.30 - 0.40$ |

---

## 3. Inventory of Existing Reusable Systems

Day 24 will build directly on top of the established, battle-tested subsystems without reinventing data models or scoring mechanisms:

1. **`src-tauri/src/db/operations.rs` & `models.rs`**:
   - `Session`, `Workspace`, `Item`, `ContextSignal`, `ContextMembership`.
   - `update_workspace_lifecycle_state`, `touch_workspace_activity`, `evaluate_workspace_lifecycle`.
2. **`src-tauri/src/context_engine/transitions.rs`**:
   - `ContextTransitionDetector`, `TransitionStabilityConfig`, `TransitionEvidenceType`.
   - Hysteresis tracking, streak calculation, return detection, margin gating.
3. **`src-tauri/src/context_engine/segments.rs`**:
   - `ContextActivitySegmenter`, `ContextActivitySegment`, `ActiveSegmentState`.
   - Continuous duration accumulation, multi-session aggregation, item reference packaging.
4. **`src-tauri/src/context_engine/context_state.rs`**:
   - `ContextState`, `ContextStateValue`, `StateFreshness`, `MeaningfulActivityResolver`, `ResumeReadiness`.
   - Progressive disclosure explanations and defensible time-relative phrasing.
5. **`src-tauri/src/context_engine/workset.rs`**:
   - `ContextWorkset`, `WorksetState` (`Active`, `DetourReturn`, `Historical`, `Emerging`).
   - 3-tier workset categorization (`PrimaryWorkingSet`, `SupportingContext`, `ReferenceMaterial`).
6. **`src-tauri/src/context_engine/snapshot.rs`**:
   - `ContextSnapshot`, `ContextSnapshotDiff`, point-in-time state preservation, differential comparison.
7. **`src-tauri/src/context_engine/quality_guard.rs` & `contamination.rs`**:
   - Anti-contamination filters, noise suppression, delayed evidence resolution.
8. **`src-tauri/src/context_engine/reconstruction.rs`**:
   - Unified orchestration pipeline aggregating state, snapshots, memory, and workset into a single resume payload.

---

## 4. Missing Lifecycle & Session Concepts

While the existing subsystems cover point-in-time classification, several critical lifecycle concepts are missing or underspecified:

### 4.1 Native Context Focus Sessions ("Context Session")
- **The Gap**: The database only contains global capture `sessions`. There is no first-class `ContextSession` entity representing a dedicated block of work on a specific workspace.
- **Requirement**: Derive or track context-level focus sessions with explicit start time, end time, total active duration (excluding idle detours), primary tasks, and associated items.

### 4.2 Explicit Pause vs. Abandonment vs. Completion Heuristics
- **The Gap**: Currently, any context inactive for $> 24$ hours is labeled `Paused`, and after 7 days becomes `Dormant`. There is no distinction between:
  1. *Intentional Pause*: Work left in a clean, resumable state with pending tasks.
  2. *Completed Work*: All tasks finished; goal achieved.
  3. *Abandoned / Stalled Work*: Abruptly halted mid-task, high error signals, no return for weeks.
- **Requirement**: Multi-factor pause classification based on task completion status, git commit events, and resume intent markers.

### 4.3 Intelligent Dormancy & Cold-Start Reopening
- **The Gap**: When a dormant context (> 30 days old) is reopened, current code simply updates `lifecycle_state = 'active'`. It does not perform **cold-start context warming** (e.g. restoring high-relevance architectural docs while archiving stale ephemeral tabs).
- **Requirement**: A dedicated **Context Reopening / Awakening Engine** that prepares a historical workspace for instant resume without tab clutter.

### 4.4 Micro-Breaks vs. Macro-Detours vs. Context Switches
- **The Gap**: A 2-minute quick email check is currently treated either as an ambiguous transition or a detour, but there is no formalized classification separating:
  - *Micro-Break*: $< 3\text{ minutes}$ off-task (e.g. Spotify, lock screen).
  - *Work Detour*: $3 - 15\text{ minutes}$ auxiliary task (e.g. Slack response, reading doc for another project).
  - *Full Context Switch*: Sustained $\ge 15\text{ minutes}$ deep work on another workspace.

---

## 5. Detailed Lifecycle Mechanics

### 5.1 Pause Detection Engine
Pause detection must distinguish between transient pauses and deep context switches:

```
[Inactivity / Switch Detected]
               │
               ▼
   Is it an explicit switch to Context B?
     ├── YES ──► Emit ContextSwitch -> Mark Context A as Paused (switched_away)
     └── NO  ──► Check elapsed idle time
                   ├── < 15 min: Retain Active
                   ├── 15m - 24h: Transition to RecentlyActive
                   └── > 24h: Transition to Paused (idle_timeout)
```

### 5.2 Resume Detection Engine
When telemetry resumes in a previously inactive workspace:

```
[New Activity in Workspace A]
               │
               ▼
   What was Workspace A's previous state?
     ├── Paused (< 24h)  ──► Fast-track ContextReturn; restore Warm Workset
     ├── Paused (> 24h)  ──► ContextReturn; present Resume Summary & Last State
     ├── Dormant (> 7d)  ──► Context Awakening; load Evergreen Memory, prune stale tabs
     └── Archived        ──► Require explicit user unarchive confirmation
```

### 5.3 Dormancy & Decay Rules
- **Graduated Decay Schedule**:
  - $0 - 24\text{ hours}$: $0\%$ decay (Immediate Warm State).
  - $1 - 7\text{ days}$: $15\%$ memory discount; workset shifts to `Historical`.
  - $7 - 30\text{ days}$: State = `Dormant`; candidate matching score discounted by $50\%$.
  - $> 30\text{ days}$: Workspace `lifecycle_state = 'dormant'`; automated suggestions gated.
- **Archival Invariant**: **ContextSplit NEVER automatically archives user contexts.** Archiving is strictly an explicit user action.

---

## 6. Failure Modes & Edge Cases

| Failure Mode | Root Cause | Systemic Risk | Day 24 Safeguard |
| :--- | :--- | :--- | :--- |
| **Rapid Flip-Flop** | Switching between IDE and browser every 10 seconds | Fragmented micro-sessions, noisy timeline | Hysteresis streak buffer ($\ge 2$ observations) & minimum segment duration |
| **Detour Contamination** | 5-minute personal banking during coding session | Banking items pollute project workset | Anti-contamination quality guard & detour suppression |
| **Zombie Active State** | Laptop left open overnight with IDE in foreground | False claim: "Worked 14 hours on ContextSplit" | Telemetry idle cap (1800s); state caps active at 15m without events |
| **False Dormancy Awakening** | Single background ping to a dormant project repo | Unwanted awakening of old workspace | Require corroborated structural signal or explicit user focus to awaken |
| **Dangling Abandoned Tasks** | User leaves unfinished task and never returns | Task remains "Active" forever in state | Auto-pause tasks when context transitions to `Paused`/`Dormant` |
| **Session Bleed Overlap** | Global capture session spans across 3 different workspaces | Items from Context A linked to Context B session | Context-specific session segmentation via `ContextActivitySegment` |

---

## 7. Testing Strategy for Day 24

To ensure zero regressions across the existing test suite (including the 659+ unit tests, adversarial benchmarks, and offline accuracy evaluators), Day 24 implementation will follow a 5-part verification strategy:

1. **Unit Tests (`lifecycle_tests.rs`)**:
   - Test session creation, heartbeat touches, inactivity expiration, and explicit teardown.
   - Verify state transitions across `Active`, `RecentlyActive`, `Paused`, `Dormant`, and `Uncertain`.
   - Verify `StateFreshness` calculation and relative string formatting.
2. **Context Return & Resume Tests (`resume_tests.rs`)**:
   - Verify short-term ($< 24\text{h}$) vs long-term ($> 7\text{d}$) return transitions.
   - Test resume readiness score stability across warm and cold contexts.
3. **Detour & Inactivity Simulation (`session_intelligence_tests.rs`)**:
   - Simulate a realistic workday: Morning Focus (90m) $\rightarrow$ Coffee Break (20m) $\rightarrow$ Context Switch (45m) $\rightarrow$ Micro-Detour (3m) $\rightarrow$ Return (60m) $\rightarrow$ Overnight Sleep (12h).
   - Assert accurate segment durations and zero session bleed.
4. **Dormancy & Awakening Tests (`dormancy_tests.rs`)**:
   - Test 30-day inactivity evaluation.
   - Verify auto-awakening on meaningful user interaction and suppression of dormant suggestions.
5. **Reconstruction & Quality Guard End-to-End (`reconstruction_e2e_tests.rs`)**:
   - Validate that `ContextReconstructionService` faithfully embeds the enhanced lifecycle state.

---

## 8. Audit Sign-Off & Next Steps

This audit confirms that ContextSplit has robust building blocks for lifecycle management, but requires formalization of **context-level session intelligence**, **clear pause/resume boundaries**, and **dormancy awakening pipelines**.

### Recommended Next Steps for Day 24:
- **Step 2**: Implement `ContextLifecycleManager` and formalize Context Session data structures.
- **Step 3**: Implement Intelligent Pause & Resume Detection Engines with detour compensation.
- **Step 4**: Implement Dormancy Awakening & Context Reopening Pipeline.
- **Step 5**: Comprehensive Multi-Day Workday Simulation & Adversarial Lifecycle Test Suite.
