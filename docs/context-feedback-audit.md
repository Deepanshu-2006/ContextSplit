# Context Feedback & Adaptation Engine: Architecture Audit (Day 25 — Step 1)

## Executive Summary

As ContextSplit enters **Day 25 (Context Feedback & Adaptation Engine)**, our primary architectural objective is to establish an explicit, deterministic, explainable, and safe **feedback and learning adaptation engine**.

Our foundational design axiom for Day 25 is:
> **The system must get smarter through user corrections without becoming a black box, corrupting cross-workspace boundaries, or suffering from permanent historical bias.**
> User corrections are the highest-quality ground-truth signals in personal context computing. However, naive machine learning or unconstrained global adaptation causes catastrophic forgetting, cross-project contamination, feedback loops, and silent misclassifications. Adaptation in ContextSplit must remain deterministic, inspectable, scoped, and reversible.

Over Days 1–24, ContextSplit established 26 foundational subsystems—including multi-signal evidence collection, 5-tier confidence calibration, hysteresis-based transition detection, active workset tiering, conservative state evaluation, anti-contamination quality guards, memory importance refinement, and lifecycle state machines. In Phase 1 (Step 9), an initial `ContextCorrectionService` and `CorrectionEvidenceSignature` were introduced.

This audit provides a comprehensive end-to-end evaluation of how user feedback is captured, stored, and utilized across the entire ContextSplit stack; answers the 10 core architectural questions in deep technical detail; inventories existing mechanisms; pinpoints missing feedback concepts; establishes feedback scoping, priority hierarchy, and conflict resolution rules; models feedback decay and privacy safeguards; and outlines a verification and testing strategy for Day 25.

---

## 1. Answers to the 10 Core Architectural Questions

### Q1: Existing Feedback Events
The codebase currently recognizes multiple distinct feedback events spread across several tables and subsystems:
1. **Direct Item Removal / Dismissal (`user_correction`)**:
   - Recorded via `ContextCorrectionService::record_context_correction` in `src-tauri/src/context_engine/correction.rs`.
   - Stored in `context_signals` with `signal_type = 'user_correction'` and payload containing `action: "user_removed_from_context"`, `item_id`, `workspace_id`, `evidence_signature`, `reason`, and `scope: "workspace_scoped"`.
2. **Context Membership Dismissal (`membership_dismissed`)**:
   - Recorded via `operations::dismiss_membership` in `src-tauri/src/db/operations.rs`.
   - Stored in `context_memberships` with `membership_type = 'dismissed'`, `confidence = 0.0`, `source = 'user'`.
   - Stored in `context_signals` with `signal_type = 'membership_dismissed'`.
3. **Explicit User Confirmation / Manual Assignment (`confirmed`)**:
   - Recorded via `create_or_update_membership` with `membership_type = 'confirmed'`, `confidence = 1.0`, `source = 'user'`.
   - Sets `items.workspace_id` directly.
4. **Inbox Suggestion Dismissal / Acceptance (`inbox_state`)**:
   - Recorded via `InboxStateMachine::dismiss_item` / `InboxStateMachine::accept_item` in `src-tauri/src/context_engine/inbox_state.rs`.
   - Moves `inbox_item_states.state` to `InboxState::Dismissed` or `InboxState::Accepted`.
5. **Candidate Context Feedback (`candidate_accepted` / `candidate_dismissed`)**:
   - Recorded via `discovery::update_candidate_status` in `src-tauri/src/context_engine/discovery.rs`.
   - Emits `candidate_accepted` or `candidate_dismissed` signals in `context_signals`.
6. **Workset Pinning / Unpinning (`is_pinned`)**:
   - In `context_workset` / `workset.rs`, explicit pinning prevents temporal decay and exclusion.
7. **Important Memory Feedback**:
   - In `important_memory.rs`, user-confirmed high-importance items are promoted to permanent memory records.

---

