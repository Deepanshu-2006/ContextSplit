/**
 * Minimal Context Workset Contract
 *
 * Defines the typed interface, domain separation, tier derivation,
 * multi-context representation, and edge-case handling for Context Worksets in ContextSplit.
 *
 * Invariants & Requirements:
 * 1. Reuses existing context, item, task, session, and relationship models.
 * 2. Defines the minimum information required to represent an actionable workset.
 * 3. Strictly distinguishes confirmed context membership from inferred relevance.
 * 4. Does not treat every item in a workspace as automatically relevant.
 * 5. Preserves the difference between physical item location and contextual association.
 * 6. Supports items associated with multiple contexts without cross-contamination.
 * 7. Incorporates existing context relevance and calibrated confidence scores.
 * 8. Predictably handles missing, deleted, archived, or unavailable items.
 * 9. Entirely executable and derivable without invoking AI services.
 * 10. AI enrichment remains strictly optional and outside the core retrieval path.
 * 11. Ephemeral projection: avoids persistent workset tables in SQLite.
 * 12. Exposes a clean, presentation-ready adapter for the Context Launcher.
 */

import type { ContextOperationalLifecycleState } from './quickActions';
import type { ContextWorkset, WorksetItemRelevance } from '../services/contextWorkset';

// =========================================================================
// 1. Relevance Tiers & Epistemic Types
// =========================================================================

/**
 * Functional relevance classification for an item in a workset.
 * - `primary`: Core active task document, file, or component currently being edited/worked on.
 * - `supporting`: Directly referenced specifications, mocks, or libraries supporting the active task.
 * - `reference`: Background workspace repositories, documentation links, or persistent assets.
 * - `recent`: Items touched in the latest session burst without explicit active-task grounding.
 * - `suggested`: High-confidence candidate items from the Inbox suggested for addition.
 */
export type WorksetItemRelevanceTier =
  | 'primary'
  | 'supporting'
  | 'reference'
  | 'recent'
  | 'suggested';

/**
 * Epistemic source indicating how an item's association with the context was established.
 */
export type WorksetProvenance = 'confirmed' | 'inferred';

/**
 * Physical storage location of an item in the system.
 */
export type WorksetPhysicalLocation = 'workspace' | 'inbox' | 'external';

/**
 * Lifecycle/availability status of an item within the workset.
 */
export type WorksetItemStatus = 'available' | 'missing' | 'archived' | 'deleted';

/**
 * Operational state of the working set.
 */
export type WorksetState =
  | 'active'
  | 'recently_active'
  | 'detour_return'
  | 'dormant'
  | 'insufficient_evidence';

// =========================================================================
// 2. Minimal Workset Item Model
// =========================================================================

/**
 * Lightweight reference representing a single actionable item within a context workset.
 * Invariant: Never duplicates raw vector embeddings, heavy base64 strings, or multi-megabyte payloads.
 */
export interface MinimalWorksetItem {
  /** Unique item identifier */
  id: string;
  /** Human-readable title or clean descriptive fallback */
  title: string;
  /** Recognized item type (e.g., 'url', 'file', 'text', 'image', 'clipboard') */
  itemType: string;
  /** Functional relevance classification tier */
  relevanceTier: WorksetItemRelevanceTier;
  /** Epistemic provenance: user-confirmed vs engine-inferred */
  provenance: WorksetProvenance;
  /** Physical location: stored in workspace directory, unfiled inbox, or external */
  physicalLocation: WorksetPhysicalLocation;
  /** Physical workspace ID (if placed in a workspace, null if in Inbox) */
  workspaceId: string | null;
  /** The specific context ID this workset projection is derived for */
  contextId: string;
  /** Composite relevance / confidence score in range [0.0, 1.0] */
  confidence: number;
  /** True if this is an unfiled inbox item suggested for addition to this context */
  isSuggestedAddition: boolean;
  /** True if the user explicitly pinned this item as important contextual memory */
  isImportant: boolean;
  /** Natural-language explainable reasons for inclusion and ranking */
  reasons: string[];
  /** Associated foreground application name (e.g., "VS Code", "Safari") */
  appName?: string | null;
  /** Associated web domain (e.g., "github.com", "figma.com") */
  domain?: string | null;
  /** Privacy-safe content preview snippet (max 120 chars) */
  contentSnippet?: string | null;
  /** Operational availability status */
  status: WorksetItemStatus;
  /** ISO-8601 creation timestamp */
  createdAt?: string | null;
  /** ISO-8601 last active timestamp */
  lastActiveAt?: string | null;
}

