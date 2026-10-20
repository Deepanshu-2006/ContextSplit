/**
 * Context Organization Contract
 *
 * Defines the typed interface, domain separation, action descriptors,
 * state-aware permissions, identity preservation rules, and validation logic
 * for organizing contexts (workspaces) in ContextSplit.
 *
 * Architectural Invariants:
 * 1. Strict separation between Organization Metadata, Operational Lifecycle, and Item Membership.
 * 2. Immutable context identity (`id: string` UUID) across all organization operations.
 * 3. Genuinely supported operations delegate to real backend APIs; unsupported operations
 *    (such as context pinning requiring unmigrated columns) are explicitly documented and blocked.
 * 4. Context Engine remains the definitive source of truth for operational state.
 * 5. Pinning, memory importance, lifecycle state, and workspace membership are never treated as interchangeable.
 */

import {
  resolveContextLifecycleState,
  validateContextId,
} from './quickActions';
import type {
  ContextOperationalLifecycleState,
  ValidationResult,
} from './quickActions';

// Re-export shared primitives to avoid duplicate definitions across the codebase
export {
  resolveContextLifecycleState,
  validateContextId,
};
export type {
  ContextOperationalLifecycleState,
  ValidationResult,
};

// =========================================================================
// 1. Organization Action Identifiers & Support Status
// =========================================================================

/**
 * All formal organization operations that can target a Context entity.
 */
export type ContextOrganizationActionId =
  | 'context.rename'
  | 'context.archive'
  | 'context.unarchive'
  | 'context.pin'
  | 'context.unpin'
  | 'context.favorite'
  | 'context.unfavorite'
  | 'context.delete'
  | 'context.set_metadata';

/**
 * Categorization of backend backing status for organization operations.
 */
export type OrganizationSupportStatus =
  | 'SUPPORTED'
  | 'UNSUPPORTED_REQUIRES_PERSISTENCE'
  | 'UNSUPPORTED_RESTRICTED';

/**
 * Standard outcome enumeration for organization actions.
 */
export type OrganizationActionOutcome =
  | 'ALLOWED'
  | 'BLOCKED'
  | 'UNSUPPORTED'
  | 'INVALID_INPUT';

/**
 * Detailed descriptor for an organization operation.
 */
export interface ContextOrganizationActionDescriptor {
  id: ContextOrganizationActionId;
  label: string;
  description: string;
  isSupported: boolean;
  supportStatus: OrganizationSupportStatus;
  unsupportedReason?: string;
  underlyingOperation?: string;
  requiresConfirmation?: boolean;
  hotkey?: string;
}

/**
 * Static registry of all organization operations with explicit verification of backend backing.
 * Grounded in docs/day-34-organization-audit.md.
 */
export const CONTEXT_ORGANIZATION_REGISTRY: Record<
  ContextOrganizationActionId,
  ContextOrganizationActionDescriptor
