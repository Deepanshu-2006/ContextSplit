import { describe, it, expect, vi, beforeEach } from 'vitest';
import { quickActionsService } from './quickActionsService';
import {
  validateContextId,
  validateItemId,
  getAvailableContextActions,
  getAvailableItemActions,
} from '../models/quickActions';
import { contextInteractionService } from './contextInteractionService';
import { contextWorksetService } from './contextWorkset';
import { db } from './db';
import { useWorkspaceStore } from '../store/workspaceStore';

vi.mock('./contextInteractionService', () => ({
  contextInteractionService: {
    resumeContext: vi.fn(),
    pauseContext: vi.fn(),
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    getResumeReadiness: vi.fn(),
  },
}));

vi.mock('./contextWorkset', () => ({
  contextWorksetService: {
    controlContextMemory: vi.fn(),
  },
}));

vi.mock('./db', () => ({
  db: {
    archiveWorkspace: vi.fn(),
    unarchiveWorkspace: vi.fn(),
  },
}));

describe('Quick Actions Contract & Service', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWorkspaceStore.setState({
      workspaces: [],
      activeWorkspaceId: null,
      loadWorkspaces: vi.fn().mockResolvedValue(undefined),
      selectWorkspace: vi.fn(),
      openOverview: vi.fn(),
    });
  });

  // =========================================================================
  // 1. Validation Requirements
  // =========================================================================
  describe('ID Validation', () => {
    it('validates context ID correctly', () => {
      expect(validateContextId(null).isValid).toBe(false);
      expect(validateContextId(undefined).isValid).toBe(false);
      expect(validateContextId('').isValid).toBe(false);
      expect(validateContextId('   ').isValid).toBe(false);
      expect(validateContextId(123).isValid).toBe(false);
      expect(validateContextId('ctx-valid-1').isValid).toBe(true);
    });

    it('validates item ID correctly', () => {
      expect(validateItemId(null).isValid).toBe(false);
      expect(validateItemId(undefined).isValid).toBe(false);
      expect(validateItemId('').isValid).toBe(false);
      expect(validateItemId('   ').isValid).toBe(false);
      expect(validateItemId({}).isValid).toBe(false);
      expect(validateItemId('item-valid-1').isValid).toBe(true);
    });

    it('rejects context action if context ID is invalid without calling backend', async () => {
      const result = await quickActionsService.executeContextAction('context.resume', '');
      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.error).toContain('Context ID cannot be empty');
      expect(contextInteractionService.resumeContext).not.toHaveBeenCalled();
    });

    it('rejects item action if item ID or context ID is invalid without calling backend', async () => {
      const res1 = await quickActionsService.executeItemAction('item.mark_important', '', 'item-1');
      expect(res1.success).toBe(false);
      expect(res1.outcome).toBe('ERROR');
      expect(contextWorksetService.controlContextMemory).not.toHaveBeenCalled();

      const res2 = await quickActionsService.executeItemAction('item.mark_important', 'ctx-1', '  ');
      expect(res2.success).toBe(false);
      expect(res2.outcome).toBe('ERROR');
      expect(contextWorksetService.controlContextMemory).not.toHaveBeenCalled();
    });
  });

  // =========================================================================
  // 2. Context Actions
  // =========================================================================
  describe('Context Actions Execution', () => {
    describe('context.resume', () => {
      it('blocks resumption when readiness is insufficient and force is false', async () => {
        const result = await quickActionsService.executeContextAction('context.resume', 'ctx-insufficient', {
          readinessBand: 'insufficient',
          readinessExplanation: 'No recent workset items found.',
          force: false,
        });

        expect(result.success).toBe(false);
        expect(result.outcome).toBe('BLOCKED');
        expect(result.message).toContain('No recent workset items found');
        expect(contextInteractionService.resumeContext).not.toHaveBeenCalled();
      });

      it('resumes successfully when readiness is insufficient but force is true', async () => {
        vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ status: 'ok' });

        const result = await quickActionsService.executeContextAction('context.resume', 'ctx-insufficient', {
          readinessBand: 'insufficient',
          force: true,
        });

        expect(result.success).toBe(true);
        expect(result.outcome).toBe('SUCCESS');
        expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-insufficient');
        expect(useWorkspaceStore.getState().selectWorkspace).toHaveBeenCalledWith('ctx-insufficient');
      });

      it('resumes normal context and updates workspace store', async () => {
        vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ status: 'ok' });

        const result = await quickActionsService.executeContextAction('context.resume', 'ctx-normal', {
          readinessBand: 'ready',
        });

        expect(result.success).toBe(true);
        expect(result.outcome).toBe('SUCCESS');
        expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-normal');
        expect(useWorkspaceStore.getState().selectWorkspace).toHaveBeenCalledWith('ctx-normal');
      });

      it('unarchives context first if state is archived', async () => {
        vi.mocked(db.unarchiveWorkspace).mockResolvedValueOnce({} as any);
        vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ status: 'ok' });

        const result = await quickActionsService.executeContextAction('context.resume', 'ctx-archived', {
          lifecycleState: 'archived',
        });

        expect(result.success).toBe(true);
        expect(db.unarchiveWorkspace).toHaveBeenCalledWith('ctx-archived');
        expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-archived');
      });

      it('returns ERROR outcome if backend resume fails', async () => {
        vi.mocked(contextInteractionService.resumeContext).mockRejectedValueOnce(new Error('Tauri IPC timeout'));

        const result = await quickActionsService.executeContextAction('context.resume', 'ctx-fail');

        expect(result.success).toBe(false);
        expect(result.outcome).toBe('ERROR');
        expect(result.error).toContain('Tauri IPC timeout');
      });
    });

    describe('context.open_details', () => {
      it('opens context overview via workspaceStore', async () => {
        const result = await quickActionsService.executeContextAction('context.open_details', 'ctx-100');

        expect(result.success).toBe(true);
        expect(result.outcome).toBe('SUCCESS');
        expect(useWorkspaceStore.getState().openOverview).toHaveBeenCalledWith('ctx-100');
      });
    });

    describe('context.pause', () => {
      it('pauses active context successfully', async () => {
        vi.mocked(contextInteractionService.pauseContext).mockResolvedValueOnce({ status: 'ok' });

        const result = await quickActionsService.executeContextAction('context.pause', 'ctx-active', {
          lifecycleState: 'active',
        });

        expect(result.success).toBe(true);
        expect(result.outcome).toBe('SUCCESS');
        expect(contextInteractionService.pauseContext).toHaveBeenCalledWith('ctx-active');
      });

      it('blocks pause if context is already archived or dormant', async () => {
        const resArchived = await quickActionsService.executeContextAction('context.pause', 'ctx-archived', {
          lifecycleState: 'archived',
        });
        expect(resArchived.success).toBe(false);
        expect(resArchived.outcome).toBe('BLOCKED');
        expect(contextInteractionService.pauseContext).not.toHaveBeenCalled();

        const resDormant = await quickActionsService.executeContextAction('context.pause', 'ctx-dormant', {
          lifecycleState: 'dormant',
        });
        expect(resDormant.success).toBe(false);
        expect(resDormant.outcome).toBe('BLOCKED');
        expect(contextInteractionService.pauseContext).not.toHaveBeenCalled();
      });

      it('handles pause backend failure', async () => {
        vi.mocked(contextInteractionService.pauseContext).mockRejectedValueOnce(new Error('Engine rejection'));

        const result = await quickActionsService.executeContextAction('context.pause', 'ctx-active', {
          lifecycleState: 'active',
        });

        expect(result.success).toBe(false);
        expect(result.outcome).toBe('ERROR');
        expect(result.error).toContain('Engine rejection');
      });
    });

    describe('context.archive', () => {
      it('safely archives context and deselects if it was active', async () => {
        useWorkspaceStore.setState({ activeWorkspaceId: 'ctx-to-archive' });
        vi.mocked(db.archiveWorkspace).mockResolvedValueOnce({} as any);

        const result = await quickActionsService.executeContextAction('context.archive', 'ctx-to-archive', {
          lifecycleState: 'active',
        });

        expect(result.success).toBe(true);
        expect(result.outcome).toBe('SUCCESS');
        expect(db.archiveWorkspace).toHaveBeenCalledWith('ctx-to-archive');
        expect(useWorkspaceStore.getState().selectWorkspace).toHaveBeenCalledWith('inbox');
        expect(useWorkspaceStore.getState().loadWorkspaces).toHaveBeenCalled();
      });

      it('blocks archive if context is already archived', async () => {
        const result = await quickActionsService.executeContextAction('context.archive', 'ctx-already-archived', {
          lifecycleState: 'archived',
        });

        expect(result.success).toBe(false);
        expect(result.outcome).toBe('BLOCKED');
        expect(result.message).toContain('already archived');
        expect(db.archiveWorkspace).not.toHaveBeenCalled();
      });
    });

    describe('Unsupported context actions', () => {
      it('explicitly returns UNSUPPORTED for context.delete', async () => {
        const result = await quickActionsService.executeContextAction('context.delete', 'ctx-1');

        expect(result.success).toBe(false);
        expect(result.outcome).toBe('UNSUPPORTED');
        expect(result.message).toContain('Direct context deletion is not supported');
      });

      it('returns UNSUPPORTED for unknown action', async () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const result = await quickActionsService.executeContextAction('context.unknown' as any, 'ctx-1');

        expect(result.success).toBe(false);
        expect(result.outcome).toBe('UNSUPPORTED');
      });
    });
  });

  // =========================================================================
  // 3. Item Actions
  // =========================================================================
  describe('Item Actions Execution', () => {
    describe('item.mark_important', () => {
      it('delegates to contextWorksetService.controlContextMemory with keep_important', async () => {
        vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValueOnce(true);

        const result = await quickActionsService.executeItemAction('item.mark_important', 'ctx-1', 'item-10');

        expect(result.success).toBe(true);
        expect(result.outcome).toBe('SUCCESS');
        expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith('ctx-1', 'item-10', 'keep_important');
      });

      it('returns ERROR when controlContextMemory returns false', async () => {
        vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValueOnce(false);

        const result = await quickActionsService.executeItemAction('item.mark_important', 'ctx-1', 'item-10');

        expect(result.success).toBe(false);
        expect(result.outcome).toBe('ERROR');
      });
    });

    describe('item.unmark_important', () => {
      it('delegates to contextWorksetService.controlContextMemory with remove_important', async () => {
        vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValueOnce(true);

        const result = await quickActionsService.executeItemAction('item.unmark_important', 'ctx-1', 'item-10');

        expect(result.success).toBe(true);
        expect(result.outcome).toBe('SUCCESS');
        expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith('ctx-1', 'item-10', 'remove_important');
      });
    });

    describe('item.remove_from_context', () => {
      it('delegates to contextWorksetService.controlContextMemory with remove_from_context', async () => {
        vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValueOnce(true);

        const result = await quickActionsService.executeItemAction('item.remove_from_context', 'ctx-1', 'item-10');

        expect(result.success).toBe(true);
        expect(result.outcome).toBe('SUCCESS');
        expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith('ctx-1', 'item-10', 'remove_from_context');
      });
    });

    describe('Unsupported item actions', () => {
      it('explicitly returns UNSUPPORTED for item.delete', async () => {
        const result = await quickActionsService.executeItemAction('item.delete', 'ctx-1', 'item-10');

        expect(result.success).toBe(false);
        expect(result.outcome).toBe('UNSUPPORTED');
        expect(result.message).toContain('Permanent item deletion is not supported');
        expect(contextWorksetService.controlContextMemory).not.toHaveBeenCalled();
      });
    });
  });

  // =========================================================================
  // 4. Action Descriptors & Availability
  // =========================================================================
  describe('Action Descriptors Grounding', () => {
    it('evaluates available context actions for active context', () => {
      const actions = getAvailableContextActions({
        contextId: 'ctx-1',
        isActive: true,
        lifecycleState: 'active',
      });

      const resumeAct = actions.find((a) => a.id === 'context.resume');
      expect(resumeAct?.label).toBe('Focus Workspace');
      expect(resumeAct?.isSupported).toBe(true);

      const pauseAct = actions.find((a) => a.id === 'context.pause');
      expect(pauseAct?.isSupported).toBe(true);

      const archiveAct = actions.find((a) => a.id === 'context.archive');
      expect(archiveAct?.isSupported).toBe(true);

      const deleteAct = actions.find((a) => a.id === 'context.delete');
      expect(deleteAct?.isSupported).toBe(false);
      expect(deleteAct?.unsupportedReason).toBeDefined();
    });

    it('evaluates available context actions for archived context', () => {
      const actions = getAvailableContextActions({
        contextId: 'ctx-archived',
        lifecycleState: 'archived',
      });

      const resumeAct = actions.find((a) => a.id === 'context.resume');
      expect(resumeAct?.label).toBe('Restore & Resume');

      const pauseAct = actions.find((a) => a.id === 'context.pause');
      expect(pauseAct?.isSupported).toBe(false);
      expect(pauseAct?.unsupportedReason).toContain('Cannot pause an archived context');

      const archiveAct = actions.find((a) => a.id === 'context.archive');
      expect(archiveAct?.isSupported).toBe(false);
      expect(archiveAct?.unsupportedReason).toContain('already archived');
    });

    it('evaluates available item actions dynamically based on item importance', () => {
      const nonImportantActions = getAvailableItemActions({
        contextId: 'ctx-1',
        itemId: 'item-1',
        isImportant: false,
      });

      expect(nonImportantActions.some((a) => a.id === 'item.mark_important')).toBe(true);
      expect(nonImportantActions.some((a) => a.id === 'item.unmark_important')).toBe(false);

      const importantActions = getAvailableItemActions({
        contextId: 'ctx-1',
        itemId: 'item-2',
        isImportant: true,
      });

      expect(importantActions.some((a) => a.id === 'item.unmark_important')).toBe(true);
      expect(importantActions.some((a) => a.id === 'item.mark_important')).toBe(false);

      const deleteAct = importantActions.find((a) => a.id === 'item.delete');
      expect(deleteAct?.isSupported).toBe(false);
      expect(deleteAct?.unsupportedReason).toBeDefined();
    });
  });
});
