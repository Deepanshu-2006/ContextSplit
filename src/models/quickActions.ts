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
  | 'context.rename'
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

export type ContextOperationalLifecycleState = 'active' | 'paused' | 'dormant' | 'archived';

export interface ContextLifecycleStateInput {
  lifecycleState?: string | null;
  isActive?: boolean;
}

/**
 * Resolves the operational lifecycle state of a context.
 * Normalizes between database lifecycle states and real-time active session states.
 */
export function resolveContextLifecycleState(input: ContextLifecycleStateInput): ContextOperationalLifecycleState {
  if (input.isActive) return 'active';
  const raw = (input.lifecycleState || '').trim().toLowerCase();
  if (raw === 'active') return 'active';
  if (raw === 'paused') return 'paused';
  if (raw === 'archived' || raw === 'inactive') return 'archived';
  return 'dormant';
}

export interface ContextActionTransitionValidation {
  isValid: boolean;
  reason?: string;
}

/**
 * Validates whether a context action transition is permissible from the context's current lifecycle state.
 * Encapsulates the domain rules of the Context Engine lifecycle model.
 */
export function validateContextActionTransition(
  actionId: ContextActionId,
  currentState: ContextOperationalLifecycleState
): ContextActionTransitionValidation {
  switch (actionId) {
    case 'context.resume':
      return { isValid: true };

    case 'context.open_details':
      return { isValid: true };

    case 'context.rename':
      return { isValid: true };

    case 'context.pause': {
      if (currentState === 'active') {
        return { isValid: true };
      }
      if (currentState === 'paused') {
        return {
          isValid: false,
          reason: 'Context is already paused and not currently active.',
        };
      }
      if (currentState === 'dormant') {
        return {
          isValid: false,
          reason: 'Cannot pause a dormant context.',
        };
      }
      if (currentState === 'archived') {
        return {
          isValid: false,
          reason: 'Cannot pause an archived context.',
        };
      }
      return {
        isValid: false,
        reason: 'Context is not currently active.',
      };
    }

    case 'context.archive': {
      if (currentState === 'archived') {
        return {
          isValid: false,
          reason: 'Context is already archived.',
        };
      }
      return { isValid: true };
    }

    case 'context.delete':
      return {
        isValid: false,
        reason: 'Context deletion must be performed from workspace management settings.',
      };

    default:
      return {
        isValid: false,
        reason: `Action '${actionId}' is unsupported.`,
      };
  }
}

export interface PrimaryContextActionDescriptor {
  actionId: ContextActionId;
  label: string;
  inProgressLabel: string;
  description: string;
  shortcut: string;
  isSupported: boolean;
  unsupportedReason?: string;
}

/**
 * Generates the authoritative primary action descriptor for a context given its lifecycle state.
 */
export function getPrimaryContextActionDescriptor(
  currentState: ContextOperationalLifecycleState
): PrimaryContextActionDescriptor {
  switch (currentState) {
    case 'active':
      return {
        actionId: 'context.resume',
        label: 'Focus Workspace',
        inProgressLabel: 'Focusing...',
        description: 'Switch to and focus this active workspace.',
        shortcut: '↵',
        isSupported: true,
      };
    case 'archived':
      return {
        actionId: 'context.resume',
        label: 'Restore & Resume',
        inProgressLabel: 'Restoring...',
        description: 'Unarchive and resume this context.',
        shortcut: '↵',
        isSupported: true,
      };
    case 'paused':
      return {
        actionId: 'context.resume',
        label: 'Resume Context',
        inProgressLabel: 'Resuming...',
        description: 'Resume work in this paused context.',
        shortcut: '↵',
        isSupported: true,
      };
    case 'dormant':
    default:
      return {
        actionId: 'context.resume',
        label: 'Resume Context',
        inProgressLabel: 'Resuming...',
        description: 'Resume work in this context and focus its workspace.',
        shortcut: '↵',
        isSupported: true,
      };
  }
}

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
  const currentState = resolveContextLifecycleState({
    lifecycleState: input.lifecycleState,
    isActive: input.isActive,
  });

  const actions: ContextQuickActionDescriptor[] = [];

  // 1. Primary Action: Resume Context / Focus Workspace / Restore & Resume
  const primaryDesc = getPrimaryContextActionDescriptor(currentState);
  actions.push({
    id: primaryDesc.actionId,
    label: primaryDesc.label,
    description: primaryDesc.description,
    targetType: 'context',
    isSupported: primaryDesc.isSupported,
    unsupportedReason: primaryDesc.unsupportedReason,
    shortcut: primaryDesc.shortcut,
  });

  // 2. Open Details / Overview
  const openDetailsValidation = validateContextActionTransition('context.open_details', currentState);
  actions.push({
    id: 'context.open_details',
    label: 'Open Details',
    description: 'Open full context overview, task goals, and timeline.',
    targetType: 'context',
    isSupported: openDetailsValidation.isValid,
    unsupportedReason: openDetailsValidation.reason,
    shortcut: '⌘O',
  });

  // 3. Rename Context
  const renameValidation = validateContextActionTransition('context.rename', currentState);
  actions.push({
    id: 'context.rename',
    label: 'Rename Context',
    description: 'Update the user-facing name of the context.',
    targetType: 'context',
    isSupported: renameValidation.isValid,
    unsupportedReason: renameValidation.reason,
    shortcut: '⌥R',
  });

  // 4. Pause Context
  const pauseValidation = validateContextActionTransition('context.pause', currentState);
  actions.push({
    id: 'context.pause',
    label: 'Pause Context',
    description: pauseValidation.isValid
      ? 'Signal that work in this context has paused.'
      : (pauseValidation.reason || 'Pause context is only valid for active contexts.'),
    targetType: 'context',
    isSupported: pauseValidation.isValid,
    unsupportedReason: pauseValidation.reason,
    shortcut: '⌥P',
  });

  // 4. Archive Context
  const archiveValidation = validateContextActionTransition('context.archive', currentState);
  actions.push({
    id: 'context.archive',
    label: 'Archive Context',
    description: archiveValidation.isValid
      ? 'Safely move this context to archived state.'
      : (archiveValidation.reason || 'Context is already archived.'),
    targetType: 'context',
    isSupported: archiveValidation.isValid,
    unsupportedReason: archiveValidation.reason,
  });

  // 5. Delete Context (Explicitly unsupported in launcher)
  const deleteValidation = validateContextActionTransition('context.delete', currentState);
  actions.push({
    id: 'context.delete',
    label: 'Delete Context',
    description: 'Permanent deletion is not supported from the quick actions launcher.',
    targetType: 'context',
    isSupported: false,
    unsupportedReason: deleteValidation.reason || 'Context deletion must be performed from workspace management settings.',
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
      description: 'Remove item from pinned working-set memories for this context. Does not affect parent context lifecycle or status.',
      targetType: 'item',
      isSupported: true,
    });
  } else {
    actions.push({
      id: 'item.mark_important',
      label: 'Mark as Important',
      description: 'Pin item as important working-set memory for this context. Does not affect parent context lifecycle or status.',
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