// =========================================================================
// 3. Optional AI Enrichment Model (Outside Retrieval Path)
// =========================================================================

/**
 * Optional AI enrichment metadata.
 * Invariant: Must remain outside the core retrieval path and never block workset generation.
 */
export interface WorksetAiEnrichment {
  /** Optional synthesized active working state summary */
  summary?: string;
  /** Inferred key concepts or topic keywords */
  keyConcepts?: string[];
  /** Suggested next steps or follow-ups */
  suggestedNextActions?: string[];
  /** ISO-8601 generation timestamp */
  enrichedAt: string;
  /** Model or provider that generated the enrichment */
  provider?: string;
}

// =========================================================================
// 4. Minimal Context Workset Model
// =========================================================================

/**
 * Complete minimal working state required to resume and continue work in a context.
 * Synthesized transiently on demand; not stored as a persistent database row.
 */
export interface MinimalContextWorkset {
  /** Context / Workspace identifier */
  contextId: string;
  /** Human-readable context display name */
  contextName: string;
  /** Operational lifecycle state */
  lifecycleState: ContextOperationalLifecycleState;
  /** Operational workset state */
  worksetState: WorksetState;
  /** Inferred or confirmed active task ID */
  activeTaskId?: string | null;
  /** Inferred or confirmed active task title */
  activeTaskTitle?: string | null;
  /** Calibrated confidence in the active task [0.0, 1.0] */
  taskConfidence?: number | null;
  /** ISO-8601 timestamp of derivation */
  generatedAt: string;
  /** ISO-8601 timestamp of most recent activity in this context */
  lastActiveAt?: string | null;
  /** Overall context workset confidence [0.0, 1.0] */
  confidence: number;
  /** Qualitative confidence band ('high', 'medium', 'low', 'none') */
  confidenceBand: string;
  /** All eligible workset items, ordered by relevance */
  items: MinimalWorksetItem[];
  /** Core working documents / components directly tied to active task (max 5) */
  primaryItems: MinimalWorksetItem[];
  /** Supporting assets and specifications tied to active task */
  supportingItems: MinimalWorksetItem[];
  /** Background references, repositories, and documentation links */
  referenceItems: MinimalWorksetItem[];
  /** High-confidence unfiled inbox items suggested for addition */
  suggestedAdditions: MinimalWorksetItem[];
  /** Total count of items considered in this workset */
  totalItemCount: number;
  /** Resume readiness band ('ready', 'mostly_ready', 'partial', 'insufficient') */
  readinessBand: string;
  /** Human-readable readiness explanation */
  readinessExplanation?: string | null;
  /** Natural-language summary of recent contextual activity */
  recentActivitySummary?: string | null;
  /** Optional AI enrichment (never blocks generation) */
  aiEnrichment?: WorksetAiEnrichment | null;
}

// =========================================================================
// 5. Item Normalization & Edge-Case Handlers
// =========================================================================

export interface RawWorksetItemInput {
  id: string;
  title?: string | null;
  itemType?: string | null;
  content?: string | null;
  contentSnippet?: string | null;
  workspaceId?: string | null;
  relevanceTier?: WorksetItemRelevanceTier | WorksetItemRelevance | null;
  confidence?: number | null;
  isSuggestedAddition?: boolean | null;
  isImportant?: boolean | null;
  reasons?: string[] | null;
  appName?: string | null;
  domain?: string | null;
  status?: WorksetItemStatus | null;
  isDeleted?: boolean;
  isArchived?: boolean;
  createdAt?: string | null;
  lastActiveAt?: string | null;
  provenance?: WorksetProvenance | null;
}

