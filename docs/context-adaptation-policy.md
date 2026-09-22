# Context Adaptation Policy

## 1. Overview & Core Philosophy

ContextSplit adapts its contextual understanding through **deterministic, inspectable, and locally computed rules** driven by structured user feedback. 

### Core Architectural Invariants:
1. **No Machine Learning Training Pipeline**: The engine does NOT train or retrain neural networks, gradient boosted trees, or probabilistic black-box models on user actions.
2. **No Model Fine-Tuning**: No language models or embeddings are modified, fine-tuned, or parameterized based on user activity.
3. **Zero External AI Communication for Learning**: User actions, corrections, and contextual history are **NEVER** sent to an external service or cloud provider to infer user habits or preferences.
4. **Specific Entity Learning vs Broad Application Assumptions**: User feedback adapts specific contextual relationships (e.g. associating or dissociating specific repositories, domains, documents, and tasks) rather than imposing blunt, system-wide application bans.

---

## 2. What Feedback Changes vs What Feedback Does Not Change

| Dimension | What Feedback Changes | What Feedback DOES NOT Change |
| :--- | :--- | :--- |
| **Specific Entities** | • Adjusts association score for exact items/entities in the target context.<br>• Boosts confirmed items to permanent authority (`1.0`).<br>• Penalizes rejected items in the target workspace. | • Does not delete raw capture records from the `items` table.<br>• Does not modify historical logs or audit trails. |
| **Application Behavior** | • Applies a **generic noise dampener** to generic/peripheral pages from an application with repeated rejections in that specific workspace (e.g., generic Google searches in PayX). | • Does **NOT** ban the application globally or contextually.<br>• Does **NOT** suppress specific workspace entities from that same app (e.g., a PayX GitHub PR in Chrome remains strongly recognized). |
| **Context Scope** | • Scopes positive affirmations and negative corrections strictly to the target workspace or specific entity. | • Does **NOT** contaminate or penalize unrelated workspaces (e.g., rejecting Chrome tabs in PayX has zero impact on Personal Notes or E-Commerce). |
| **Task Understanding** | • Replaces active task candidate interpretation with user-confirmed task description.<br>• Boosts confirmed tasks to `1.0` confidence. | • Preserves previous task inferences in audit history.<br>• Does not alter tasks in other unrelated workspaces. |
| **Lifecycle State** | • Transitions context state explicitly (e.g., `ResumeContext` moves `Dormant` → `Active`, `ArchiveContext` sets `Archived`). | • Does not delete archived workspace data.<br>• Inactivity alone never triggers `Archived`. |

---

## 3. Scoping & Specificity Hierarchy

ContextSplit feedback operates under a strict 4-level scoping hierarchy. Narrower scopes strictly supersede wider scopes during arbitration:

```
┌─────────────────────────────────────────────────────────┐
│ 1. ItemContext (Specificity: 1 - Highest Priority)      │
│    Target Item + Specific Workspace                     │
├─────────────────────────────────────────────────────────┤
│ 2. SourceContext (Specificity: 2)                       │
│    App / Domain + Specific Workspace                    │
├─────────────────────────────────────────────────────────┤
│ 3. ItemGlobal (Specificity: 3)                          │
│    Target Item across all Workspaces                    │
├─────────────────────────────────────────────────────────┤
│ 4. Global (Specificity: 4 - Lowest Priority)            │
│    Global defaults across all entities & workspaces     │
└─────────────────────────────────────────────────────────┘
```

- **ItemContext Scope**: The standard scope for user corrections (e.g., removing item $X$ from Workspace $W$). Affects only the $(X, W)$ pair.
- **SourceContext Scope**: Applied when generic noise dampening is evaluated (e.g., generic browser tabs in Workspace $W$).
- **Cross-Context Isolation**: Feedback in Workspace $A$ never mutates or dampens candidate scores in Workspace $B$.

---

## 4. Evidence Precedence Hierarchy

When evaluating candidate associations or arbitrating between competing signals, ContextSplit enforces an immutable 7-tier precedence hierarchy:

