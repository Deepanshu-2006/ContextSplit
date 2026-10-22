# Context Workset Contract Specification

**Milestone:** Phase 2 — Context Launcher (Day 35: Context Workset Integration)  
**Date:** October 22, 2026  
**Document:** [`docs/context-workset-contract.md`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/docs/context-workset-contract.md)  
**Status:** **RATIFIED & VERIFIED**  
**Associated Implementation & Tests:** [`src/models/contextWorkset.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/models/contextWorkset.ts), [`src/models/contextWorkset.test.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/models/contextWorkset.test.ts)

---

## 1. Executive Summary & Purpose

A **Context Workset** represents the minimal, cohesive set of working information a user needs to resume and continue work within a selected context.

When a user switches contexts (e.g. opens the Context Launcher and inspects a workspace), they should not encounter an unranked dump of dozens of accumulated historical assets, nor should they be delayed by slow cloud or local AI inferences. Instead, ContextSplit provides a **deterministic, ephemeral working-state projection** that grounds current focus:

1. **What was I specifically working on?** (The active task and goal from `task_candidates` and `ContextMemory`).
2. **What items are core to this active state right now?** (`Primary` working documents, files, and components).
3. **What items are peripheral or background references?** (`Supporting` specs and `Reference` documentation links).
4. **What unfiled inbox items are relevant to this context?** (`Suggested Additions` inferred with high confidence).
5. **How fresh is this state?** (Operational readiness and recency).

This contract establishes a formal, strongly-typed boundary between the Phase 1 Context Engine, the data persistence layer, and the Context Launcher presentation surfaces.

---

## 2. Invariants & Architectural Requirements