> = {
  'context.rename': {
    id: 'context.rename',
    label: 'Rename Context',
    description: 'Update the user-facing name of the workspace.',
    isSupported: true,
    supportStatus: 'SUPPORTED',
    underlyingOperation: 'db.updateWorkspace(id, name)',
    requiresConfirmation: false,
  },
  'context.archive': {
    id: 'context.archive',
    label: 'Archive Context',
    description: 'Move an inactive or dormant workspace into the archived state.',
    isSupported: true,
    supportStatus: 'SUPPORTED',
    underlyingOperation: 'db.archiveWorkspace(id)',
    requiresConfirmation: false,
    hotkey: '⌥A',
  },
  'context.unarchive': {
    id: 'context.unarchive',
    label: 'Restore Context',
    description: 'Restore an archived workspace back to active or dormant status.',
    isSupported: true,
    supportStatus: 'SUPPORTED',
    underlyingOperation: 'db.unarchiveWorkspace(id)',
    requiresConfirmation: false,
    hotkey: '↵',
  },
  'context.delete': {
    id: 'context.delete',
    label: 'Delete Workspace',
    description: 'Permanently remove a workspace and cascade-delete its associations.',
    isSupported: true,
    supportStatus: 'SUPPORTED',
    underlyingOperation: 'db.deleteWorkspace(id)',
    requiresConfirmation: true,
  },
  'context.pin': {
    id: 'context.pin',
    label: 'Pin Context',
    description: 'Elevate context to stay pinned at the top of the launcher and sidebar.',
    isSupported: true,
    supportStatus: 'SUPPORTED',
    underlyingOperation: 'db.setWorkspacePinned(id, true)',
    requiresConfirmation: false,
    hotkey: '⌥P',
  },
  'context.unpin': {
    id: 'context.unpin',
    label: 'Unpin Context',
    description: 'Remove context from the pinned group.',
    isSupported: true,
    supportStatus: 'SUPPORTED',
    underlyingOperation: 'db.setWorkspacePinned(id, false)',
    requiresConfirmation: false,
    hotkey: '⌥P',
  },
  'context.favorite': {
    id: 'context.favorite',
    label: 'Favorite Context',
    description: 'Mark context as a starred favorite for quick navigational access.',
    isSupported: false,
    supportStatus: 'UNSUPPORTED_RESTRICTED',
    unsupportedReason:
      'Context favoriting is omitted in favor of context pinning to avoid duplicate organizational concepts.',
  },
  'context.unfavorite': {
    id: 'context.unfavorite',
    label: 'Unfavorite Context',
    description: 'Remove context from starred favorites.',
    isSupported: false,
    supportStatus: 'UNSUPPORTED_RESTRICTED',
    unsupportedReason:
      'Context favoriting is omitted in favor of context pinning to avoid duplicate organizational concepts.',
  },
  'context.set_metadata': {
    id: 'context.set_metadata',
    label: 'Set Context Metadata',
    description: 'Assign project identifier, repository, or tags to a context.',
    isSupported: false,
    supportStatus: 'UNSUPPORTED_REQUIRES_PERSISTENCE',
    unsupportedReason:
      'Custom project and repository assignment on workspaces is not yet stored in SQLite. Context engine currently reads these opportunistically from window titles.',
  },
};

// =========================================================================
// 2. Conceptual Domain Separation
// =========================================================================

/**
 * Three strictly orthogonal domains govern ContextSplit. They MUST NOT be conflated:
 *
 * 1. ORGANIZATION: How the user labels, prioritizes, arranges, and curates contexts (name, pinning, favorites).
 * 2. LIFECYCLE: Operational state governed by Context Engine signals and activity decay (active, paused, dormant, archived).
 * 3. MEMBERSHIP: Associations of individual data items within a context workset, and memory retention weighting.
 */
export type ContextDomainScope = 'organization' | 'lifecycle' | 'membership';

/**
 * Pure organization metadata describing a context entity.
 */
export interface ContextOrganizationMetadata {
  /** Immutable primary key */
  id: string;
  /** Human-readable context title */
  name: string;
  /** Whether the context is pinned to top navigation (Gap: requires DB migration) */
  isPinned?: boolean;
  /** Codebase / Project grouping key (Inferred from engine signals) */
  projectIdentifier?: string | null;
  /** Git repository name or URI (Inferred from engine signals) */
  repository?: string | null;
  /** Creation timestamp in ISO-8601 */
  createdAt: string;
  /** Last modification timestamp in ISO-8601 */
  updatedAt: string;
}

/**
 * Pure operational lifecycle representation governed by the Context Engine.
 */
export interface ContextLifecycleStateInfo {
  contextId: string;
  state: ContextOperationalLifecycleState;
  enteredStateAt?: string;
  lastActiveAt?: string | null;
  archivedAt?: string | null;
  confidence?: number;
}

/**
 * Pure item membership and working set memory model.
 */
export interface ContextItemMembershipInfo {
  itemId: string;
  workspaceId: string;
  membershipType: 'confirmed' | 'suggested' | 'rejected';
  isImportant: boolean; // Governed by Context Engine important_memory service
  confidence: number;
}

/**
 * Evaluates which domain a given property or concept belongs to.
 */
export function getConceptDomain(concept: string): ContextDomainScope {
  const c = concept.toLowerCase().trim();
  switch (c) {
    case 'name':
    case 'rename':
    case 'pin':
    case 'pinning':
    case 'favorite':
    case 'favoriting':
    case 'sort_order':
    case 'project_identifier':
    case 'repository':
      return 'organization';

    case 'active':
    case 'paused':
    case 'dormant':
    case 'archived':
    case 'lifecycle':
    case 'resume':
    case 'pause':
    case 'decay':
    case 'freshness':
      return 'lifecycle';

    case 'item':
    case 'membership':
    case 'important':
    case 'mark_important':
    case 'workset':
    case 'memory':
      return 'membership';

    default:
      return 'organization';
  }
}

