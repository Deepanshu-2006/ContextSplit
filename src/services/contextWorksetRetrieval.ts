/**
 * Context Workset Retrieval Service
 *
 * Lightweight, privacy-safe, deterministic retrieval service for Context Worksets
 * adhering strictly to the approved Context Workset Contract (docs/context-workset-contract.md).
 *
 * Invariants & Requirements:
 * 1. Reuses existing context, item, task, session, and relationship models.
 * 2. Retrieves data through existing services and indexed repositories.
 * 3. Starts with explicitly associated or user-confirmed items.
 * 4. Includes inferred associations only when the existing architecture provides
 *    reliable evidence (confidence >= threshold) and clear epistemic distinction.
 * 5. Uses existing relevance and calibrated confidence information.
 * 6. Avoids retrieving unrelated items simply because they share a workspace.
 * 7. Preserves stable context and item IDs.
 * 8. Handles missing, deleted, archived, or unavailable records safely.
 * 9. Avoids duplicate items when multiple relationships point to the same item.
 * 10. Uses deterministic ordering with clear tie-breaking rules.
 * 11. Avoids expensive full-database scans using targeted indexed queries.
 * 12. Never invokes AI to perform basic retrieval.
 * 13. Keeps item contents out of logs (privacy-safe telemetry).
 * 14. Completely independent of React UI.
 */

import { db, Workspace, Item, ContextMembership } from './db';
import { contextWorksetService } from './contextWorkset';
import { contextInteractionService } from './contextInteractionService';
import {
  MinimalContextWorkset,
  RawWorksetItemInput,
  deriveMinimalWorkset,
  adaptEngineWorksetToMinimal,
} from '../models/contextWorkset';
import { validateContextId } from '../models/quickActions';

// =========================================================================
// Types & Options
// =========================================================================

export interface WorksetRetrievalOptions {
  /**
   * Minimum confidence threshold for admitting inferred/suggested associations.
   * Default: 0.60 (per Phase 1 engine specification)
   */
  minInferredConfidence?: number;
  /**
   * Minimum relevance score for workspace items without explicit confirmation.
   * Default: 0.28
   */
  minWorkspaceRelevanceScore?: number;
  /**
   * Maximum primary items to admit into active focus.
   * Default: 5
   */
  maxPrimaryItems?: number;
  /**
   * Prefer direct database repository queries over Context Engine IPC.
   * Default: false
   */
  preferRepository?: boolean;
  /**
   * Optional custom logger for privacy-safe telemetry/diagnostics.
   */
  logger?: (meta: WorksetRetrievalLogMeta) => void;
}

export interface WorksetRetrievalLogMeta {
  contextId: string;
  durationMs: number;
  source: 'engine' | 'repository' | 'fallback';
  totalItems: number;
  primaryCount: number;
  supportingCount: number;
  referenceCount: number;
  suggestedCount: number;
  status: 'success' | 'not_found' | 'archived' | 'error';
  errorMessage?: string;
}

// =========================================================================
// Privacy-Safe Logging (Requirement 13)
// =========================================================================

/**
 * Emits privacy-safe structured diagnostics.
 * Invariant: Never logs item content, text snippets, titles, or user notes.
 */
function defaultWorksetLogger(meta: WorksetRetrievalLogMeta): void {
  // Only IDs, counts, durations, and status codes are allowed in logs
  if (meta.status === 'error') {
    console.warn(
      `[WorksetRetrieval] Error for context '${meta.contextId}' (${meta.source}): ${meta.errorMessage || 'unknown'}`
    );
  } else {
    // Debug level to avoid noisy stdout in production
    // console.debug(
    //   `[WorksetRetrieval] ${meta.contextId} [${meta.source}]: ${meta.totalItems} items (${meta.durationMs.toFixed(1)}ms)`
    // );
  }
}

// =========================================================================
// Helper Utilities
// =========================================================================

