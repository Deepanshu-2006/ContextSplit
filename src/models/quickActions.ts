/**
 * Context Launcher Quick Actions Contract
 *
 * Defines the typed interface, action descriptors, outcome models, and
 * validation rules for Context Launcher quick actions.
 *
 * Architectural Invariants:
 * 1. Strict separation between Context Actions and Item Actions.
 * 2. ID validation is mandatory before invoking any backend operations.
 * 3. Unsupported actions are explicitly represented with structured reasons.
 * 4. Zero invented backend operations; all supported actions delegate to real APIs.
 */

// =========================================================================
// Action Identifiers
// =========================================================================

/**
 * Operations that target a Context/Workspace entity.
 */
export type ContextActionId =
  | 'context.resume'
  | 'context.open_details'
  | 'context.pause'
  | 'context.archive'
  | 'context.delete'; // Explicitly unsupported action

/**
 * Operations that target an Item within a Context workset.
 */
export type ItemActionId =
  | 'item.mark_important'
  | 'item.unmark_important'
  | 'item.remove_from_context'
  | 'item.delete'; // Explicitly unsupported action

export type QuickActionTargetType = 'context' | 'item';

export type QuickActionOutcome = 'SUCCESS' | 'BLOCKED' | 'UNSUPPORTED' | 'ERROR';

// =========================================================================
// Action Descriptors & Metadata
// =========================================================================

export interface BaseQuickActionDescriptor<TActionId extends string> {
  id: TActionId;
  label: string;
  description: string;
  targetType: QuickActionTargetType;
  isSupported: boolean;
  unsupportedReason?: string;
  shortcut?: string;
}

export interface ContextQuickActionDescriptor extends BaseQuickActionDescriptor<ContextActionId> {
  targetType: 'context';
}

export interface ItemQuickActionDescriptor extends BaseQuickActionDescriptor<ItemActionId> {
  targetType: 'item';
}

export type QuickActionDescriptor = ContextQuickActionDescriptor | ItemQuickActionDescriptor;

// =========================================================================
// Execution Results
// =========================================================================

export interface QuickActionResult<TActionId extends string = string> {
  actionId: TActionId;
  success: boolean;
  outcome: QuickActionOutcome;
  message?: string;
  resolvedContextId?: string;
  targetItemId?: string;
  error?: string;
}

// =========================================================================
// Context & State Evaluation Models
// =========================================================================

export interface ContextActionEvaluationInput {
  contextId: string;
  lifecycleState?: 'active' | 'paused' | 'dormant' | 'archived' | string;
  isActive?: boolean;
  readinessBand?: 'ready' | 'mostly_ready' | 'partial' | 'insufficient' | string;
}

export interface ItemActionEvaluationInput {
  contextId: string;
  itemId: string;
  isImportant?: boolean;
}

// =========================================================================
// Validation Helpers
// =========================================================================

export interface ValidationResult {
  isValid: boolean;
  error?: string;
}

/**
 * Validates that a context ID is a non-empty, non-whitespace string.
 */
export function validateContextId(contextId: unknown): ValidationResult {
  if (typeof contextId !== 'string') {
    return { isValid: false, error: 'Context ID must be a string.' };
  }
  const trimmed = contextId.trim();
  if (trimmed.length === 0) {
    return { isValid: false, error: 'Context ID cannot be empty or whitespace.' };
  }
  return { isValid: true };
}

/**
 * Validates that an item ID is a non-empty, non-whitespace string.
 */
export function validateItemId(itemId: unknown): ValidationResult {
  if (typeof itemId !== 'string') {
    return { isValid: false, error: 'Item ID must be a string.' };
  }
  const trimmed = itemId.trim();
  if (trimmed.length === 0) {
    return { isValid: false, error: 'Item ID cannot be empty or whitespace.' };
  }
  return { isValid: true };
}

// =========================================================================
// Available Actions Registry Helpers
// =========================================================================

/**
 * Returns available context quick actions dynamically grounded in the context's operational state.
 */