/**
 * Distinctness verification helper to enforce that pinning, importance,
 * lifecycle state, and membership are never treated as interchangeable.
 */
export function verifyConceptualBoundary(
  conceptA: string,
  conceptB: string
): { areDistinct: boolean; explanation: string } {
  const domainA = getConceptDomain(conceptA);
  const domainB = getConceptDomain(conceptB);

  if (domainA === domainB && conceptA.toLowerCase() === conceptB.toLowerCase()) {
    return {
      areDistinct: false,
      explanation: `'${conceptA}' and '${conceptB}' refer to the identical concept in the '${domainA}' domain.`,
    };
  }

  const explanations: Record<string, string> = {
    'pinning:importance':
      'Pinning operates on Contexts (elevating workspaces in navigation). Importance operates on Items (retaining specific items in context memory). They operate on different entity levels.',
    'pinning:lifecycle':
      'Pinning is a user navigation preference. Lifecycle is an operational runtime state (active/paused/dormant/archived). A pinned context can be active, paused, or dormant.',
    'pinning:membership':
      'Pinning prioritizes a context in UI lists. Membership links a data item to a workspace. Neither implies the other.',
    'importance:lifecycle':
      'Importance designates an item as high-priority context memory. Lifecycle designates workspace operational status. They are orthogonal.',
    'importance:membership':
      'Membership defines whether an item belongs to a workspace. Importance defines whether a member item is pinned in working memory.',
    'lifecycle:membership':
      'Lifecycle governs workspace operational freshness. Membership defines which items belong to the workspace.',
  };

  const key1 = `${conceptA.toLowerCase()}:${conceptB.toLowerCase()}`;
  const key2 = `${conceptB.toLowerCase()}:${conceptA.toLowerCase()}`;
  const explanation =
    explanations[key1] ||
    explanations[key2] ||
    `'${conceptA}' belongs to domain '${domainA}', while '${conceptB}' belongs to domain '${domainB}'. These domains have separate lifecycles, APIs, and persistence models.`;

  return {
    areDistinct: true,
    explanation,
  };
}

// =========================================================================
// 3. Context Identity Preservation Invariants
// =========================================================================

/**
 * Invariant rules governing context identity during organization operations:
 * - The primary key UUID must remain immutable.
 * - Renaming must never regenerate or mutate the context ID.
 * - Archiving or restoring must preserve all historical transitions and items.
 * - Deleting a context requires explicit confirmation and cascades cleanly.
 */
export const CONTEXT_IDENTITY_INVARIANTS = {
  IMMUTABLE_PRIMARY_KEY: 'Context ID is an immutable UUID that cannot be changed after creation.',
  MEMBERSHIP_PRESERVATION: 'Organization operations (rename, archive, restore) must preserve all existing item memberships and signals.',
  ACTIVE_SESSION_CONTINUITY: 'Renaming an active workspace must preserve the active session ID and activeWorkspaceId pointer.',
  RESTRICTED_ACTIVE_ARCHIVAL: 'An active context cannot be archived while in foreground focus without an explicit pause or switch transition.',
} as const;

/**
 * Asserts that context identity has been strictly preserved between two revisions of context metadata.
 */
export function assertContextIdentityPreserved(
  before: { id: string; name?: string },
  after: { id: string; name?: string }
): { isPreserved: boolean; violation?: string } {
  const idValidationBefore = validateContextId(before.id);
  if (!idValidationBefore.isValid) {
    return { isPreserved: false, violation: `Initial context ID is invalid: ${idValidationBefore.error}` };
  }

  const idValidationAfter = validateContextId(after.id);
  if (!idValidationAfter.isValid) {
    return { isPreserved: false, violation: `Updated context ID is invalid: ${idValidationAfter.error}` };
  }

  if (before.id !== after.id) {
    return {
      isPreserved: false,
      violation: `Identity mutation detected: Context ID changed from '${before.id}' to '${after.id}'. IDs must be immutable.`,
    };
  }

  return { isPreserved: true };
}

// =========================================================================
// 4. State-Aware Action Availability Matrix
// =========================================================================

export interface OrganizationTransitionValidation {
  isValid: boolean;
  outcome: OrganizationActionOutcome;
  reason?: string;
}

/**
 * Evaluates whether an organization action is permissible given the context's
 * current operational lifecycle state and active workspace status.
 */