### Q2: Existing User Correction Behavior
When a user corrects an association (e.g., removing an erroneously associated browser tab, document, or application window from Workspace A):
1. **Membership Demotion**: `context_memberships` row for `(item_id, workspace_id)` is updated to `membership_type = 'dismissed'`, `confidence = 0.0`, `source = 'user'`.
2. **Physical Workspace Unassignment**: If `items.workspace_id == workspace_id`, it is reset to `NULL` (unassigned).
3. **Data Preservation Invariant**: The item in the `items` table is **NEVER** deleted. The raw capture event remains preserved for auditability and assignment to other contexts.
4. **Negative Signal Emission**: A `user_correction` signal is inserted into `context_signals` containing the serialized `CorrectionEvidenceSignature`.
5. **Downstream Cache Invalidation**: Immediately clears caches:
   - `invalidate_workset_cache(Some(workspace_id))`
   - `invalidate_memory_cache(Some(workspace_id))`
   - `invalidate_snapshot_cache(workspace_id)`
   - `invalidate_reconstruction_cache(Some(workspace_id))`

---

### Q3: How Rejected Associations are Stored
Rejected associations are persisted across three structural layers in SQLite:
1. **`context_memberships` Table**:
   ```sql
   (item_id, workspace_id, membership_type='dismissed', confidence=0.0, source='user')
   ```
   Prevents immediate re-inclusion when querying memberships for a workspace.
2. **`context_signals` Table**:
   ```sql
   (id, workspace_id, item_id, signal_type='user_correction', payload='{
     "action": "user_removed_from_context",
     "evidence_signature": {
       "domain": "medium.com",
       "app_name": "Google Chrome",
       "title_keywords": ["random", "tech"],
       "item_type": "url",
       "url": "https://medium.com/random-tech"
     },
     "reason": "Not related to project",
     "scope": "workspace_scoped"
   }')
   ```
   Stores the extracted semantic signature for similarity-based defense against re-association.
3. **`inbox_item_states` Table**:
   ```sql
   (item_id, state='dismissed', previous_state='suggested', reason='User dismissed recommendation')
   ```
   Suppresses attention/inbox badges for the item.

---

### Q4: How Feedback Affects Context Assignment
Feedback currently impacts context assignment at three points in the pipeline:
1. **Evidence Collection (`evidence.rs`)**:
   - `ContextEvidenceCollector` observes direct dismissals (`membership_dismissed`) and injects an `observed_user_dismissal` evidence item with negative contribution.
2. **Contradiction Analysis (`contradiction.rs`)**:
   - Classifies dismissed items under `ContradictionKind::UserCorrection`, applying a severe baseline confidence penalty discount of **0.50** (-50% confidence).
3. **Quality Guard (`quality_guard.rs`)**:
   - Evaluates `is_dismissed` against `context_memberships`. If directly dismissed, triggers a negative feedback dimension score (-1.0) leading to `QualityGuardDecision::Reject`.
4. **Correction Defense Service (`correction.rs`)**:
   - `evaluate_correction_defense` executes a 3-step check:
     - *Step A*: Explicit user action $\rightarrow$ `OverridePermitted` ($\text{conf} = 1.0$).
     - *Step B*: Direct item dismissal match $\rightarrow$ `BlockedByCorrection` unless strong corroborated anchor with $\text{confidence} \ge 0.85$.
     - *Step C*: Pattern similarity against past 30 dismissed signatures $\rightarrow$ if $\text{similarity} \ge 0.70$, yields `BlockedByCorrection` unless strong corroborated anchor with $\text{confidence} \ge 0.85$.

---

### Q5: How Feedback Affects Memory
1. **Memory Query Filtering**: `ContextMemoryCollector` in `memory.rs` queries `context_memberships` filtering for `membership_type IN ('confirmed', 'suggested')` with `confidence > 0.0`. Dismissed items (`membership_type = 'dismissed'`) are completely omitted from memory results.
2. **Memory Importance Promotion**: In `important_memory.rs` and `memory_promotion.rs`, candidate memories with user dismissals are rejected from promotion into long-term structured memory cards.
3. **Cache Coherency**: Memory caches are purged immediately upon feedback recording.
4. **Current Limitation**: Feedback currently acts as a binary gate (exclude vs include). There is no mechanism for user feedback to adjust semantic memory importance weights or correct memory summaries directly.

---

