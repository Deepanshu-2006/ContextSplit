/**
 * Context Workset Presentation Model
 *
 * Defines the minimal, strongly-typed presentation model for Context Worksets,
 * tailored specifically for the Context Launcher preview and resumption surfaces.
 *
 * Requirements & Invariants:
 * 1. Zero Invention: Never invents tasks, titles, summaries, or descriptions when data is missing.
 * 2. Nonexistent Data Guard: Does not generate summaries or tasks from nonexistent data.
 * 3. Identity Preservation: Preserves links to underlying item and context IDs.
 * 4. Model Separation: Keeps presentation data strictly separated from source-of-truth persistence models.
 * 5. Type Reuse: Reuses existing domain types (tiers, provenance, lifecycle states, action descriptors).
 * 6. Explicit Population States: Makes empty, partial, and complete worksets first-class valid states.
 * 7. Reference Efficiency: Avoids duplicating entire item records; retains only launcher-needed references.
 * 8. Deterministic Ordering: Preserves deterministic ranking and stable tie-breaking.
 * 9. Launcher-Focused: Includes only fields genuinely required by the Context Launcher.
 * 10. Verified Coverage: Comprehensively tested for empty, partial, and complete states.
 */

import type { ContextOperationalLifecycleState } from './quickActions';
import type {
  MinimalContextWorkset,
  MinimalWorksetItem,
  WorksetItemRelevanceTier,
  WorksetProvenance,
  WorksetItemStatus,
  WorksetItemActionDescriptor,
} from './contextWorkset';
import { resolveWorksetItemAction } from './contextWorkset';

// =========================================================================
// 1. Presentation Model Interfaces
// =========================================================================

/**
 * Population completeness state of a workset presentation.
 * - 'empty': Zero items and no active task. Clean-slate context.
 * - 'partial': Context has items but no task, or an active task with no items, or only background references/suggestions.
 * - 'complete': Context has both an active task grounding and primary working focus items.
 */
export type WorksetPopulationState = 'empty' | 'partial' | 'complete';

/**
 * Minimal presentation item for rendering in launcher workset lists.
 * Lightweight projection of MinimalWorksetItem with pre-resolved action descriptors.
 * Invariant: Never duplicates raw item content, full JSON metadata, or vector embeddings.
 */
export interface WorksetPresentationItem {
  /** Underlying item identifier */
  id: string;
  /** Context identifier this item is associated with */
  contextId: string;
  /** Human-readable display title or clean fallback */
  title: string;
  /** Item type (e.g., 'url', 'file', 'text', 'image') */
  itemType: string;
  /** Functional relevance classification tier */
  relevanceTier: WorksetItemRelevanceTier;
  /** Epistemic provenance: confirmed vs inferred */
  provenance: WorksetProvenance;
  /** True if this is an unfiled inbox suggestion */
  isSuggestedAddition: boolean;
  /** True if explicitly marked important */
  isImportant: boolean;
  /** Operational availability status */
  status: WorksetItemStatus;
  /** Associated foreground application name (if known, null if absent) */
  appName: string | null;
  /** Associated domain for web links (if known, null if absent) */
  domain: string | null;
  /** Calibrated confidence score [0.0, 1.0] */
  confidence: number;
  /** Pre-resolved interactive action descriptor */
  action: WorksetItemActionDescriptor;
}

/**
 * Minimal Presentation Model for the Context Launcher.
 * Contains only the information that genuinely helps users resume work within a selected context.
 */
export interface WorksetPresentationModel {
  /** Stable context / workspace identifier */
  contextId: string;
  /** Context display name */
  contextName: string;
  /** Operational lifecycle state */
  lifecycleState: ContextOperationalLifecycleState;
  /** Qualitative readiness band ('ready' | 'mostly_ready' | 'partial' | 'insufficient' | 'blocked') */
  readinessBand: string;
  /** Human-readable explanation of readiness (null if unavailable - zero invention) */
  readinessExplanation: string | null;
  /** Inferred or confirmed active task ID (null if unavailable) */
  activeTaskId: string | null;
  /** Inferred or confirmed active task title (null if unavailable - zero invention) */
  activeTaskTitle: string | null;
  /** Calibrated confidence in the active task [0.0, 1.0] (null if unavailable) */
  taskConfidence: number | null;
  /** True if context has an active task */
  hasTask: boolean;
  /** Natural-language summary of recent contextual activity (null if unavailable - zero invention) */
  recentActivitySummary: string | null;
  /** ISO-8601 timestamp of last activity (null if unavailable) */
  lastActiveAt: string | null;
  /** Primary working focus items directly tied to active work (max 3 for bounded UI) */
  primaryFocus: WorksetPresentationItem[];
  /** Supporting specifications and background reference items (max 3) */
  references: WorksetPresentationItem[];
  /** High-confidence suggested additions from Inbox (max 2) */
  suggestedAdditions: WorksetPresentationItem[];
  /** Total count of items considered in this workset */
  totalItemCount: number;
  /** Population completeness state ('empty' | 'partial' | 'complete') */
  populationState: WorksetPopulationState;
  /** True if working set contains any items */
  hasItems: boolean;
  /** True if working set contains suggested inbox additions */
  hasSuggestions: boolean;
}

