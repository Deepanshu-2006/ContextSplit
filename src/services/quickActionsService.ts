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
} from '../models/quickActions';

export interface ExecuteContextActionOptions {
  force?: boolean;
  lifecycleState?: string;
  readinessBand?: string;
  readinessExplanation?: string;
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
          // If context is archived, unarchive it first to restore active lifecycle
          const isArchived = (options?.lifecycleState || '').toLowerCase() === 'archived';
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
      // 3. Pause Context
      // ---------------------------------------------------------------------
      case 'context.pause': {
        // State guard: Cannot pause archived or dormant contexts
        const state = (options?.lifecycleState || '').toLowerCase();
        if (state === 'archived' || state === 'dormant') {
          return {
            actionId,
            success: false,
            outcome: 'BLOCKED',
            message: `Cannot pause context in '${state}' state. Only active contexts can be paused.`,
            resolvedContextId: cleanContextId,
          };
        }

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
        const isArchived = (options?.lifecycleState || '').toLowerCase() === 'archived';
        if (isArchived) {
          return {
            actionId,
            success: false,
            outcome: 'BLOCKED',
            message: `Context '${cleanContextId}' is already archived.`,
            resolvedContextId: cleanContextId,
          };
        }

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
      // 5. Unsupported Context Actions
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