function safeParseMetadata(raw: string | null | undefined): Record<string, any> {
  if (!raw || typeof raw !== 'string') return {};
  try {
    const parsed = JSON.parse(raw);
    return typeof parsed === 'object' && parsed !== null ? parsed : {};
  } catch {
    return {};
  }
}

// =========================================================================
// Retrieval Implementations
// =========================================================================

/**
 * Retrieves the Context Workset using direct targeted repository queries against persisted SQLite tables.
 *
 * Indexed Queries:
 * - `db.getWorkspace(contextId)` (PK lookup)
 * - `db.listWorkspaceMemberships(contextId)` (uses `idx_context_memberships_ws_type_conf`)
 * - `db.listItems(contextId)` (uses `idx_items_workspace_created`)
 * - `db.getItem(itemId)` (targeted PK lookup for external/inbox suggested items)
 *
 * Invariants Enforced:
 * - Confirmed memberships prioritized over inferred suggestions.
 * - Inferred items admitted only if confidence >= minInferredConfidence (default 0.60).
 * - Stale, unrelated workspace clutter excluded (anti-accumulation).
 * - Deduplication by item ID with confirmed status winning conflicts.
 * - Deterministic tie-breaking on ordering.
 * - Missing/deleted items handled predictably without throwing.
 */