/**
 * Normalizes an item for a specific context workset, handling edge cases:
 * - Empty, blank, or missing titles fallback to content snippet or generic item type.
 * - Missing or deleted items receive a predictable 'deleted' / 'missing' status.
 * - Epistemic provenance is enforced based on physical placement and suggestion flags.
 * - Physical location is isolated from contextual association.
 */
export function normalizeWorksetItem(
  raw: RawWorksetItemInput,
  targetContextId: string
): MinimalWorksetItem {
  const cleanId = (raw.id || '').trim();

  // 1. Resolve Availability Status
  let status: WorksetItemStatus = raw.status || 'available';
  if (raw.isDeleted) {
    status = 'deleted';
  } else if (raw.isArchived) {
    status = 'archived';
  } else if (!raw.content && !raw.title && !raw.contentSnippet) {
    status = 'missing';
  }

  // 2. Resolve Physical Location
  const physicalWorkspaceId = raw.workspaceId ? raw.workspaceId.trim() : null;
  let physicalLocation: WorksetPhysicalLocation = 'inbox';
  if (physicalWorkspaceId && physicalWorkspaceId !== 'inbox') {
    physicalLocation = 'workspace';
  }

  // 3. Resolve Provenance & Suggestion Flag
  const isSuggested = Boolean(raw.isSuggestedAddition);
  const provenance: WorksetProvenance =
    raw.provenance || (isSuggested || physicalWorkspaceId !== targetContextId ? 'inferred' : 'confirmed');

  // 4. Resolve Clean Title (Zero Fabrication, Graceful Fallback)
  let cleanTitle = (raw.title || '').trim();
  if (status === 'deleted') {
    cleanTitle = '[Deleted Item]';
  } else if (status === 'missing') {
    cleanTitle = '[Unavailable Item]';
  } else if (!cleanTitle) {
    if (raw.contentSnippet) {
      cleanTitle = raw.contentSnippet.trim();
    } else if (raw.itemType === 'url') {
      cleanTitle = raw.domain ? `Link (${raw.domain})` : 'Captured Link';
    } else if (raw.itemType === 'file') {
      cleanTitle = 'Attached File';
    } else {
      cleanTitle = 'Captured Note';
    }
  }

  // 5. Resolve Relevance Tier
  let relevanceTier: WorksetItemRelevanceTier = 'reference';
  if (isSuggested) {
    relevanceTier = 'suggested';
  } else if (raw.relevanceTier) {
    const r = raw.relevanceTier.toLowerCase();
    if (r === 'primary') relevanceTier = 'primary';
    else if (r === 'supporting') relevanceTier = 'supporting';
    else if (r === 'recent') relevanceTier = 'recent';
    else if (r === 'suggested' || r === 'related') relevanceTier = 'suggested';
    else relevanceTier = 'reference';
  }

  // 6. Clean Snippet
  const snippet = raw.contentSnippet
    ? raw.contentSnippet.trim()
    : raw.content
    ? raw.content.trim().slice(0, 120)
    : null;

  return {
    id: cleanId,
    title: cleanTitle,
    itemType: raw.itemType || 'text',
    relevanceTier,
    provenance,
    physicalLocation,
    workspaceId: physicalWorkspaceId,
    contextId: targetContextId,
    confidence: typeof raw.confidence === 'number' ? Math.max(0, Math.min(1, raw.confidence)) : 0.5,
    isSuggestedAddition: isSuggested,
    isImportant: Boolean(raw.isImportant),
    reasons: Array.isArray(raw.reasons) ? raw.reasons : [],
    appName: raw.appName ? raw.appName.trim() : null,
    domain: raw.domain ? raw.domain.trim() : null,
    contentSnippet: snippet,
    status,
    createdAt: raw.createdAt || null,
    lastActiveAt: raw.lastActiveAt || null,
  };
}

// =========================================================================
// 6. Workset Partitioning & Quality Filtering
// =========================================================================

export interface WorksetPartitionResult {
  primaryItems: MinimalWorksetItem[];
  supportingItems: MinimalWorksetItem[];
  referenceItems: MinimalWorksetItem[];
  suggestedAdditions: MinimalWorksetItem[];
  totalItemCount: number;
}