### Q6: How Feedback Affects Worksets
1. **Tier Exclusion**: In `workset.rs`, `ContextWorksetBuilder` excludes dismissed items from `WorksetTier::Active` and `WorksetTier::Secondary`.
2. **Pinning Affirmation**: User pinning (`is_pinned = true`) acts as positive feedback, exempting items from temporal decay and keeping them anchored in the active workset.
3. **Resume Readiness Impact**: In `resume_readiness.rs`, removing uncorroborated items via user correction cleans up spurious working artifacts, improving the resume readiness score.
4. **Current Limitation**: Workset feedback does not distinguish between *"this item is not part of this workspace at all"* (context correction) vs *"this item is not relevant to my current task today"* (task switch / temporary workset trim).

---

### Q7: How Feedback Affects Future Classification
1. **Signature-Based Pattern Matching**:
   - When an item is dismissed, `CorrectionEvidenceSignature` computes a weighted signature:
     $$\text{Similarity}(A, B) = \frac{0.45 \cdot \mathbb{I}(\text{dom}_A = \text{dom}_B) + 0.20 \cdot \mathbb{I}(\text{app}_A = \text{app}_B) + 0.35 \cdot \text{Jaccard}(\text{tokens}_A, \text{tokens}_B)}{0.45 + 0.20 + 0.35}$$
   - When a new item arrives for scoring against Workspace $W$, `evaluate_correction_defense` tests against historical signatures in $W$.
   - If similarity $\ge 0.70$, future automated classification into $W$ is blocked.
2. **Deterministic & Explainable**:
   - The blocking reason is surfaced transparently: *"Matches previously corrected evidence pattern (similarity 82.5%) in workspace 'PayX'"*.
3. **Current Limitation**: The pattern matcher is currently localized inside `correction.rs` and is not fully integrated into `scoring.rs` candidate scoring passes or `quality_guard.rs` batch evaluations.

---

### Q8: Whether Feedback Can Accidentally Become Global
**Risk Assessment**:
- **Critical Failure Mode**: If user feedback on a generic app (e.g. dismissing `google.com` or `VS Code` or `Terminal` from "Personal Finance") were applied globally, it would brick automated context detection across *all* software development and research workspaces.
- **Current Safeguards**:
  - All feedback storage (`context_memberships`, `context_signals`) requires a specific `workspace_id`.
  - Pattern similarity checks in `correction.rs` execute queries strictly constrained by `WHERE workspace_id = ?1`.
  - Unrelated workspaces are mathematically isolated.
- **Vulnerabilities Identified**:
  - Generic URL hosts (e.g. `localhost`, `127.0.0.1`) are filtered, but shared multi-tenant domains (e.g. `github.com`, `notion.so`, `docs.google.com`, `youtube.com`) could over-penalize a workspace if domain weighting (0.45) dominates without path/title token specificity.
  - Workspace-scoped vs Global noise rules are not clearly separated in the data model.

---

### Q9: Whether Feedback Can Conflict
**Conflict Scenarios & Current Resolutions**:
1. **User Dismissal vs Future Explicit Assignment**:
   - *Scenario*: User dismissed Item X yesterday, but today manually drags/assigns Item X to Workspace A.
   - *Resolution*: Handled correctly. Explicit user action sets `is_explicit_user_action = true`, resulting in `CorrectionDefenseVerdict::OverridePermitted` with confidence $1.0$.
2. **User Dismissal vs Strong Corroborated Anchor**:
   - *Scenario*: User dismissed an article from `medium.com/foo`. Later, a commit or file path firmly links `medium.com/foo` to Workspace A with confidence $\ge 0.85$.
   - *Resolution*: Handled. `evaluate_correction_defense` allows `OverridePermitted` when corroborated by a strong anchor and high confidence ($\ge 0.85$).
3. **Multi-Signal Conflict (Confirm then Dismiss)**:
   - *Scenario*: Item X was auto-confirmed in Session 1, then dismissed in Session 2.
   - *Resolution*: Handled via timestamp ordering. `create_or_update_membership` updates the existing row to `membership_type = 'dismissed'`.