export async function retrieveFromRepository(
  contextId: string,
  options?: WorksetRetrievalOptions
): Promise<MinimalContextWorkset | null> {
  const start = performance.now();
  const log = options?.logger || defaultWorksetLogger;

  // 1. Mandatory Context ID Validation
  const idValidation = validateContextId(contextId);
  if (!idValidation.isValid) {
    log({
      contextId: contextId || 'invalid',
      durationMs: performance.now() - start,
      source: 'repository',
      totalItems: 0,
      primaryCount: 0,
      supportingCount: 0,
      referenceCount: 0,
      suggestedCount: 0,
      status: 'error',
      errorMessage: idValidation.error,
    });
    return null;
  }

  const cleanContextId = contextId.trim();

  // 2. Retrieve Workspace Record
  let workspace: Workspace | null = null;
  try {
    workspace = await db.getWorkspace(cleanContextId);
  } catch (err: any) {
    log({
      contextId: cleanContextId,
      durationMs: performance.now() - start,
      source: 'repository',
      totalItems: 0,
      primaryCount: 0,
      supportingCount: 0,
      referenceCount: 0,
      suggestedCount: 0,
      status: 'not_found',
      errorMessage: err?.message || 'Workspace not found',
    });
    return null;
  }

  if (!workspace) {
    log({
      contextId: cleanContextId,
      durationMs: performance.now() - start,
      source: 'repository',
      totalItems: 0,
      primaryCount: 0,
      supportingCount: 0,
      referenceCount: 0,
      suggestedCount: 0,
      status: 'not_found',
    });
    return null;
  }

  // 3. Handle Archived Contexts Safely (Requirement 8)
  if (workspace.lifecycle_state === 'archived') {
    const archivedWorkset: MinimalContextWorkset = {
      contextId: cleanContextId,
      contextName: workspace.name,
      lifecycleState: 'archived',
      worksetState: 'dormant',
      activeTaskId: null,
      activeTaskTitle: null,
      taskConfidence: null,
      generatedAt: new Date().toISOString(),
      lastActiveAt: workspace.last_activity_at || null,
      confidence: 1.0,
      confidenceBand: 'high',
      items: [],
      primaryItems: [],
      supportingItems: [],
      referenceItems: [],
      suggestedAdditions: [],
      totalItemCount: 0,
      readinessBand: 'blocked',
      readinessExplanation: 'Context is archived and must be unarchived before resuming.',
      recentActivitySummary: null,
      aiEnrichment: null,
    };

    log({
      contextId: cleanContextId,
      durationMs: performance.now() - start,
      source: 'repository',
      totalItems: 0,
      primaryCount: 0,
      supportingCount: 0,
      referenceCount: 0,
      suggestedCount: 0,
      status: 'archived',
    });

    return archivedWorkset;
  }

  // 4. Resolve Active Task Grounding
  let activeTaskId: string | null = null;
  let activeTaskTitle: string | null = null;
  let taskConfidence: number | null = null;

  try {
    const lastTask = await db.getLastActiveTask(cleanContextId).catch(() => null);
    if (lastTask && lastTask.is_current) {
      activeTaskId = lastTask.task_id || null;
      activeTaskTitle = lastTask.title;
      taskConfidence = lastTask.confidence;
    } else {
      const memory = await db.getContextMemory(cleanContextId).catch(() => null);
      if (memory && memory.recent_task && memory.recent_task.title) {
        activeTaskId = memory.recent_task.id || null;
        activeTaskTitle = memory.recent_task.title;
        taskConfidence = memory.recent_task.confidence || null;
      } else if (memory && memory.task) {
        activeTaskTitle = memory.task;
      }
    }
  } catch {
    // Non-blocking task grounding fallback
  }

  // 5. Query Targeted Indexes (Requirements 2 & 11)
  const minInferredConfidence = options?.minInferredConfidence ?? 0.60;
  const minScore = options?.minWorkspaceRelevanceScore ?? 0.28;

  const [memberships, physicalItems] = await Promise.all([
    db.listWorkspaceMemberships(cleanContextId).catch(() => [] as ContextMembership[]),
    db.listItems(cleanContextId).catch(() => [] as Item[]),
  ]);

  // 6. Accumulate and Deduplicate Workset Items (Requirements 3, 4, 6, 9)
  const itemsMap = new Map<string, RawWorksetItemInput>();
  const membershipByItemId = new Map<string, ContextMembership>();

  // Index active valid memberships
  for (const m of memberships) {
    if (m.membership_type === 'dismissed') continue;
    // Inferred associations require reliable evidence (Requirement 4)
    if (m.membership_type === 'suggested' && m.confidence < minInferredConfidence) continue;

    const existing = membershipByItemId.get(m.item_id);
    if (!existing) {
      membershipByItemId.set(m.item_id, m);
    } else {
      // Confirmed wins over suggested; highest confidence preserved
      if (m.membership_type === 'confirmed' || m.confidence > existing.confidence) {
        membershipByItemId.set(m.item_id, m);
      }
    }
  }

  // Process physical items stored in this workspace
  for (const item of physicalItems) {
    const meta = safeParseMetadata(item.metadata);
    const isDeleted = Boolean(meta.is_deleted || meta.deleted || meta.status === 'deleted');
    const isImportant = Boolean(meta.isImportant || meta.is_important || meta.pinned);
    const isTaskAligned = Boolean(
      activeTaskId && (meta.task_id === activeTaskId || meta.taskId === activeTaskId)
    );

    const membership = membershipByItemId.get(item.id);

    // Requirement 6: Avoid retrieving unrelated items simply because they share a workspace.
    const hasExplicitMembership = membership && membership.membership_type === 'confirmed';
    const hasSuggestedMembership = membership && membership.membership_type === 'suggested';
    const hasRecentActivity = typeof meta.relevance_score === 'number' && meta.relevance_score >= minScore;

    // Item is admitted if confirmed, task-aligned, explicitly important, has suggested link, or meets minimum score
    if (!hasExplicitMembership && !isImportant && !isTaskAligned && !hasSuggestedMembership && !hasRecentActivity) {
      continue;
    }

    const isPhysicalWorkspaceItem = item.workspace_id === cleanContextId;
    const isSuggested = !isPhysicalWorkspaceItem && Boolean(hasSuggestedMembership);
    const provenance: 'confirmed' | 'inferred' = isSuggested ? 'inferred' : 'confirmed';

    let relevanceTier: 'primary' | 'supporting' | 'reference' | 'suggested' = 'reference';
    if (isSuggested) {
      relevanceTier = 'suggested';
    } else if (isTaskAligned) {
      relevanceTier = 'primary';
    } else if (isImportant) {
      relevanceTier = 'supporting';
    }

    itemsMap.set(item.id, {
      id: item.id,
      title: isDeleted ? '[Deleted Item]' : item.title,
      itemType: item.item_type,
      content: item.content,
      contentSnippet: item.content ? item.content.slice(0, 120) : null,
      workspaceId: item.workspace_id,
      relevanceTier,
      confidence: membership ? membership.confidence : (isTaskAligned ? 0.95 : (isImportant ? 0.90 : 0.75)),
      isSuggestedAddition: isSuggested,
      isImportant,
      status: isDeleted ? 'deleted' : 'available',
      isDeleted,
      provenance,
      createdAt: item.created_at,
      lastActiveAt: item.updated_at,
    });
  }

  // Process remaining memberships not physically in physicalItems (e.g., Inbox suggestions, multi-context items)
  for (const [itemId, membership] of membershipByItemId.entries()) {
    if (itemsMap.has(itemId)) {
      // If already present, ensure confirmed status takes precedence
      const existing = itemsMap.get(itemId)!;
      if (membership.membership_type === 'confirmed') {
        existing.provenance = 'confirmed';
        existing.isSuggestedAddition = false;
        if (existing.relevanceTier === 'suggested') {
          existing.relevanceTier = 'reference';
        }
      }
      existing.confidence = Math.max(existing.confidence ?? 0, membership.confidence);
      continue;
    }

    // Retrieve the external / unfiled item safely
    let itemRecord: Item | null = null;
    try {
      itemRecord = await db.getItem(itemId);
    } catch {
      // Record unavailable or corrupted (Requirement 8)
      itemRecord = null;
    }

    if (!itemRecord) {
      // Handle unavailable/deleted records safely without throwing
      itemsMap.set(itemId, {
        id: itemId,
        title: '[Unavailable Item]',
        itemType: 'text',
        status: 'missing',
        confidence: membership.confidence,
        provenance: membership.membership_type === 'confirmed' ? 'confirmed' : 'inferred',
        isSuggestedAddition: membership.membership_type === 'suggested',
        relevanceTier: membership.membership_type === 'suggested' ? 'suggested' : 'reference',
        workspaceId: null,
        createdAt: membership.created_at,
        lastActiveAt: membership.updated_at,
      });
      continue;
    }

    const meta = safeParseMetadata(itemRecord.metadata);
    const isDeleted = Boolean(meta.is_deleted || meta.deleted || meta.status === 'deleted');
    const isImportant = Boolean(meta.isImportant || meta.is_important || meta.pinned);
    const isPhysicalWorkspaceItem = itemRecord.workspace_id === cleanContextId;
    const isSuggested = !isPhysicalWorkspaceItem && membership.membership_type === 'suggested';
    const provenance: 'confirmed' | 'inferred' = isSuggested ? 'inferred' : 'confirmed';

    itemsMap.set(itemId, {
      id: itemRecord.id,
      title: isDeleted ? '[Deleted Item]' : itemRecord.title,
      itemType: itemRecord.item_type,
      content: itemRecord.content,
      contentSnippet: itemRecord.content ? itemRecord.content.slice(0, 120) : null,
      workspaceId: itemRecord.workspace_id,
      relevanceTier: isSuggested ? 'suggested' : (isImportant ? 'supporting' : 'reference'),
      confidence: membership.confidence,
      isSuggestedAddition: isSuggested,
      isImportant,
      status: isDeleted ? 'deleted' : 'available',
      isDeleted,
      provenance,
      createdAt: itemRecord.created_at,
      lastActiveAt: itemRecord.updated_at,
    });
  }

  // 7. Derive Minimal Workset with Deterministic Tie-Breaking (Requirements 9 & 10)
  const rawItems = Array.from(itemsMap.values());
  const minimalWorkset = deriveMinimalWorkset({
    contextId: cleanContextId,
    contextName: workspace.name,
    lifecycleState: workspace.lifecycle_state || 'active',
    worksetState: rawItems.length === 0 ? 'insufficient_evidence' : 'active',
    activeTaskId,
    activeTaskTitle,
    taskConfidence,
    rawItems,
    lastActiveAt: workspace.last_activity_at,
    readinessBand: rawItems.length === 0 ? 'low' : 'ready',
    readinessExplanation: rawItems.length === 0
      ? 'Context workspace is empty. Add or capture items to build a workset.'
      : undefined,
  });

  // 8. Privacy-Safe Logging (Requirement 13)
  log({
    contextId: cleanContextId,
    durationMs: performance.now() - start,
    source: 'repository',
    totalItems: minimalWorkset.totalItemCount,
    primaryCount: minimalWorkset.primaryItems.length,
    supportingCount: minimalWorkset.supportingItems.length,
    referenceCount: minimalWorkset.referenceItems.length,
    suggestedCount: minimalWorkset.suggestedAdditions.length,
    status: 'success',
  });

  return minimalWorkset;
}

