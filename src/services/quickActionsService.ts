import { contextInteractionService } from './contextInteractionService';
import { contextWorksetService } from './contextWorkset';
import { db } from './db';
import { useWorkspaceStore } from '../store/workspaceStore';
import {
  ContextActionId,
  ItemActionId,
  QuickActionResult,
  validateContextId,
  validateItemId,
  ContextActionEvaluationInput,
  ItemActionEvaluationInput,
  getAvailableContextActions,
  getAvailableItemActions,
  resolveContextLifecycleState,
  validateContextActionTransition,
  ContextOperationalLifecycleState,
} from '../models/quickActions';
import { validateContextName } from '../models/contextOrganization';

export interface ExecuteContextActionOptions {
  force?: boolean;
  lifecycleState?: string;
  readinessBand?: string;
  readinessExplanation?: string;
  newName?: string;
}

/**
 * Quick Actions Service
 *
 * Provides safe, validated execution of context and item quick actions
 * strictly delegating to real Phase 1 Context Engine operations and stores.
 */
export const quickActionsService = {
  /**
   * Executes an action targeting a Context entity.
   */
  async executeContextAction(
    actionId: ContextActionId,
    contextId: string,
    options?: ExecuteContextActionOptions
  ): Promise<QuickActionResult<ContextActionId>> {
    // 1. Mandatory ID validation
    const idValidation = validateContextId(contextId);
    if (!idValidation.isValid) {
      return {
        actionId,
        success: false,
        outcome: 'ERROR',
        error: idValidation.error,
        message: idValidation.error,
      };
    }

    const cleanContextId = contextId.trim();

    // 2. Revalidate live operational lifecycle state against live store
    const workspaceStore = useWorkspaceStore.getState();
    const liveWorkspaces = workspaceStore.workspaces;
    const currentActiveWorkspaceId = workspaceStore.activeWorkspaceId;
    const isCurrentlyActive = currentActiveWorkspaceId === cleanContextId;

    const liveWorkspace = liveWorkspaces?.find((w) => w.id === cleanContextId);

    let operationalState: ContextOperationalLifecycleState;
    if (liveWorkspace) {
      const isArchived = (liveWorkspace.lifecycle_state || '').toLowerCase() === 'archived';
      if (isArchived) {
        operationalState = 'archived';
      } else if (isCurrentlyActive) {
        operationalState = 'active';
      } else {
        const cached = (options?.lifecycleState || '').toLowerCase();
        if (cached === 'paused') {
          operationalState = 'paused';
        } else if (cached === 'dormant' || (liveWorkspace.lifecycle_state || '').toLowerCase() === 'dormant') {
          operationalState = 'dormant';
        } else {
          // If it was previously marked active in cached options but is no longer the active workspace,
          // it transitioned to paused
          operationalState = 'paused';
        }
      }
    } else {
      operationalState = resolveContextLifecycleState({
        lifecycleState: options?.lifecycleState,
        isActive: isCurrentlyActive || (options?.lifecycleState || '').toLowerCase() === 'active',
      });
    }

    // 3. Centralized Lifecycle Transition Revalidation
    const transitionValidation = validateContextActionTransition(actionId, operationalState);
    if (!transitionValidation.isValid) {
      if (actionId === 'context.delete') {
        return {
          actionId,
          success: false,
          outcome: 'UNSUPPORTED',
          message: 'Direct context deletion is not supported from the quick actions launcher.',
          resolvedContextId: cleanContextId,
        };
      }
      const isKnownLifecycleAction = ['context.resume', 'context.open_details', 'context.pause', 'context.archive'].includes(actionId);
      if (!isKnownLifecycleAction) {
        return {
          actionId,
          success: false,
          outcome: 'UNSUPPORTED',
          message: transitionValidation.reason || 'Action is unsupported in the launcher.',
          resolvedContextId: cleanContextId,
        };
      }
      return {
        actionId,
        success: false,
        outcome: 'BLOCKED',
        message: `Action is no longer valid: ${transitionValidation.reason}`,
        resolvedContextId: cleanContextId,
      };
    }

    switch (actionId) {
      // ---------------------------------------------------------------------
      // 1. Resume Context
      // ---------------------------------------------------------------------
      case 'context.resume': {
        // Readiness guard: require explicit force override if readiness is insufficient
        if (!options?.force && options?.readinessBand === 'insufficient') {
          const reason =
            options?.readinessExplanation ||
            'Context has insufficient recorded evidence to resume safely. Explicit confirmation required.';
          return {
            actionId,
            success: false,
            outcome: 'BLOCKED',
            message: reason,
            resolvedContextId: cleanContextId,
          };
        }

        try {
          // If context is archived in real operational state, unarchive it first to restore active lifecycle
          const isArchived = operationalState === 'archived' || (options?.lifecycleState || '').toLowerCase() === 'archived';
          if (isArchived) {
            await db.unarchiveWorkspace(cleanContextId);
          }

          // Emit resume user_action signal to Context Engine
          await contextInteractionService.resumeContext(cleanContextId);

          // Update workspace store selection
          useWorkspaceStore.getState().selectWorkspace(cleanContextId);

          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: isArchived
              ? `Unarchived and resumed context '${cleanContextId}'`
              : `Resumed context '${cleanContextId}'`,
            resolvedContextId: cleanContextId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          const isUnavailable =
            errMsg.toLowerCase().includes('not found') ||
            errMsg.toLowerCase().includes('does not exist') ||
            errMsg.toLowerCase().includes('unavailable') ||
            errMsg.toLowerCase().includes('no such') ||
            errMsg.toLowerCase().includes('deleted');
          const message = isUnavailable
            ? `Context '${cleanContextId}' is no longer available. It may have been closed or deleted.`
            : `Failed to resume context: ${errMsg}`;
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message,
            resolvedContextId: cleanContextId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 2. Open Context Details (Overview / Timeline)
      // ---------------------------------------------------------------------
      case 'context.open_details': {
        try {
          useWorkspaceStore.getState().openOverview(cleanContextId);
          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: `Opened overview for context '${cleanContextId}'`,
            resolvedContextId: cleanContextId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message: `Failed to open context details: ${errMsg}`,
            resolvedContextId: cleanContextId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 3. Rename Context
      // ---------------------------------------------------------------------
      case 'context.rename': {
        const nameValidation = validateContextName(options?.newName);
        if (!nameValidation.isValid) {
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: nameValidation.error,
            message: nameValidation.error,
            resolvedContextId: cleanContextId,
          };
        }

        const sanitizedName = nameValidation.sanitizedName!;

        try {
          // Safe rename execution via real db operation
          const updatedWorkspace = await db.updateWorkspace(cleanContextId, sanitizedName);

          const finalName = updatedWorkspace?.name || sanitizedName;
          const finalUpdatedAt = updatedWorkspace?.updated_at || new Date().toISOString();

          // Update workspace store state
          useWorkspaceStore.setState((state) => ({
            workspaces: state.workspaces.map((ws) =>
              ws.id === cleanContextId
                ? { ...ws, name: finalName, updated_at: finalUpdatedAt }
                : ws
            ),
          }));

          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: `Renamed context to "${sanitizedName}"`,
            resolvedContextId: cleanContextId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message: `Failed to rename context: ${errMsg}`,
            resolvedContextId: cleanContextId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 4. Pin Context
      // ---------------------------------------------------------------------
      case 'context.pin': {
        try {
          const updatedWorkspace = await db.setWorkspacePinned(cleanContextId, true);
          const finalUpdatedAt = updatedWorkspace?.updated_at || new Date().toISOString();

          // Update workspace store state
          useWorkspaceStore.setState((state) => ({
            workspaces: state.workspaces.map((ws) =>
              ws.id === cleanContextId
                ? { ...ws, is_pinned: true, updated_at: finalUpdatedAt }
                : ws
            ),
          }));

          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: 'Pinned context for quick access',
            resolvedContextId: cleanContextId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message: `Failed to pin context: ${errMsg}`,
            resolvedContextId: cleanContextId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 5. Unpin Context
      // ---------------------------------------------------------------------
      case 'context.unpin': {
        try {
          const updatedWorkspace = await db.setWorkspacePinned(cleanContextId, false);
          const finalUpdatedAt = updatedWorkspace?.updated_at || new Date().toISOString();

          // Update workspace store state
          useWorkspaceStore.setState((state) => ({
            workspaces: state.workspaces.map((ws) =>
              ws.id === cleanContextId
                ? { ...ws, is_pinned: false, updated_at: finalUpdatedAt }
                : ws
            ),
          }));

          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: 'Unpinned context',
            resolvedContextId: cleanContextId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message: `Failed to unpin context: ${errMsg}`,
            resolvedContextId: cleanContextId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 6. Pause Context
      // ---------------------------------------------------------------------
      case 'context.pause': {
        try {
          // Emit pause user_action signal to Context Engine
          await contextInteractionService.pauseContext(cleanContextId);

          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: `Paused context '${cleanContextId}'`,
            resolvedContextId: cleanContextId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message: `Failed to pause context: ${errMsg}`,
            resolvedContextId: cleanContextId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 4. Archive Context
      // ---------------------------------------------------------------------
      case 'context.archive': {
        try {
          // Safe archive execution via real db operation
          await db.archiveWorkspace(cleanContextId);

          // Safety check: If the archived workspace was currently active in UI, deselect it
          const currentActive = useWorkspaceStore.getState().activeWorkspaceId;
          if (currentActive === cleanContextId) {
            useWorkspaceStore.getState().selectWorkspace('inbox');
          }

          // Reload workspaces list in store
          await useWorkspaceStore.getState().loadWorkspaces();

          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: `Archived context '${cleanContextId}'`,
            resolvedContextId: cleanContextId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message: `Failed to archive context: ${errMsg}`,
            resolvedContextId: cleanContextId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 5. Restore Context
      // ---------------------------------------------------------------------
      case 'context.restore': {
        try {
          // Safe restore execution via real db operation
          await db.unarchiveWorkspace(cleanContextId);

          // Reload workspaces list in store
          await useWorkspaceStore.getState().loadWorkspaces();

          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: `Restored context '${cleanContextId}'`,
            resolvedContextId: cleanContextId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message: `Failed to restore context: ${errMsg}`,
            resolvedContextId: cleanContextId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 6. Unsupported Context Actions
      // ---------------------------------------------------------------------
      case 'context.delete': {
        return {
          actionId,
          success: false,
          outcome: 'UNSUPPORTED',
          message: 'Direct context deletion is not supported from the quick actions launcher.',
          resolvedContextId: cleanContextId,
        };
      }

      default: {
        return {
          actionId,
          success: false,
          outcome: 'UNSUPPORTED',
          message: `Action '${actionId}' is unsupported.`,
          resolvedContextId: cleanContextId,
        };
      }
    }
  },

  /**
   * Executes an action targeting an Item within a Context workset.
   */
  async executeItemAction(
    actionId: ItemActionId,
    contextId: string,
    itemId: string
  ): Promise<QuickActionResult<ItemActionId>> {
    // 1. Mandatory ID validation
    const contextValidation = validateContextId(contextId);
    if (!contextValidation.isValid) {
      return {
        actionId,
        success: false,
        outcome: 'ERROR',
        error: contextValidation.error,
        message: contextValidation.error,
      };
    }

    const itemValidation = validateItemId(itemId);
    if (!itemValidation.isValid) {
      return {
        actionId,
        success: false,
        outcome: 'ERROR',
        error: itemValidation.error,
        message: itemValidation.error,
        resolvedContextId: contextId.trim(),
      };
    }

    const cleanContextId = contextId.trim();
    const cleanItemId = itemId.trim();

    switch (actionId) {
      // ---------------------------------------------------------------------
      // 1. Mark as Important Memory
      // ---------------------------------------------------------------------
      case 'item.mark_important': {
        try {
          const success = await contextWorksetService.controlContextMemory(
            cleanContextId,
            cleanItemId,
            'keep_important'
          );
          if (!success) {
            return {
              actionId,
              success: false,
              outcome: 'ERROR',
              error: 'Context Engine rejected memory control operation.',
              message: 'Failed to mark item as important.',
              resolvedContextId: cleanContextId,
              targetItemId: cleanItemId,
            };
          }

          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: `Marked item '${cleanItemId}' as important memory in context '${cleanContextId}'`,
            resolvedContextId: cleanContextId,
            targetItemId: cleanItemId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message: `Failed to mark item as important: ${errMsg}`,
            resolvedContextId: cleanContextId,
            targetItemId: cleanItemId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 2. Unmark Important Memory
      // ---------------------------------------------------------------------
      case 'item.unmark_important': {
        try {
          const success = await contextWorksetService.controlContextMemory(
            cleanContextId,
            cleanItemId,
            'remove_important'
          );
          if (!success) {
            return {
              actionId,
              success: false,
              outcome: 'ERROR',
              error: 'Context Engine rejected memory control operation.',
              message: 'Failed to unmark item as important.',
              resolvedContextId: cleanContextId,
              targetItemId: cleanItemId,
            };
          }

          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: `Unmarked item '${cleanItemId}' from important memory in context '${cleanContextId}'`,
            resolvedContextId: cleanContextId,
            targetItemId: cleanItemId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message: `Failed to unmark item as important: ${errMsg}`,
            resolvedContextId: cleanContextId,
            targetItemId: cleanItemId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 3. Remove Item from Context Workset
      // ---------------------------------------------------------------------
      case 'item.remove_from_context': {
        try {
          const success = await contextWorksetService.controlContextMemory(
            cleanContextId,
            cleanItemId,
            'remove_from_context'
          );
          if (!success) {
            return {
              actionId,
              success: false,
              outcome: 'ERROR',
              error: 'Context Engine rejected remove_from_context operation.',
              message: 'Failed to remove item from context.',
              resolvedContextId: cleanContextId,
              targetItemId: cleanItemId,
            };
          }

          return {
            actionId,
            success: true,
            outcome: 'SUCCESS',
            message: `Removed item '${cleanItemId}' from context '${cleanContextId}' workset`,
            resolvedContextId: cleanContextId,
            targetItemId: cleanItemId,
          };
        } catch (err) {
          const errMsg = err instanceof Error ? err.message : String(err);
          return {
            actionId,
            success: false,
            outcome: 'ERROR',
            error: errMsg,
            message: `Failed to remove item from context: ${errMsg}`,
            resolvedContextId: cleanContextId,
            targetItemId: cleanItemId,
          };
        }
      }

      // ---------------------------------------------------------------------
      // 4. Unsupported Item Actions
      // ---------------------------------------------------------------------
      case 'item.delete': {
        return {
          actionId,
          success: false,
          outcome: 'UNSUPPORTED',
          message: 'Permanent item deletion is not supported from the quick actions launcher.',
          resolvedContextId: cleanContextId,
          targetItemId: cleanItemId,
        };
      }

      default: {
        return {
          actionId,
          success: false,
          outcome: 'UNSUPPORTED',
          message: `Action '${actionId}' is unsupported.`,
          resolvedContextId: cleanContextId,
          targetItemId: cleanItemId,
        };
      }
    }
  },

  /**
   * Evaluates context actions availability based on context state.
   */
  getAvailableContextActions(input: ContextActionEvaluationInput) {
    return getAvailableContextActions(input);
  },

  /**
   * Evaluates item actions availability based on item state.
   */
  getAvailableItemActions(input: ItemActionEvaluationInput) {
    return getAvailableItemActions(input);
  },
};