4. **Missing Conflict Handling**:
   - No structured conflict resolution exists for contradictory user signals across merged workspaces, split contexts, or simultaneous multi-device inputs.

---

### Q10: Whether Feedback Can Become Stale
1. **Current Inefficiency**:
   - `correction.rs` queries `ORDER BY created_at DESC LIMIT 30` with **zero time decay**.
   - A dismissal from 6 months ago on a temporary topic remains equally potent as a dismissal from 5 minutes ago.
2. **Scope Drift**:
   - Workspaces naturally evolve. A topic or domain that was irrelevant 6 months ago may become the primary focus of the workspace today.
3. **Missing Decay Mechanism**:
   - Negative feedback signals currently lack half-life decay, time-based fading, or relevance re-validation.

---

## 2. Existing Feedback Architecture & Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          USER FEEDBACK ACTIONS                              │
│  ┌───────────────────────┐  ┌───────────────────────┐  ┌─────────────────┐  │
│  │ Manual Item Remove    │  │ Suggestion Dismissal  │  │ Manual Assign   │  │
│  │ (Context Correction)  │  │ (Inbox Reject)        │  │ (Affirmation)   │  │
│  └───────────┬───────────┘  └───────────┬───────────┘  └────────┬────────┘  │
└──────────────┼──────────────────────────┼───────────────────────┼───────────┘
               ▼                          ▼                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         CONTEXT CORRECTION SERVICE                          │
│                                                                             │
│  1. Extract Signature: domain, app_name, title_tokens, item_type, url       │
│  2. Update context_memberships: (item_id, ws_id) -> 'dismissed' (conf: 0.0) │
│  3. Clear physical assignment: items.workspace_id = NULL                    │
│  4. Emit context_signals: 'user_correction' / 'membership_dismissed'        │
│  5. Invalidate downstream caches: Workset, Memory, Snapshot, Reconstruction │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           STORAGE & PERSISTENCE                             │
│  ┌──────────────────────┐  ┌──────────────────────┐  ┌───────────────────┐  │
│  │ context_memberships  │  │ context_signals      │  │ inbox_item_states │  │
│  │ (item_id, ws_id,     │  │ (workspace_id,       │  │ (item_id,         │  │
│  │  'dismissed', 0.0)   │  │  payload with sig)   │  │  'dismissed')     │  │
│  └──────────────────────┘  └──────────────────────┘  └───────────────────┘  │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                        INCOMING ACTIVITY EVALUATION                         │
│                                                                             │
│  Candidate Activity arrive for Workspace W:                                 │
│  1. Check explicit user action? ---> Allow (Override conf = 1.0)            │
│  2. Check exact item dismissed? ---> Block (unless anchor + conf >= 0.85)   │
│  3. Compute signature similarity against past 30 dismissals:                │
│     - If similarity >= 0.70 ---> BlockByCorrection                          │
│     - If similarity < 0.70  ---> Proceed to normal scoring                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Comprehensive Inventory of Missing Feedback Concepts

| Missing Concept | Impact on Current System | Proposed Architectural Solution |
| :--- | :--- | :--- |
| **Unified Feedback Engine** | Feedback handling is split between `db/operations.rs`, `correction.rs`, and `inbox_state.rs`. | Consolidate all feedback handling into a centralized `ContextFeedbackEngine`. |
| **Multi-Class Feedback Types** | Only binary remove/dismiss exists; lacks explicit affirmative feedback, misattribution feedback, and noise tagging. | Introduce rich `FeedbackType` enum: `ExplicitRemoval`, `TargetWorkspaceReassignment`, `PermanentExclusion`, `TemporaryDetourNoise`, `MemoryRefinement`, `AffirmativeAffinity`. |
| **Negative Pattern Decay** | Historical negative feedback never decays, risking permanent blind spots. | Implement exponential half-life decay for pattern-based corrections (e.g. $t_{1/2} = 14\text{ days}$ for weak patterns; permanent for exact item IDs). |
| **Feedback Scope Taxonomy** | Ambiguity between workspace-specific rejections vs global noise exclusions. | Formalize `FeedbackScope`: `WorkspaceScoped(ws_id)`, `CrossWorkspaceReassignment(from_id, to_id)`, and `GlobalNoiseFilter`. |
| **Feedback Priority Hierarchy** | No explicit deterministic priority matrix between conflicting signals. | Define a strict 6-tier deterministic priority hierarchy. |
| **Direct Scoring Integration** | `evaluate_correction_defense` is not directly evaluated during the scoring pipeline loop in `scoring.rs`. | Embed feedback defense directly into `ContextEvidenceScorer` and `ContradictionAnalyzer`. |
| **Explainable Feedback Audit Trail** | User cannot inspect what rules or patterns were learned from past corrections. | Build an inspectable `FeedbackLedger` with explanation records and revocation capabilities. |
| **Memory & Workset Feedback API** | No clean way for UI to send feedback on memory cards or task summaries. | Implement Tauri IPC commands for contextual feedback on memories, tasks, and worksets. |