/**
 * Retrieves the Context Workset through the Rust Context Engine service via Tauri IPC.
 */
export async function retrieveFromEngine(
  contextId: string,
  options?: WorksetRetrievalOptions
): Promise<MinimalContextWorkset | null> {
  const start = performance.now();
  const log = options?.logger || defaultWorksetLogger;

  const idValidation = validateContextId(contextId);
  if (!idValidation.isValid) {
    log({
      contextId: contextId || 'invalid',
      durationMs: performance.now() - start,
      source: 'engine',
      totalItems: 0,
      primaryCount: 0,
      supportingCount: 0,
      referenceCount: 0,
      suggestedCount: 0,
      status: 'error',
      errorMessage: idValidation.error,
    });
    return null;
  }

  const cleanContextId = contextId.trim();

  try {
    let engineWorkset: any = null;
    try {
      engineWorkset = await contextInteractionService.getContextWorkset(cleanContextId);
    } catch {
      engineWorkset = await contextWorksetService.getContextWorkset(cleanContextId);
    }

    if (!engineWorkset) {
      log({
        contextId: cleanContextId,
        durationMs: performance.now() - start,
        source: 'engine',
        totalItems: 0,
        primaryCount: 0,
        supportingCount: 0,
        referenceCount: 0,
        suggestedCount: 0,
        status: 'not_found',
      });
      return null;
    }

    const minimal = adaptEngineWorksetToMinimal(engineWorkset);

    log({
      contextId: cleanContextId,
      durationMs: performance.now() - start,
      source: 'engine',
      totalItems: minimal.totalItemCount,
      primaryCount: minimal.primaryItems.length,
      supportingCount: minimal.supportingItems.length,
      referenceCount: minimal.referenceItems.length,
      suggestedCount: minimal.suggestedAdditions.length,
      status: 'success',
    });

    return minimal;
  } catch (err: any) {
    log({
      contextId: cleanContextId,
      durationMs: performance.now() - start,
      source: 'engine',
      totalItems: 0,
      primaryCount: 0,
      supportingCount: 0,
      referenceCount: 0,
      suggestedCount: 0,
      status: 'error',
      errorMessage: err?.message || 'Context Engine retrieval error',
    });
    return null;
  }
}

/**
 * Main retrieval entrypoint: Tries Context Engine IPC first, gracefully falling back
 * to direct repository queries if the engine is unavailable or unpopulated.
 */
export async function retrieveContextWorkset(
  contextId: string,
  options?: WorksetRetrievalOptions
): Promise<MinimalContextWorkset | null> {
  if (options?.preferRepository) {
    return retrieveFromRepository(contextId, options);
  }

  const fromEngine = await retrieveFromEngine(contextId, options);
  if (fromEngine) {
    return fromEngine;
  }

  // Graceful fallback to repository retrieval
  return retrieveFromRepository(contextId, {
    ...options,
    logger: (meta) => {
      const log = options?.logger || defaultWorksetLogger;
      log({ ...meta, source: 'fallback' });
    },
  });
}

export const contextWorksetRetrievalService = {
  retrieveContextWorkset,
  retrieveFromRepository,
  retrieveFromEngine,
};