1. **Tier 1: Explicit User Confirmation (Rank 1)**
   - *Authoritative*: Manual workspace assignment, user acceptance of inbox suggestions, task confirmation.
   - *Weight*: $1.00$ (Overrules all contradictory inferences).
2. **Tier 2: Explicit User Rejection (Rank 2)**
   - *Authoritative*: Manual removal from context, dismissal of inbox suggestions, task rejection.
   - *Weight*: Strong negative penalty ($\ge -0.40$ to $-1.00$). Overrides weak and inferred signals.
3. **Tier 3: Strong Specific Contextual Evidence (Rank 3)**
   - *Corroborated*: Exact repository slug match, URL containing workspace identifier, title matching workspace tokens, explicit file paths in repository root.
   - *Weight*: $\ge 0.85$ (Bypasses generic application noise dampening).
4. **Tier 4: Inferred Semantic Similarity (Rank 4)**
   - *Model/Embedding*: High vector similarity ($\ge 0.75$) with confirmed workspace items.
   - *Weight*: $0.50 - 0.75$.
5. **Tier 5: Weak Contextual Evidence (Rank 5)**
   - *Peripheral*: Shared content type, matching window title substring without repo anchor.
   - *Weight*: $0.20 - 0.40$.
6. **Tier 6: Temporal Proximity (Rank 6)**
   - *Timing*: Captured within recent minutes of workspace activity.
   - *Weight*: $0.15 - 0.30$.
7. **Tier 7: Application Identity (Rank 7)**
   - *App-Only*: Same application executable without corroborating content/URL signals.
   - *Weight*: Weakest prior ($0.10$). Cannot dominate context association on its own.

---

## 5. Expiration & Time Decay Policy

Feedback signals are immutable, but their mathematical influence on scoring gradually decays over time to prevent stale rejections from permanently blocking legitimate future activity.

### Mathematical Decay Schedule:
Let $\Delta t$ be the elapsed time in days since the feedback event was recorded:

$$\text{DecayFactor}(\Delta t) = \begin{cases} 
1.0 & \text{if } \Delta t \le 30 \text{ days (Full Active Effect)} \\
1.0 - 0.4 \times \left(\frac{\Delta t - 30}{60}\right) & \text{if } 30 < \Delta t \le 90 \text{ days (Linear Transition)} \\
0.4 & \text{if } \Delta t > 90 \text{ days (Historical Anchor Floor)}
\end{cases}$$

- **Active Window ($\le 30$ Days)**: Full negative and positive penalty/boost weights apply.
- **Transitional Window ($30-90$ Days)**: Smooth linear decay from $1.0$ down to $0.6$.
- **Stale Floor ($> 90$ Days)**: Negative penalties decay to $40\%$ of baseline, allowing fresh evidence to easily overcome ancient rejections while retaining a mild historical prior.

---

## 6. Conflict Handling & Arbitration Rules

When conflicting signals exist for the same entity or workspace:

1. **Confirmation Overrides Prior Rejection**:
   If an item was rejected in the past but the user explicitly confirms it later, the explicit confirmation wins unconditionally (`ConfirmedDirectly`, score $= 1.0$).
2. **Specific Entity Match Overrides Generic App Rejections**:
   If an application (e.g. `Google Chrome`) has 5 rejections for generic web searches in Workspace $W$, but the user opens `https://github.com/payx/api-service` in Chrome:
   - The specific URL anchor is recognized (`SpecificRelationshipRecognized`).
   - The app-level generic dampener is **completely bypassed**.
   - The item receives strong candidate recognition ($\ge 0.85$).
3. **Simultaneous Equal-Specificity Conflicts**:
   If conflicting feedback events of equal specificity share identical timestamps without clear ordering, the engine flags `EffectiveFeedbackState::Ambiguous` and applies a conservative, non-dominating baseline score rather than fabricating false certainty.
4. **Preservation of Audit History**:
   Resolving a conflict never deletes historical rows from `context_signals` or `context_memberships`. All historical events remain queryable for auditability and explainability.