export function validateOrganizationActionTransition(
  actionId: ContextOrganizationActionId,
  currentState: ContextOperationalLifecycleState,
  isCurrentlyActiveWorkspace: boolean = false
): OrganizationTransitionValidation {
  const descriptor = CONTEXT_ORGANIZATION_REGISTRY[actionId];
  if (!descriptor) {
    return {
      isValid: false,
      outcome: 'INVALID_INPUT',
      reason: `Unknown organization action: '${actionId}'.`,
    };
  }

  // 1. Check if operation is supported by the backend
  if (!descriptor.isSupported) {
    return {
      isValid: false,
      outcome: 'UNSUPPORTED',
      reason: descriptor.unsupportedReason || `Action '${actionId}' is not supported by the backend.`,
    };
  }

  // 2. State-aware rules
  switch (actionId) {
    case 'context.rename':
      // Renaming is valid across all states (active, paused, dormant, archived)
      return { isValid: true, outcome: 'ALLOWED' };

    case 'context.archive': {
      if (currentState === 'archived') {
        return {
          isValid: false,
          outcome: 'BLOCKED',
          reason: 'Context is already archived.',
        };
      }
      if (isCurrentlyActiveWorkspace || currentState === 'active') {
        return {
          isValid: false,
          outcome: 'BLOCKED',
          reason: 'Cannot archive the currently active workspace. Pause or switch to another context first.',
        };
      }
      return { isValid: true, outcome: 'ALLOWED' };
    }

    case 'context.unarchive': {
      if (currentState !== 'archived') {
        return {
          isValid: false,
          outcome: 'BLOCKED',
          reason: `Cannot unarchive context in '${currentState}' state. Only archived contexts can be restored.`,
        };
      }
      return { isValid: true, outcome: 'ALLOWED' };
    }

    case 'context.delete': {
      if (isCurrentlyActiveWorkspace) {
        return {
          isValid: false,
          outcome: 'BLOCKED',
          reason: 'Cannot delete the currently active workspace. Switch to another workspace or Inbox first.',
        };
      }
      return { isValid: true, outcome: 'ALLOWED' };
    }

    case 'context.pin':
    case 'context.unpin':
      return { isValid: true, outcome: 'ALLOWED' };

    case 'context.favorite':
    case 'context.unfavorite':
    case 'context.set_metadata':
      return {
        isValid: false,
        outcome: 'UNSUPPORTED',
        reason: descriptor.unsupportedReason,
      };

    default:
      return {
        isValid: false,
        outcome: 'UNSUPPORTED',
        reason: `Unsupported action: '${actionId}'.`,
      };
  }
}

// =========================================================================
// 5. Input Validation & Error Handling
// =========================================================================

export type OrganizationErrorCode =
  | 'INVALID_NAME'
  | 'EMPTY_CONTEXT_ID'
  | 'STATE_TRANSITION_BLOCKED'
  | 'OPERATION_UNSUPPORTED'
  | 'ACTIVE_CONTEXT_RESTRICTED'
  | 'IDENTITY_MUTATION_FORBIDDEN';

export interface ContextNameValidationResult extends ValidationResult {
  sanitizedName?: string;
  errorCode?: OrganizationErrorCode;
}

/**
 * Validates a user-supplied context/workspace name.
 * Enforces length bounds, non-empty content, and bans unprintable control characters.
 */
export function validateContextName(name: unknown): ContextNameValidationResult {
  if (typeof name !== 'string') {
    return {
      isValid: false,
      error: 'Context name must be a string.',
      errorCode: 'INVALID_NAME',
    };
  }

  const trimmed = name.trim();
  if (trimmed.length === 0) {
    return {
      isValid: false,
      error: 'Context name cannot be empty or whitespace only.',
      errorCode: 'INVALID_NAME',
    };
  }

  if (trimmed.length > 100) {
    return {
      isValid: false,
      error: 'Context name cannot exceed 100 characters.',
      errorCode: 'INVALID_NAME',
    };
  }

  // Reject newlines, tabs, and unprintable ASCII control characters
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1F\x7F]/.test(trimmed)) {
    return {
      isValid: false,
      error: 'Context name cannot contain control characters or newlines.',
      errorCode: 'INVALID_NAME',
    };
  }

  return {
    isValid: true,
    sanitizedName: trimmed,
  };
}

export interface ValidateOrganizationActionParams {
  actionId: ContextOrganizationActionId;
  contextId: unknown;
  currentState: ContextOperationalLifecycleState;
  isCurrentlyActiveWorkspace?: boolean;
  targetName?: unknown;
}