// =========================================================================
// 2. Presentation Transformation Functions
// =========================================================================

/**
 * Converts a MinimalWorksetItem into a lightweight WorksetPresentationItem
 * with pre-resolved action descriptors.
 */
export function toWorksetPresentationItem(item: MinimalWorksetItem): WorksetPresentationItem {
  return {
    id: item.id,
    contextId: item.contextId,
    title: item.title,
    itemType: item.itemType,
    relevanceTier: item.relevanceTier,
    provenance: item.provenance,
    isSuggestedAddition: item.isSuggestedAddition,
    isImportant: item.isImportant,
    status: item.status,
    appName: item.appName ? item.appName.trim() : null,
    domain: item.domain ? item.domain.trim() : null,
    confidence: item.confidence,
    action: resolveWorksetItemAction(item),
  };
}

/**
 * Transforms a MinimalContextWorkset into a bounded, presentation-ready WorksetPresentationModel.
 *
 * Guarantees:
 * - Zero Invention: When a field (task, activity summary, explanation) is absent, it resolves strictly to null.
 * - Bounded Density: Primary items capped at 3, references capped at 3, suggestions capped at 2.
 * - Population States: Accurately classifies worksets into 'empty', 'partial', or 'complete'.
 */
export function createWorksetPresentationModel(
  workset: MinimalContextWorkset
): WorksetPresentationModel {
  // 1. Task Grounding (Zero Invention)
  const hasTask = Boolean(workset.activeTaskTitle && workset.activeTaskTitle.trim());
  const activeTaskTitle = hasTask ? workset.activeTaskTitle!.trim() : null;
  const activeTaskId = hasTask && workset.activeTaskId ? workset.activeTaskId.trim() : null;
  const taskConfidence = hasTask && typeof workset.taskConfidence === 'number'
    ? workset.taskConfidence
    : null;

  // 2. Activity & Readiness (Zero Invention)
  const recentActivitySummary = workset.recentActivitySummary && workset.recentActivitySummary.trim()
    ? workset.recentActivitySummary.trim()
    : null;

  const readinessExplanation = workset.readinessExplanation && workset.readinessExplanation.trim()
    ? workset.readinessExplanation.trim()
    : null;

  const lastActiveAt = workset.lastActiveAt && workset.lastActiveAt.trim()
    ? workset.lastActiveAt.trim()
    : null;

  // 3. Bounded Presentation Slices (Preserves Deterministic Sorting)
  const primaryFocus = workset.primaryItems.slice(0, 3).map(toWorksetPresentationItem);
  const references = [...workset.supportingItems, ...workset.referenceItems]
    .slice(0, 3)
    .map(toWorksetPresentationItem);
  const suggestedAdditions = workset.suggestedAdditions
    .slice(0, 2)
    .map(toWorksetPresentationItem);

  // 4. Population State Evaluation
  const totalItemCount = workset.totalItemCount;
  const hasItems = totalItemCount > 0;
  const hasSuggestions = suggestedAdditions.length > 0;

  let populationState: WorksetPopulationState;
  if (!hasItems && !hasTask) {
    populationState = 'empty';
  } else if (hasTask && primaryFocus.length > 0) {
    populationState = 'complete';
  } else {
    populationState = 'partial';
  }

  return {
    contextId: workset.contextId,
    contextName: workset.contextName,
    lifecycleState: workset.lifecycleState,
    readinessBand: workset.readinessBand,
    readinessExplanation,
    activeTaskId,
    activeTaskTitle,
    taskConfidence,
    hasTask,
    recentActivitySummary,
    lastActiveAt,
    primaryFocus,
    references,
    suggestedAdditions,
    totalItemCount,
    populationState,
    hasItems,
    hasSuggestions,
  };
}

/**
 * Creates a valid empty WorksetPresentationModel for contexts with zero items or uninitialized state.
 * Invariant: Never fabricates placeholder tasks or synthetic activity.
 */
export function createEmptyWorksetPresentation(
  contextId: string,
  contextName: string,
  lifecycleState: ContextOperationalLifecycleState = 'active'
): WorksetPresentationModel {
  const cleanId = contextId.trim();
  const cleanName = contextName.trim() || 'Untitled Context';

  return {
    contextId: cleanId,
    contextName: cleanName,
    lifecycleState,
    readinessBand: lifecycleState === 'archived' ? 'blocked' : 'low',
    readinessExplanation: lifecycleState === 'archived'
      ? 'Context is archived and must be unarchived before resuming.'
      : 'Context workspace is empty. Add or capture items to build a workset.',
    activeTaskId: null,
    activeTaskTitle: null,
    taskConfidence: null,
    hasTask: false,
    recentActivitySummary: null,
    lastActiveAt: null,
    primaryFocus: [],
    references: [],
    suggestedAdditions: [],
    totalItemCount: 0,
    populationState: 'empty',
    hasItems: false,
    hasSuggestions: false,
  };
}