/**
 * Partitions items into actionable workset tiers while enforcing quality rules:
 * - Excludes items with status 'deleted' or 'missing' from primary and supporting tiers.
 * - Limits primary items to top 5 to prevent cognitive overload.
 * - Ensures confirmed items take precedence over inferred items.
 */
export function partitionWorksetItems(items: MinimalWorksetItem[]): WorksetPartitionResult {
  const primaryItems: MinimalWorksetItem[] = [];
  const supportingItems: MinimalWorksetItem[] = [];
  const referenceItems: MinimalWorksetItem[] = [];
  const suggestedAdditions: MinimalWorksetItem[] = [];

  for (const item of items) {
    // Deleted or missing items cannot occupy primary or supporting focus
    if (item.status === 'deleted' || item.status === 'missing') {
      referenceItems.push(item);
      continue;
    }

    if (item.isSuggestedAddition || item.relevanceTier === 'suggested') {
      suggestedAdditions.push(item);
    } else if (item.relevanceTier === 'primary') {
      if (primaryItems.length < 5) {
        primaryItems.push(item);
      } else {
        supportingItems.push(item);
      }
    } else if (item.relevanceTier === 'supporting') {
      supportingItems.push(item);
    } else {
      referenceItems.push(item);
    }
  }

  return {
    primaryItems,
    supportingItems,
    referenceItems,
    suggestedAdditions,
    totalItemCount: items.length,
  };
}

// =========================================================================
// 7. Core Workset Derivation Builder (Offline, Deterministic, Zero AI)
// =========================================================================

export interface WorksetDerivationInput {
  contextId: string;
  contextName: string;
  lifecycleState?: ContextOperationalLifecycleState;
  worksetState?: WorksetState;
  activeTaskId?: string | null;
  activeTaskTitle?: string | null;
  taskConfidence?: number | null;
  rawItems: RawWorksetItemInput[];
  readinessBand?: string;
  readinessExplanation?: string | null;
  lastActiveAt?: string | null;
  recentActivitySummary?: string | null;
  aiEnrichment?: WorksetAiEnrichment | null;
}

/**
 * Synthesizes a MinimalContextWorkset deterministically without invoking any external AI.
 * Reuses existing context, task, session, and membership outputs.
 */