export function getAvailableContextActions(input: ContextActionEvaluationInput): ContextQuickActionDescriptor[] {
  const isArchived = (input.lifecycleState || '').toLowerCase() === 'archived';
  const isActive = Boolean(input.isActive || (input.lifecycleState || '').toLowerCase() === 'active');
  const isDormant = (input.lifecycleState || '').toLowerCase() === 'dormant';

  const actions: ContextQuickActionDescriptor[] = [];

  // 1. Resume Context (Primary action)
  if (isActive) {
    actions.push({
      id: 'context.resume',
      label: 'Focus Workspace',
      description: 'Switch to and focus this active workspace.',
      targetType: 'context',
      isSupported: true,
      shortcut: '↵',
    });
  } else if (isArchived) {
    actions.push({
      id: 'context.resume',
      label: 'Restore & Resume',
      description: 'Unarchive and resume this context.',
      targetType: 'context',
      isSupported: true,
      shortcut: '↵',
    });
  } else {
    actions.push({
      id: 'context.resume',
      label: 'Resume Context',
      description: 'Resume work in this context and focus its workspace.',
      targetType: 'context',
      isSupported: true,
      shortcut: '↵',
    });
  }

  // 2. Open Details / Overview
  actions.push({
    id: 'context.open_details',
    label: 'Open Details',
    description: 'Open full context overview, task goals, and timeline.',
    targetType: 'context',
    isSupported: true,
    shortcut: '⌘O',
  });

  // 3. Pause Context (Only supported for active contexts)
  if (isActive) {
    actions.push({
      id: 'context.pause',
      label: 'Pause Context',
      description: 'Signal that work in this context has paused.',
      targetType: 'context',
      isSupported: true,
      shortcut: '⌥P',
    });
  } else {
    actions.push({
      id: 'context.pause',
      label: 'Pause Context',
      description: 'Pause context is only valid for active contexts.',
      targetType: 'context',
      isSupported: false,
      unsupportedReason: isArchived
        ? 'Cannot pause an archived context.'
        : isDormant
        ? 'Cannot pause a dormant context.'
        : 'Context is not currently active.',
    });
  }

  // 4. Archive Context
  if (isArchived) {
    actions.push({
      id: 'context.archive',
      label: 'Archive Context',
      description: 'Context is already archived.',
      targetType: 'context',
      isSupported: false,
      unsupportedReason: 'Context is already archived.',
    });
  } else {
    actions.push({
      id: 'context.archive',
      label: 'Archive Context',
      description: 'Safely move this context to archived state.',
      targetType: 'context',
      isSupported: true,
    });
  }

  // 5. Delete Context (Explicitly unsupported in launcher)
  actions.push({
    id: 'context.delete',
    label: 'Delete Context',
    description: 'Permanent deletion is not supported from the quick actions launcher.',
    targetType: 'context',
    isSupported: false,
    unsupportedReason: 'Context deletion must be performed from workspace management settings.',
  });

  return actions;
}

/**
 * Returns available item quick actions for an item within a context.
 */
export function getAvailableItemActions(input: ItemActionEvaluationInput): ItemQuickActionDescriptor[] {
  const isImportant = Boolean(input.isImportant);

  const actions: ItemQuickActionDescriptor[] = [];

  if (isImportant) {
    actions.push({
      id: 'item.unmark_important',
      label: 'Unmark Important',
      description: 'Remove item from pinned important contextual memories.',
      targetType: 'item',
      isSupported: true,
    });
  } else {
    actions.push({
      id: 'item.mark_important',
      label: 'Mark as Important',
      description: 'Pin item as permanent important contextual memory.',
      targetType: 'item',
      isSupported: true,
    });
  }

  actions.push({
    id: 'item.remove_from_context',
    label: 'Remove from Context',
    description: 'Disassociate item from this context workset without deleting the underlying item.',
    targetType: 'item',
    isSupported: true,
  });

  // Explicitly unsupported item action
  actions.push({
    id: 'item.delete',
    label: 'Delete Item',
    description: 'Permanent item deletion is not supported from the quick actions launcher.',
    targetType: 'item',
    isSupported: false,
    unsupportedReason: 'Permanent item deletion must be performed in the main inbox or canvas.',
  });

  return actions;
}
