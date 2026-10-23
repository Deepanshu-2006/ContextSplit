import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLauncherStore } from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { quickActionsService } from '../../services/quickActionsService';
import { toast } from 'sonner';

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock sonner toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

describe('Context Launcher Feedback, Loading, Error, and Recovery Behavior', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-alpha',
          name: 'Alpha Workspace',
          path: '/path/to/alpha',
          lifecycle_state: 'active',
          is_pinned: false,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        } as any,
        {
          id: 'ctx-beta',
          name: 'Beta Workspace',
          path: '/path/to/beta',
          lifecycle_state: 'recent',
          is_pinned: true,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        } as any,
      ],
      activeWorkspaceId: 'ctx-alpha',
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: 'ctx-alpha',
      activeFilter: 'all',
      activeSort: 'recent',
      isPreviewOpen: true,
      isLoading: false,
      isLoadingPreview: false,
      isResuming: false,
      isActionExecuting: false,
      executingActionId: null,
      executingTargetId: null,
      resumeStatus: 'idle',
      resumeError: null,
      actionError: null,
      notReadyReason: null,
      error: null,
      previewError: null,
      activeContext: {
        id: 'ctx-alpha',
        name: 'Alpha Workspace',
        isActive: true,
        isPinned: false,
        state: 'active',
      },
      recentContexts: [
        {
          id: 'ctx-beta',
          name: 'Beta Workspace',
          isActive: false,
          isPinned: true,
          state: 'recent',
        },
      ],
      activePreview: {
        contextId: 'ctx-alpha',
        name: 'Alpha Workspace',
        state: 'active',
        readinessBand: 'ready',
        itemCount: 1,
        isPinned: false,
        reasons: [],
        importantItems: [],
        isAmbiguous: false,
        isStale: false,
        worksetItems: [
          {
            id: 'item-101',
            title: 'Config File',
            itemType: 'file',
            status: 'available',
            isImportant: false,
            provenance: 'confirmed',
          },
        ],
      },
    });
  });

  describe('1. Loading & In-Flight Concurrency Guards', () => {
    it('blocks duplicate submissions of context actions when an action is already executing', async () => {
      useLauncherStore.setState({ isActionExecuting: true, executingActionId: 'context.pause' });

      const result = await useLauncherStore.getState().executeContextAction('context.pin', 'ctx-alpha');
      expect(result.success).toBe(false);
      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('already in progress');
    });

    it('blocks duplicate submissions of context actions when a context is resuming', async () => {
      useLauncherStore.setState({ isResuming: true });

      const result = await useLauncherStore.getState().executeContextAction('context.archive', 'ctx-alpha');
      expect(result.success).toBe(false);
      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('already in progress');
    });

    it('blocks duplicate submissions of item actions when an action is already executing', async () => {
      useLauncherStore.setState({ isActionExecuting: true, executingActionId: 'item.open' });

      const result = await useLauncherStore.getState().executeItemAction('item.mark_important', 'ctx-alpha', 'item-101');
      expect(result.success).toBe(false);
      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('already in progress');
    });

    it('blocks duplicate submissions of item actions when context resume is in progress', async () => {
      useLauncherStore.setState({ isResuming: true });

      const result = await useLauncherStore.getState().executeItemAction('item.open', 'ctx-alpha', 'item-101');
      expect(result.success).toBe(false);
      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('already in progress');
    });
  });

  describe('2. Success Feedback Integrity', () => {
    it('fires success toast only after context pin succeeds', async () => {
      vi.spyOn(quickActionsService, 'executeContextAction').mockResolvedValueOnce({
        actionId: 'context.pin',
        success: true,
        outcome: 'SUCCESS',
        message: 'Context pinned',
      });

      const result = await useLauncherStore.getState().executeContextAction('context.pin', 'ctx-alpha');
      expect(result.success).toBe(true);
      expect(toast.success).toHaveBeenCalledWith('Context pinned');
      expect(toast.error).not.toHaveBeenCalled();
    });

    it('fires success toast only after item action succeeds', async () => {
      vi.spyOn(quickActionsService, 'executeItemAction').mockResolvedValueOnce({
        actionId: 'item.mark_important',
        success: true,
        outcome: 'SUCCESS',
        message: 'Item pinned in context memory',
      });

      const result = await useLauncherStore.getState().executeItemAction('item.mark_important', 'ctx-alpha', 'item-101');
      expect(result.success).toBe(true);
      expect(toast.success).toHaveBeenCalledWith('Item pinned in context memory');
      expect(toast.error).not.toHaveBeenCalled();
    });

    it('never fires success toast when an action fails or is blocked', async () => {
      vi.spyOn(quickActionsService, 'executeContextAction').mockResolvedValueOnce({
        actionId: 'context.rename',
        success: false,
        outcome: 'ERROR',
        error: 'Network timeout',
        message: 'Could not rename context',
      });

      const result = await useLauncherStore.getState().executeContextAction('context.rename', 'ctx-alpha', { newName: 'New' });
      expect(result.success).toBe(false);
      expect(toast.success).not.toHaveBeenCalled();
      expect(toast.error).toHaveBeenCalled();
    });
  });

  describe('3. Error Feedback & Data Preservation on Failure', () => {
    it('preserves existing displayed contexts in store when fetchLauncherData encounters an error', async () => {
      const initialActive = useLauncherStore.getState().activeContext;
      const initialRecents = useLauncherStore.getState().recentContexts;

      // Mock an unexpected error during workspace/engine retrieval
      vi.spyOn(useWorkspaceStore, 'getState').mockImplementationOnce(() => {
        throw new Error('Database locked');
      });

      await useLauncherStore.getState().fetchLauncherData();

      const state = useLauncherStore.getState();
      expect(state.isLoading).toBe(false);
      // Valid data remains preserved in store so user can still access contexts
      expect(state.activeContext).toEqual(initialActive);
      expect(state.recentContexts.length).toBe(initialRecents.length);
      expect(state.error).toContain('Database locked');
    });

    it('preserves valid preview data when preview refresh fails', async () => {
      const initialPreview = useLauncherStore.getState().activePreview;
      expect(initialPreview).not.toBeNull();

      const contextWorksetRetrieval = await import('../../services/contextWorksetRetrieval');
      vi.spyOn(contextWorksetRetrieval, 'retrieveContextWorkset').mockRejectedValueOnce(
        new Error('Workset retrieval failed')
      );

      await useLauncherStore.getState().selectContextById('ctx-alpha', { forceRefresh: true });

      const state = useLauncherStore.getState();
      expect(state.isLoadingPreview).toBe(false);
      // Valid preview data was preserved instead of unmounting into empty
      expect(state.activePreview?.contextId).toBe('ctx-alpha');
      expect(state.activePreview?.worksetItems.length).toBe(1);
      expect(state.previewError).toContain('Workset retrieval failed');
    });

    it('generates concise, descriptive actionError info with retry capability for transient errors', async () => {
      vi.spyOn(quickActionsService, 'executeContextAction').mockResolvedValueOnce({
        actionId: 'context.pin',
        success: false,
        outcome: 'ERROR',
        error: 'I/O failure writing workspace metadata',
        message: 'Disk temporarily busy',
      });

      await useLauncherStore.getState().executeContextAction('context.pin', 'ctx-alpha');

      const actionError = useLauncherStore.getState().actionError;
      expect(actionError).not.toBeNull();
      expect(actionError?.title).toBe('Pin Failed');
      expect(actionError?.isRetryable).toBe(true);
      expect(actionError?.targetId).toBe('ctx-alpha');
    });

    it('marks actionError as non-retryable when target context is permanently unavailable', async () => {
      vi.spyOn(quickActionsService, 'executeContextAction').mockResolvedValueOnce({
        actionId: 'context.resume',
        success: false,
        outcome: 'ERROR',
        error: "Context 'ctx-deleted' is no longer available. It may have been closed or deleted.",
        message: 'Context not found',
      });

      await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-alpha');

      const actionError = useLauncherStore.getState().actionError;
      expect(actionError).not.toBeNull();
      expect(actionError?.title).toBe('Context Unavailable');
      expect(actionError?.isRetryable).toBe(false);
    });
  });

  describe('4. Recovery & Error Clearing Actions', () => {
    it('clears global error via clearError() without affecting contexts or preview', () => {
      useLauncherStore.setState({ error: 'Connection Error' });
      expect(useLauncherStore.getState().error).toBe('Connection Error');

      useLauncherStore.getState().clearError();
      expect(useLauncherStore.getState().error).toBeNull();
      expect(useLauncherStore.getState().activeContext).not.toBeNull();
    });

    it('clears previewError via clearPreviewError() without affecting active preview model', () => {
      useLauncherStore.setState({ previewError: 'Workset Notice' });
      expect(useLauncherStore.getState().previewError).toBe('Workset Notice');

      useLauncherStore.getState().clearPreviewError();
      expect(useLauncherStore.getState().previewError).toBeNull();
      expect(useLauncherStore.getState().activePreview).not.toBeNull();
    });

    it('clears actionError and resume states via clearActionError() and clearResumeState()', () => {
      useLauncherStore.setState({
        actionError: {
          actionId: 'context.resume',
          title: 'Resume Failed',
          message: 'Process error',
          isRetryable: true,
        },
        resumeStatus: 'error',
        resumeError: 'Process error',
      });

      useLauncherStore.getState().clearActionError();
      expect(useLauncherStore.getState().actionError).toBeNull();
      expect(useLauncherStore.getState().resumeError).toBeNull();

      useLauncherStore.setState({ resumeStatus: 'not_ready', notReadyReason: 'Dormant context' });
      useLauncherStore.getState().clearResumeState();
      expect(useLauncherStore.getState().resumeStatus).toBe('idle');
      expect(useLauncherStore.getState().notReadyReason).toBeNull();
    });
  });
});