export function deriveMinimalWorkset(input: WorksetDerivationInput): MinimalContextWorkset {
  const contextId = (input.contextId || '').trim();
  const contextName = (input.contextName || '').trim() || 'Untitled Context';
  const lifecycleState = input.lifecycleState || 'active';
  const worksetState = input.worksetState || 'active';

  // 1. Normalize items and filter out unrelated noise
  const normalizedItems: MinimalWorksetItem[] = [];
  for (const raw of input.rawItems) {
    const item = normalizeWorksetItem(raw, contextId);

    // Rule 4: Do not treat every item in a workspace as automatically relevant.
    // An item is eligible if:
    // a) It is marked important, or
    // b) It is primary or supporting, or
    // c) It has task alignment or recent activity, or
    // d) Confidence >= 0.28
    const isExplicitlyImportant = item.isImportant;
    const isHighRelevanceTier = item.relevanceTier === 'primary' || item.relevanceTier === 'supporting';
    const hasSufficientConfidence = item.confidence >= 0.28;

    if (isExplicitlyImportant || isHighRelevanceTier || hasSufficientConfidence) {
      normalizedItems.push(item);
    }
  }

  // 2. Sort items: Confirmed first, then higher relevance tier, then confidence descending
  normalizedItems.sort((a, b) => {
    // Confirmed outranks inferred
    if (a.provenance === 'confirmed' && b.provenance === 'inferred') return -1;
    if (a.provenance === 'inferred' && b.provenance === 'confirmed') return 1;

    // Primary > Supporting > Reference > Recent > Suggested
    const tierWeight = (tier: WorksetItemRelevanceTier) => {
      switch (tier) {
        case 'primary': return 5;
        case 'supporting': return 4;
        case 'recent': return 3;
        case 'reference': return 2;
        case 'suggested': return 1;
      }
    };
    const diff = tierWeight(b.relevanceTier) - tierWeight(a.relevanceTier);
    if (diff !== 0) return diff;

    // Confidence descending
    const confDiff = b.confidence - a.confidence;
    if (Math.abs(confDiff) > 0.0001) return confDiff;

    // Recency descending
    const timeA = a.lastActiveAt || a.createdAt || '';
    const timeB = b.lastActiveAt || b.createdAt || '';
    if (timeA !== timeB) return timeB.localeCompare(timeA);

    // Stable tie-breaker: item id ascending
    return a.id.localeCompare(b.id);
  });

  // 3. Partition into categorized slices
  const partitions = partitionWorksetItems(normalizedItems);

  // 4. Overall Confidence & Band
  let overallConfidence = 0.50;
  if (partitions.primaryItems.length > 0) {
    overallConfidence = Math.max(overallConfidence, 0.85);
  } else if (normalizedItems.length > 0) {
    overallConfidence = Math.max(overallConfidence, 0.65);
  } else {
    overallConfidence = 0.20;
  }

  let confidenceBand = 'low';
  if (overallConfidence >= 0.80) confidenceBand = 'high';
  else if (overallConfidence >= 0.50) confidenceBand = 'medium';
  else if (overallConfidence > 0.0) confidenceBand = 'low';
  else confidenceBand = 'none';

  return {
    contextId,
    contextName,
    lifecycleState,
    worksetState,
    activeTaskId: input.activeTaskId || null,
    activeTaskTitle: input.activeTaskTitle || null,
    taskConfidence: input.taskConfidence ?? null,
    generatedAt: new Date().toISOString(),
    lastActiveAt: input.lastActiveAt || null,
    confidence: overallConfidence,
    confidenceBand,
    items: normalizedItems,
    primaryItems: partitions.primaryItems,
    supportingItems: partitions.supportingItems,
    referenceItems: partitions.referenceItems,
    suggestedAdditions: partitions.suggestedAdditions,
    totalItemCount: normalizedItems.length,
    readinessBand: input.readinessBand || (normalizedItems.length > 0 ? 'ready' : 'partial'),
    readinessExplanation: input.readinessExplanation || null,
    recentActivitySummary: input.recentActivitySummary || null,
    aiEnrichment: input.aiEnrichment || null,
  };
}

// =========================================================================
// 8. Bridge Adapter from Backend Engine DTO
// =========================================================================

export interface EngineWorksetAdapterOptions {
  lifecycleState?: ContextOperationalLifecycleState;
  readinessBand?: string;
  readinessExplanation?: string | null;
}

/**
 * Adapts the raw Rust backend `ContextWorkset` DTO into the strongly-typed `MinimalContextWorkset`.
 * Ensures full backward and forward compatibility without changing the Rust API.
 */
export function adaptEngineWorksetToMinimal(
  engineWorkset: ContextWorkset,
  options?: EngineWorksetAdapterOptions
): MinimalContextWorkset {
  const rawItems: RawWorksetItemInput[] = (engineWorkset.items || []).map((item) => ({
    id: item.id,
    title: item.title,
    itemType: item.item_type,
    contentSnippet: item.content_snippet,
    relevanceTier: item.relevance,
    confidence: item.relevance_score,
    isSuggestedAddition: item.is_suggested_addition,
    isImportant: item.relevance === 'primary',
    reasons: item.reasons,
    appName: item.app_name,
    domain: item.domain,
    createdAt: item.created_at,
    lastActiveAt: item.last_active_at,
    provenance: item.provenance === 'confirmed' ? 'confirmed' : 'inferred',
  }));

  let worksetState: WorksetState = 'active';
  if (engineWorkset.workset_state) {
    const s = engineWorkset.workset_state.toLowerCase();
    if (s === 'active') worksetState = 'active';
    else if (s === 'recently_active') worksetState = 'recently_active';
    else if (s === 'detour_return') worksetState = 'detour_return';
    else if (s === 'dormant') worksetState = 'dormant';
    else if (s === 'insufficient_evidence') worksetState = 'insufficient_evidence';
  }

  return deriveMinimalWorkset({
    contextId: engineWorkset.context_id,
    contextName: engineWorkset.context_name,
    lifecycleState: options?.lifecycleState || 'active',
    worksetState,
    activeTaskId: engineWorkset.task_id,
    activeTaskTitle: engineWorkset.task_title,
    taskConfidence: engineWorkset.confidence,
    rawItems,
    readinessBand: options?.readinessBand,
    readinessExplanation: options?.readinessExplanation || engineWorkset.explanation?.summary,
    lastActiveAt: engineWorkset.last_active_at,
    recentActivitySummary: engineWorkset.recent_activity,
  });
}