### Requirement 1: Model Reuse Without Reinvention
The Context Workset layer does **not** introduce redundant database schemas, parallel ranking algorithms, or conflicting entity definitions. It strictly reuses:
- **Context / Workspace:** SQLite table `workspaces` ([`src-tauri/src/db/models.rs:3-14`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/db/models.rs#L3-L14)) and [`src/services/db.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/services/db.ts).
- **Items:** SQLite table `items` ([`src-tauri/src/db/models.rs:16-27`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/db/models.rs#L16-L27)) storing content, titles, types (`url`, `file`, `text`, `image`), and metadata.
- **Context Memberships:** SQLite table `context_memberships` ([`src-tauri/src/db/models.rs:240-251`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src-tauri/src/db/models.rs#L240-L251)) tracking confirmed, suggested, and dismissed associations.
- **Tasks:** SQLite table `task_candidates` (Migration 21) and `ContextMemory.current_task`.
- **Sessions & Activity:** SQLite table `sessions`, `context_activity_segments`, and `context_transitions`.
- **Item Relationships:** SQLite table `item_relationships` (`references`, `documents`, `semantic_similarity`).

### Requirement 2: Minimal Information Density
The workset exposes only what is required to continue work:
- Context identity (`contextId`, `contextName`, `lifecycleState`).
- Active task grounding (`activeTaskId`, `activeTaskTitle`, `taskConfidence`).
- Sliced item collections (`primaryItems`, `supportingItems`, `referenceItems`, `suggestedAdditions`).
- Operational status (`worksetState`, `confidence`, `confidenceBand`, `readinessBand`, `readinessExplanation`).
- Invariant: Heavy item payloads, raw vector embeddings, and full-text documents are never duplicated in the workset reference.

### Requirement 3: Confirmed Membership vs. Inferred Relevance
The contract strictly enforces epistemic honesty:
- **`confirmed` Provenance:** Items explicitly moved or assigned by the user to the workspace (`items.workspace_id === contextId`) or accepted in `context_memberships`. Confidence is 1.0.
- **`inferred` Provenance:** High-confidence suggestions discovered by the Context Engine from unfiled Inbox items (`context_memberships.membership_type === 'suggested' AND confidence >= 0.60`).
- **Ordering Guarantee:** Confirmed items **always outrank** inferred suggestions within equal relevance tiers, regardless of numerical scores.

### Requirement 4: Selective Relevance (Anti-Accumulation)
Over time, a workspace can accumulate dozens or hundreds of items. **Not every item in a workspace belongs in the active workset.**
An item is admitted into the minimal workset if and only if:
1. It is explicitly pinned as important contextual memory (`isImportant: true`), OR
2. It is directly tied to the active task (`relevanceTier === 'primary' | 'supporting'`), OR
3. It has recent active session activity or a composite relevance score $\ge 0.28$.
Stale, unreferenced background items remain safely in the workspace SQLite database but are excluded from the workset to eliminate clutter.

### Requirement 5: Physical Storage Location vs. Contextual Association
- **Physical Location (`physicalLocation: 'workspace' | 'inbox'`):** Indicates where the file/record physically resides (`items.workspace_id`).
- **Contextual Association (`contextId`, `isSuggestedAddition`):** Indicates the relationship to the selected workspace.
- **Non-Destructive Workset Actions:** Removing an item from a workset (`item.remove_from_context`) updates `context_memberships.membership_type = 'dismissed'`. It **never deletes the item** or moves its physical file.

### Requirement 6: Multi-Context Association Support
Because SQLite table `context_memberships` uses a composite unique constraint `UNIQUE(item_id, workspace_id)`, a single item can be associated with multiple contexts:
- An item can be a confirmed physical asset in Context A (`physicalLocation: 'workspace'`).
- The same item can simultaneously be referenced as a supporting asset in Context B (`contextId: 'ctx-b'`, `physicalLocation: 'workspace'`, `workspaceId: 'ctx-a'`).
- Removing the item from Context B's workset only dismisses the association with Context B; Context A is completely unaffected.

### Requirement 7: Existing Relevance Tiers & Calibrated Confidence
Reuses the 5-tier classification defined in the Phase 1 engine:
1. **`primary`:** Core document/code/canvas component being edited.
2. **`supporting`:** Mocks, specifications, API endpoints referencing or referenced by the active task.
3. **`reference`:** Repositories, documentation links, permanent workspace assets.
4. **`recent`:** Items interacted with in the latest burst without specific task connection.
5. **`suggested`:** Unfiled Inbox items inferred to belong to this context.

### Requirement 8: Predictable Edge-Case Handling (Missing, Deleted, Archived)
- **Deleted Items:** Marked with `status: 'deleted'`, clean fallback title `'[Deleted Item]'`, and excluded from `primaryItems` and `supportingItems`. Action descriptors mark operations as non-executable with structured explanations.
- **Missing / Corrupted Items:** Marked `status: 'missing'`, clean title `'[Unavailable Item]'`, and excluded from active focus.
- **Archived Contexts:** Resumed only through explicit unarchival (`db.unarchiveWorkspace`), preventing ghost states.

### Requirement 9: Deterministic Generation Without AI Services
The minimal workset derivation pipeline runs **entirely offline** against local SQLite database indexes. It evaluates 10 heuristic and temporal signals (task alignment, recency decay, relationship strength, segment burst frequency) in $< 15\text{ms}$. No cloud LLM, API keys, or network connections are required.

### Requirement 10: AI Enrichment Remains Optional
If local or cloud AI reasoning is enabled (`settings.enable_ai_reasoning`), semantic summaries and suggested next steps may optionally annotate the workset asynchronously (`aiEnrichment?: WorksetAiEnrichment`). The retrieval path **never awaits or blocks on an AI model**.

### Requirement 11: Ephemeral Projection (No Persistent Database Model)
The audit demonstrated that `ContextWorkset` is a derived, transient synthesis of underlying persistent tables (`workspaces`, `items`, `context_memberships`, `task_candidates`, `item_relationships`). Adding a persistent `worksets` table in SQLite would introduce redundant data, stale cache bugs, and synchronization overhead. The workset remains an **ephemeral projection cached in memory** with a 30-second TTL and event-driven invalidation.

### Requirement 12: Context Launcher Presentation Interface
Exposes a bounded `LauncherWorksetSlice` tailored for the launcher preview pane:
- Top 3 Primary items.
- Top 3 Reference items.
- Top 2 Suggested Inbox items.
- Action resolver: URL items resolve to `"Open in Browser"`, note items resolve to `"Copy Note"`, file items resolve to `"Open File"`, and suggested items resolve to `"Accept into Context"`.

---

## 3. Data Contract Specification

### 3.1 Type Definitions

```typescript
export type WorksetItemRelevanceTier =
  | 'primary'
  | 'supporting'
  | 'reference'
  | 'recent'
  | 'suggested';

export type WorksetProvenance = 'confirmed' | 'inferred';
export type WorksetPhysicalLocation = 'workspace' | 'inbox' | 'external';
export type WorksetItemStatus = 'available' | 'missing' | 'archived' | 'deleted';

export interface MinimalWorksetItem {
  id: string;
  title: string;
  itemType: string;
  relevanceTier: WorksetItemRelevanceTier;
  provenance: WorksetProvenance;
  physicalLocation: WorksetPhysicalLocation;
  workspaceId: string | null;
  contextId: string;
  confidence: number;
  isSuggestedAddition: boolean;
  isImportant: boolean;
  reasons: string[];
  appName?: string | null;
  domain?: string | null;
  contentSnippet?: string | null;
  status: WorksetItemStatus;
  createdAt?: string | null;
  lastActiveAt?: string | null;
}

export interface MinimalContextWorkset {
  contextId: string;
  contextName: string;
  lifecycleState: ContextOperationalLifecycleState;
  worksetState: WorksetState;
  activeTaskId?: string | null;
  activeTaskTitle?: string | null;
  taskConfidence?: number | null;
  generatedAt: string;
  lastActiveAt?: string | null;
  confidence: number;
  confidenceBand: string;
  items: MinimalWorksetItem[];
  primaryItems: MinimalWorksetItem[];
  supportingItems: MinimalWorksetItem[];
  referenceItems: MinimalWorksetItem[];
  suggestedAdditions: MinimalWorksetItem[];
  totalItemCount: number;
  readinessBand: string;
  readinessExplanation?: string | null;
  recentActivitySummary?: string | null;
  aiEnrichment?: WorksetAiEnrichment | null;
}
```

### 3.2 Context Launcher Presentation Model (`WorksetPresentationModel`)

To keep launcher rendering performant, deterministic, and free of hallucinations, the presentation layer uses an independent projection model defined in [`src/models/contextWorksetPresentation.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972%40gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/models/contextWorksetPresentation.ts):

```typescript
export type WorksetPopulationState = 'empty' | 'partial' | 'complete';

export interface WorksetPresentationItem {
  id: string;
  contextId: string;
  title: string;
  itemType: string;
  relevanceTier: WorksetItemRelevanceTier;
  provenance: WorksetProvenance;
  isSuggestedAddition: boolean;
  isImportant: boolean;
  status: WorksetItemStatus;
  appName: string | null;
  domain: string | null;
  confidence: number;
  action: WorksetItemActionDescriptor;
}

export interface WorksetPresentationModel {
  contextId: string;
  contextName: string;
  lifecycleState: ContextOperationalLifecycleState;
  readinessBand: string;
  readinessExplanation: string | null;
  activeTaskId: string | null;
  activeTaskTitle: string | null;
  taskConfidence: number | null;
  hasTask: boolean;
  recentActivitySummary: string | null;
  lastActiveAt: string | null;
  primaryFocus: WorksetPresentationItem[]; // Max 3
  references: WorksetPresentationItem[];   // Max 3
  suggestedAdditions: WorksetPresentationItem[]; // Max 2
  totalItemCount: number;
  populationState: WorksetPopulationState;
  hasItems: boolean;
  hasSuggestions: boolean;
}
```

#### Key Presentation Decisions
1. **Zero-Invention Guarantee:** When an active task or recent activity summary does not exist in persisted data, the presentation model assigns `null` and `hasTask: false`. Placeholder text (e.g., *"Working on context"*) is strictly forbidden.
2. **Explicit Population States:** Contexts are explicitly classified into `'empty'` (0 items, no task), `'partial'` (items without task, task without items, or references only), or `'complete'` (task grounding + primary focus items).
3. **Bounded Density:** Prevents cognitive overload by capping collections to 3 primary, 3 reference, and 2 suggested additions.
4. **Action-Precomputed:** Every item carries its pre-resolved click action (`open_browser`, `open_file`, `copy_content`, `accept_suggestion`), disabling actions for deleted or missing records with explanatory reasons.
5. **Reference Efficiency:** Omits large text bodies, full metadata JSON, and vector embeddings, preserving only IDs and metadata needed by the launcher preview pane.

---

## 4. Verification & Test Suite

The contract, retrieval service, and presentation models are verified by 48 tests across 3 dedicated test suites:

| Test Suite File | Coverage Domain | Status |
|---|---|---|
| [`src/models/contextWorkset.test.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972%40gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/models/contextWorkset.test.ts) | Contract models, normalization, provenance, tier demotion, edge-case statuses, offline derivation | **PASSED (19/19)** |
| [`src/services/contextWorksetRetrieval.test.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972%40gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/services/contextWorksetRetrieval.test.ts) | Targeted indexed queries, empty contexts, multi-context items, deduplication, clutter exclusion, zero-AI, privacy logging | **PASSED (19/19)** |
| [`src/models/contextWorksetPresentation.test.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972%40gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/models/contextWorksetPresentation.test.ts) | Empty/partial/complete population states, zero-invention invariants, bounded collections, disabled actions for missing/deleted items | **PASSED (10/10)** |
| **Total Test Execution** | **Full Context Workset Architecture Verification** | **100% PASSED (48/48)** |

---

## 5. Summary & Next Steps

This contract formally establishes the boundaries, guarantees, retrieval service, and presentation model for the Context Workset.

Next Steps:
1. Bind [`WorksetPresentationModel`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972%40gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/models/contextWorksetPresentation.ts) to the Context Launcher preview pane (`WorksetPreviewRow` and section headers) in `src/components/launcher/ContextLauncher.tsx`.
2. Connect keyboard traversal (`ArrowRight` into workset items).