export interface OrganizationValidationResult {
  isValid: boolean;
  outcome: OrganizationActionOutcome;
  reason?: string;
  errorCode?: OrganizationErrorCode;
  sanitizedValue?: unknown;
}

/**
 * Comprehensive action validation uniting ID validation, input validation,
 * state transition checking, and backend support verification.
 */
export function validateOrganizationAction(
  params: ValidateOrganizationActionParams
): OrganizationValidationResult {
  // 1. Validate Context ID
  const idResult = validateContextId(params.contextId);
  if (!idResult.isValid) {
    return {
      isValid: false,
      outcome: 'INVALID_INPUT',
      reason: idResult.error,
      errorCode: 'EMPTY_CONTEXT_ID',
    };
  }

  // 2. Validate Action Transition Rules
  const transition = validateOrganizationActionTransition(
    params.actionId,
    params.currentState,
    params.isCurrentlyActiveWorkspace
  );
  if (!transition.isValid) {
    return {
      isValid: false,
      outcome: transition.outcome,
      reason: transition.reason,
      errorCode:
        transition.outcome === 'UNSUPPORTED'
          ? 'OPERATION_UNSUPPORTED'
          : params.isCurrentlyActiveWorkspace
          ? 'ACTIVE_CONTEXT_RESTRICTED'
          : 'STATE_TRANSITION_BLOCKED',
    };
  }

  // 3. For rename operations, validate the new name
  if (params.actionId === 'context.rename') {
    const nameResult = validateContextName(params.targetName);
    if (!nameResult.isValid) {
      return {
        isValid: false,
        outcome: 'INVALID_INPUT',
        reason: nameResult.error,
        errorCode: nameResult.errorCode,
      };
    }
    return {
      isValid: true,
      outcome: 'ALLOWED',
      sanitizedValue: nameResult.sanitizedName,
    };
  }

  return {
    isValid: true,
    outcome: 'ALLOWED',
  };
}

// =========================================================================
// 6. Backend Operation Mapping Helper
// =========================================================================

export interface BackendOperationMapping {
  supported: boolean;
  serviceMethod?: string;
  isSafe: boolean;
  requiresConfirmation: boolean;
  notes?: string;
}

/**
 * Maps an organization action to its definitive underlying service call.
 */
export function getBackendOperationMapping(
  actionId: ContextOrganizationActionId
): BackendOperationMapping {
  switch (actionId) {
    case 'context.rename':
      return {
        supported: true,
        serviceMethod: 'db.updateWorkspace(id, name)',
        isSafe: true,
        requiresConfirmation: false,
      };

    case 'context.archive':
      return {
        supported: true,
        serviceMethod: 'db.archiveWorkspace(id)',
        isSafe: true,
        requiresConfirmation: false,
        notes: 'Updates lifecycle_state to archived and records archived_at timestamp.',
      };

    case 'context.unarchive':
      return {
        supported: true,
        serviceMethod: 'db.unarchiveWorkspace(id)',
        isSafe: true,
        requiresConfirmation: false,
        notes: 'Restores lifecycle_state to active and clears archived_at timestamp.',
      };

    case 'context.delete':
      return {
        supported: true,
        serviceMethod: 'db.deleteWorkspace(id)',
        isSafe: false,
        requiresConfirmation: true,
        notes: 'Permanently deletes workspace and triggers CASCADE deletion on items and memberships.',
      };

    case 'context.pin':
      return {
        supported: true,
        serviceMethod: 'db.setWorkspacePinned(id, true)',
        isSafe: true,
        requiresConfirmation: false,
        notes: 'Updates is_pinned to 1 and records workspace_pinned user action signal.',
      };

    case 'context.unpin':
      return {
        supported: true,
        serviceMethod: 'db.setWorkspacePinned(id, false)',
        isSafe: true,
        requiresConfirmation: false,
        notes: 'Updates is_pinned to 0 and records workspace_unpinned user action signal.',
      };

    case 'context.favorite':
    case 'context.unfavorite':
      return {
        supported: false,
        isSafe: true,
        requiresConfirmation: false,
        notes: 'Requires persistence support in SQLite.',
      };

    case 'context.set_metadata':
      return {
        supported: false,
        isSafe: true,
        requiresConfirmation: false,
        notes: 'Requires project_identifier and repository columns in SQLite.',
      };

    default:
      return {
        supported: false,
        isSafe: false,
        requiresConfirmation: false,
      };
  }
}