// =========================================================================
// 9. Context Launcher Presentation Adapter
// =========================================================================

/**
 * Presentation-ready slice designed specifically for the Context Launcher preview pane.
 * Keeps rendering bounded and performant.
 */
export interface LauncherWorksetSlice {
  contextId: string;
  contextName: string;
  activeTaskTitle: string | null;
  /** Primary focus items (max 3) */
  primaryFocus: MinimalWorksetItem[];
  /** Supporting and reference materials (max 3) */
  references: MinimalWorksetItem[];
  /** High-confidence suggested additions from Inbox (max 2) */
  suggestedAdditions: MinimalWorksetItem[];
  /** Total count of items in working set */
  totalCount: number;
  /** True if the working set has active items to display */
  hasItems: boolean;
}

/**
 * Creates a bounded presentation slice for the Context Launcher.
 */
export function createLauncherWorksetSlice(workset: MinimalContextWorkset): LauncherWorksetSlice {
  const primaryFocus = workset.primaryItems.slice(0, 3);
  const references = [...workset.supportingItems, ...workset.referenceItems].slice(0, 3);
  const suggestedAdditions = workset.suggestedAdditions.slice(0, 2);

  return {
    contextId: workset.contextId,
    contextName: workset.contextName,
    activeTaskTitle: workset.activeTaskTitle || null,
    primaryFocus,
    references,
    suggestedAdditions,
    totalCount: workset.totalItemCount,
    hasItems: workset.totalItemCount > 0,
  };
}

/**
 * Available user interaction actions on a workset item.
 */
export type WorksetItemActionType =
  | 'open'
  | 'copy'
  | 'toggle_importance'
  | 'remove_from_context'
  | 'accept_suggestion';

export interface WorksetItemActionDescriptor {
  actionType: WorksetItemActionType;
  label: string;
  isExecutable: boolean;
  unsupportedReason?: string;
}

/**
 * Resolves the primary click action for a workset item in the launcher.
 */
export function resolveWorksetItemAction(item: MinimalWorksetItem): WorksetItemActionDescriptor {
  if (item.status === 'deleted') {
    return {
      actionType: 'open',
      label: 'Deleted Item',
      isExecutable: false,
      unsupportedReason: 'Item has been deleted and cannot be opened.',
    };
  }

  if (item.status === 'missing') {
    return {
      actionType: 'open',
      label: 'Unavailable',
      isExecutable: false,
      unsupportedReason: 'Item is missing or unavailable.',
    };
  }

  if (item.isSuggestedAddition) {
    return {
      actionType: 'accept_suggestion',
      label: 'Accept into Context',
      isExecutable: true,
    };
  }

  if (item.itemType === 'url') {
    return {
      actionType: 'open',
      label: 'Open in Browser',
      isExecutable: true,
    };
  }

  if (item.itemType === 'file') {
    return {
      actionType: 'open',
      label: 'Open File',
      isExecutable: true,
    };
  }

  return {
    actionType: 'copy',
    label: 'Copy Note',
    isExecutable: true,
  };
}

// =========================================================================
// 10. Presentation Model Re-Exports
// =========================================================================

export {
  toWorksetPresentationItem,
  createWorksetPresentationModel,
  createEmptyWorksetPresentation,
} from './contextWorksetPresentation';

export type {
  WorksetPopulationState,
  WorksetPresentationItem,
  WorksetPresentationModel,
} from './contextWorksetPresentation';