---

## 4. Deterministic Feedback Scope & Priority Hierarchy

### Feedback Scope Matrix

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                                FEEDBACK SCOPES                               │
├──────────────────────────┬───────────────────────────────────────────────────┤
│ Scope                    │ Behavioral Rule & Storage Location                │
├──────────────────────────┼───────────────────────────────────────────────────┤
│ 1. Workspace-Scoped      │ Affects ONLY the target workspace. Stored with    │
│    (Default)             │ workspace_id. Never affects other contexts.       │
├──────────────────────────┼───────────────────────────────────────────────────┤
│ 2. Context Reassignment  │ Removes from Workspace A and attaches to B.       │
│    (Move Correction)     │ Creates negative signal in A and positive in B.   │
├──────────────────────────┼───────────────────────────────────────────────────┤
│ 3. Global Noise Filter   │ Affects system-wide noise (e.g. login pages,      │
│    (Explicit Only)       │ ad trackers). Stored with workspace_id = NULL.    │
└──────────────────────────┴───────────────────────────────────────────────────┘
```

### Deterministic Priority Hierarchy

When evaluating candidate context associations, evidence sources must resolve strictly according to the following precedence hierarchy:

$$\text{Priority Tier 1} \gg \text{Priority Tier 2} \gg \text{Priority Tier 3} \gg \text{Priority Tier 4} \gg \text{Priority Tier 5} \gg \text{Priority Tier 6}$$

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        DETERMINISTIC PRIORITY HIERARCHY                      │
├──────┬────────────────────────────────────┬────────────┬─────────────────────┤
│ Tier │ Evidence / Signal Type             │ Confidence │ Override Power      │
├──────┼────────────────────────────────────┼────────────┼─────────────────────┤
│ T1   │ Explicit User Action (Manual Move/ │ 1.00       │ Overrides all prior │
│      │ Pin/Assign)                        │            │ dismissals & guards │
├──────┼────────────────────────────────────┼────────────┼─────────────────────┤
│ T2   │ Direct Item User Correction        │ 0.00       │ Overrides all auto- │
│      │ (Exact item dismissed from ws)     │ (Blocked)  │ scores (< 0.85)     │
├──────┼────────────────────────────────────┼────────────┼─────────────────────┤
│ T3   │ Strong Corroborated Structural     │ 0.85 - 1.0 │ Overrides pattern   │
│      │ Anchor (Git repo / Project Dir)    │            │ dismissals (T4)     │
├──────┼────────────────────────────────────┼────────────┼─────────────────────┤
│ T4   │ Pattern-Based User Correction      │ Penalty    │ Suppresses weak     │
│      │ (Signature similarity >= 0.70)     │ (Blocked)  │ heuristic matches   │
├──────┼────────────────────────────────────┼────────────┼─────────────────────┤
│ T5   │ Corroborated Behavioral Heuristics │ 0.50 - 0.84│ Standard evaluation │
│      │ (App + Window Title + History)     │            │                     │
├──────┼────────────────────────────────────┼────────────┼─────────────────────┤
│ T6   │ Weak / Uncorroborated Semantic     │ 0.15 - 0.49│ Held for more       │
│      │ Similarity / Generic App Glance    │            │ evidence            │
└──────┴────────────────────────────────────┴────────────┴─────────────────────┘
```

---

## 5. Mathematical Formulation of Feedback Decay & Adaptation

### 1. Pattern Feedback Decay Formula
Pattern-based negative feedback should decay over time to prevent obsolete corrections from permanently suppressing legitimate activity as projects evolve:

$$S_{\text{effective}}(t) = S_{\text{base}} \cdot e^{-\lambda (t - t_0)}$$

Where:
- $S_{\text{base}}$ is the initial pattern similarity score $[0.0, 1.0]$.
- $t - t_0$ is the elapsed time since the user correction was recorded.
- $\lambda = \frac{\ln(2)}{t_{1/2}}$ is the decay constant with half-life $t_{1/2} = 30\text{ days}$.
- If $S_{\text{effective}}(t) < 0.35$, the pattern suppression naturally expires.

### 2. Exact Item Dismissal (No Decay)
Direct item dismissals on specific items (`membership_type = 'dismissed'`) do **not** decay. An exact item dismissed from a context remains dismissed unless explicitly re-added or overridden by a Tier 1 or Tier 3 signal.

### 3. Adaptive Affirmation Boost
When a user explicitly confirms or reassigns an item to Workspace $W$, the extracted evidence signature generates an affirmative adaptation profile:

$$B_{\text{affirm}}(x) = \min\left(0.25, \sum_{i=1}^{k} w_i \cdot \text{Sim}(x, \text{Sig}_i) \cdot e^{-\lambda (t - t_i)}\right)$$

This provides a bounded, conservative boost ($\le +0.25$) to future matching candidates without risking runaway false positives.

---

## 6. Privacy & Safety Considerations

1. **Local-Only Execution**: All feedback signatures, adaptation weights, and correction records reside strictly in the local SQLite database. No telemetry, title tokens, or feedback patterns are transmitted externally.
2. **Deterministic & Reversible**: Every adaptation rule can be inspected, traced back to the originating user action, and explicitly revoked by the user.
3. **No Unbounded Machine Learning**: The system does not train opaque neural network weights or embedding fine-tunings that could unpredictably drift or cross-contaminate.
4. **Data Minimization in Signatures**: URL query parameters, authentication tokens, and private paths are scrubbed from `CorrectionEvidenceSignature` before persistence.

---

## 7. Verification & Testing Strategy for Day 25

To ensure absolute reliability, Day 25 implementation will be verified through comprehensive test suites:

1. **Unit Tests (`context_engine/feedback_tests.rs`)**:
   - Verification of `FeedbackType` transitions and state updates.
   - Exact signature extraction, domain extraction, and keyword Jaccard math.
   - Pattern similarity calculation and threshold gating ($0.70$ blocking, $0.85$ anchor override).
   - Exponential decay calculation over simulated time deltas.
2. **Integration Tests (`context_engine/feedback_integration_tests.rs`)**:
   - End-to-end user correction flow: remove item $\rightarrow$ verify membership demotion $\rightarrow$ verify cache invalidation $\rightarrow$ verify subsequent telemetry blocked.
   - Context move correction: move item from Workspace A to B $\rightarrow$ verify A receives negative signal and B receives positive assignment.
   - Override verification: ensure explicit user action immediately unblocks a previously dismissed item.
3. **Adversarial & Regression Tests**:
   - Global leakage test: verify that correcting a generic application (e.g. Chrome/Terminal) in Workspace A does not degrade classification in Workspace B.
   - Feedback storm test: simulate rapid consecutive user corrections and verify cache consistency and database transaction stability.
   - Zero-data-loss invariant: verify that corrected items are never deleted from `items` table.

---

## 8. Summary of Day 25 Roadmap

- **Step 1 (Current)**: Full Feedback Architecture Audit (`docs/context-feedback-audit.md`).
- **Step 2**: Unification of Feedback Types, Models, and Engine (`src/context_engine/feedback.rs`).
- **Step 3**: Integration of Feedback Evaluation into Scoring Pipeline & Quality Guards.
- **Step 4**: Time-Decayed Adaptation & Affirmative Pattern Reinforcement.
- **Step 5**: Tauri IPC Commands & End-to-End Test Suite Verification.
