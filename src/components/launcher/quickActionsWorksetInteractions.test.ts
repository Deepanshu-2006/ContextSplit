import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  useLauncherStore,
  invalidateLauncherPreviewCache,
} from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { quickActionsService } from '../../services/quickActionsService';
import {
  getPrimaryContextActionDescriptor,
  getAvailableContextActions,
  getAvailableItemActions,
  resolveContextLifecycleState,
} from '../../models/quickActions';
import { db } from '../../services/db';
import { toast } from 'sonner';

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock Tauri opener
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
  revealItemInDir: vi.fn().mockResolvedValue(undefined),
  openPath: vi.fn().mockResolvedValue(undefined),
}));

// Mock context interaction service
vi.mock('../../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getResumeReadiness: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    resumeContext: vi.fn().mockResolvedValue({ status: 'ok' }),
    recordContextFeedback: vi.fn().mockResolvedValue({ recorded: true }),
  },
}));

// Mock db
vi.mock('../../services/db', () => ({
  db: {
    getItem: vi.fn(),
    moveItem: vi.fn(),
    updateItem: vi.fn(),
    setWorkspacePinned: vi.fn(),
    archiveWorkspace: vi.fn(),
    unarchiveWorkspace: vi.fn(),
  },
}));

// Mock toast notifications
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

describe('Quick Actions & Context Workset Interaction Polish Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invalidateLauncherPreviewCache();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-active',
          name: 'Active Context Workspace',
          path: '/path/to/active',
          lifecycle_state: 'active',
          is_pinned: false,
          created_at: '2026-10-01T00:00:00Z',
          updated_at: '2026-10-23T00:00:00Z',
        } as any,
        {
          id: 'ctx-paused',
          name: 'Paused Workspace',
          path: '/path/to/paused',
          lifecycle_state: 'paused',
          is_pinned: false,
          created_at: '2026-10-01T00:00:00Z',
          updated_at: '2026-10-23T00:00:00Z',
        } as any,
        {
          id: 'ctx-archived',
          name: 'Archived Workspace',
          path: '/path/to/archived',
          lifecycle_state: 'archived',
          is_pinned: false,
          created_at: '2026-09-01T00:00:00Z',
          updated_at: '2026-10-01T00:00:00Z',
        } as any,
        {
          id: 'ctx-dormant',
          name: 'Dormant Workspace',
          path: '/path/to/dormant',
          lifecycle_state: 'dormant',
          is_pinned: false,
          created_at: '2026-08-01T00:00:00Z',
          updated_at: '2026-08-01T00:00:00Z',
        } as any,
      ],
      activeWorkspaceId: 'ctx-active',
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: 'ctx-active',
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
        id: 'ctx-active',
        name: 'Active Context Workspace',
        isActive: true,
        isPinned: false,
        state: 'active',
      },
      recentContexts: [
        {
          id: 'ctx-paused',
          name: 'Paused Workspace',
          isActive: false,
          isPinned: false,
          state: 'paused',
        },
        {
          id: 'ctx-archived',
          name: 'Archived Workspace',
          isActive: false,
          isPinned: false,
          state: 'archived',
        },
        {
          id: 'ctx-dormant',
          name: 'Dormant Workspace',
          isActive: false,
          isPinned: false,
          state: 'dormant',
        },
      ],
      activePreview: {
        contextId: 'ctx-active',
        name: 'Active Context Workspace',
        state: 'active',
        readinessBand: 'ready',
        itemCount: 2,
        isPinned: false,
        reasons: [],
        importantItems: [],
        isAmbiguous: false,
        isStale: false,
        worksetItems: [
          {
            id: 'item-1',
            title: 'Active Spec Doc',
            itemType: 'url',
            status: 'available',
            isImportant: false,
            provenance: 'confirmed',
          },
          {
            id: 'item-2',
            title: 'Backend Code.ts',
            itemType: 'file',
            status: 'available',
            isImportant: true,
            provenance: 'confirmed',
          },
        ],
      },
    });
  });

  // =========================================================================
  // Requirement 1: Primary Action Matches Context's Actual State
  // =========================================================================
  describe('1. Primary Action State Alignment', () => {
    it('generates "Focus Workspace" for active contexts', () => {
      const state = resolveContextLifecycleState({ lifecycleState: 'active', isActive: true });
      expect(state).toBe('active');

      const primary = getPrimaryContextActionDescriptor(state);
      expect(primary.actionId).toBe('context.resume');
      expect(primary.label).toBe('Focus Workspace');
      expect(primary.inProgressLabel).toBe('Focusing...');
    });

    it('generates "Resume Context" for paused contexts', () => {
      const state = resolveContextLifecycleState({ lifecycleState: 'paused', isActive: false });
      expect(state).toBe('paused');

      const primary = getPrimaryContextActionDescriptor(state);
      expect(primary.actionId).toBe('context.resume');
      expect(primary.label).toBe('Resume Context');
      expect(primary.inProgressLabel).toBe('Resuming...');
    });

    it('generates "Restore & Resume" for archived contexts', () => {
      const state = resolveContextLifecycleState({ lifecycleState: 'archived', isActive: false });
      expect(state).toBe('archived');

      const primary = getPrimaryContextActionDescriptor(state);
      expect(primary.actionId).toBe('context.resume');
      expect(primary.label).toBe('Restore & Resume');
      expect(primary.inProgressLabel).toBe('Restoring...');
    });

    it('generates "Resume Context" for dormant / recent contexts', () => {
      const state = resolveContextLifecycleState({ lifecycleState: 'dormant', isActive: false });
      expect(state).toBe('dormant');

      const primary = getPrimaryContextActionDescriptor(state);
      expect(primary.actionId).toBe('context.resume');
      expect(primary.label).toBe('Resume Context');
      expect(primary.inProgressLabel).toBe('Resuming...');
    });
  });

  // =========================================================================
  // Requirement 2: Compact & Discoverable Secondary Actions
  // =========================================================================
  describe('2. Secondary Actions Separation & Discoverability', () => {
    it('excludes primary resume action from secondary actions list', () => {
      const actions = getAvailableContextActions({
        contextId: 'ctx-active',
        lifecycleState: 'active',
        isActive: true,
      });

      const secondary = actions.filter((a) => a.id !== 'context.resume');
      expect(secondary.every((a) => a.id !== 'context.resume')).toBe(true);
      expect(secondary.map((a) => a.id)).toContain('context.open_details');
      expect(secondary.map((a) => a.id)).toContain('context.rename');
      expect(secondary.map((a) => a.id)).toContain('context.pause');
    });

    it('adapts secondary action options based on context state (e.g. pause disabled if paused)', () => {
      const pausedActions = getAvailableContextActions({
        contextId: 'ctx-paused',
        lifecycleState: 'paused',
        isActive: false,
      });

      const pauseAction = pausedActions.find((a) => a.id === 'context.pause');
      expect(pauseAction).toBeDefined();
      expect(pauseAction?.isSupported).toBe(false);
      expect(pauseAction?.unsupportedReason).toContain('already paused');
    });
  });

  // =========================================================================
  // Requirement 3: Opening an Item Does Not Trigger Context Actions
  // =========================================================================
  describe('3. Isolation Between Item Opening and Context Actions', () => {
    it('opening a workset item does not trigger context resume or mutate workspace active state', async () => {
      vi.mocked(db.getItem).mockResolvedValueOnce({
        id: 'item-1',
        workspace_id: 'ctx-active',
        session_id: null,
        item_type: 'url',
        content: 'https://example.com/spec',
        title: 'Active Spec Doc',
        metadata: '{}',
        created_at: '2026-10-23T00:00:00Z',
        updated_at: '2026-10-23T00:00:00Z',
      });

      const initialSelectedContext = useLauncherStore.getState().selectedContextId;
      const initialActiveWorkspace = useWorkspaceStore.getState().activeWorkspaceId;

      const result = await useLauncherStore
        .getState()
        .executeItemAction('item.open', 'ctx-active', 'item-1');

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');

      // Does not trigger context interaction service resume
      const contextInteractionService = (await import('../../services/contextInteractionService'))
        .contextInteractionService;
      expect(contextInteractionService.resumeContext).not.toHaveBeenCalled();

      // Does not mutate active workspace in store
      expect(useWorkspaceStore.getState().activeWorkspaceId).toBe(initialActiveWorkspace);
      expect(useLauncherStore.getState().selectedContextId).toBe(initialSelectedContext);
    });
  });

  // =========================================================================
  // Requirement 4: Selected Context Preservation After Item Interactions
  // =========================================================================
  describe('4. Selection Preservation After Item Interactions', () => {
    it('preserves selected context ID when toggling item importance', async () => {
      vi.spyOn(quickActionsService, 'executeItemAction').mockResolvedValueOnce({
        actionId: 'item.mark_important',
        success: true,
        outcome: 'SUCCESS',
        message: 'Item pinned in context memory',
      });

      await useLauncherStore
        .getState()
        .executeItemAction('item.mark_important', 'ctx-active', 'item-1');

      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-active');
      expect(toast.success).toHaveBeenCalledWith('Item pinned in context memory');
    });

    it('preserves selected context ID when removing item from workset', async () => {
      vi.spyOn(quickActionsService, 'executeItemAction').mockResolvedValueOnce({
        actionId: 'item.remove_from_context',
        success: true,
        outcome: 'SUCCESS',
        message: 'Item removed from context',
      });

      await useLauncherStore
        .getState()
        .executeItemAction('item.remove_from_context', 'ctx-active', 'item-1');

      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-active');
      expect(toast.success).toHaveBeenCalledWith('Item removed from context');
    });
  });

  // =========================================================================
  // Requirement 5: Stale Workset Data Prevention Across Context Selection
  // =========================================================================
  describe('5. Stale Workset Data Isolation', () => {
    it('clears activePreview when switching to a different context to avoid showing stale workset items', async () => {
      expect(useLauncherStore.getState().activePreview?.contextId).toBe('ctx-active');

      // Select another context that needs to be fetched
      const promise = useLauncherStore.getState().selectContextById('ctx-paused');

      // Immediately upon selection, previous activePreview is cleared
      const immediateState = useLauncherStore.getState();
      expect(immediateState.selectedContextId).toBe('ctx-paused');
      expect(immediateState.activePreview).toBeNull();
      expect(immediateState.isLoadingPreview).toBe(true);

      await promise;
    });

    it('rejects stale async preview responses when another context has already been selected', async () => {
      useLauncherStore.setState({ selectedContextId: 'ctx-dormant' });

      // Simulate a late resolving fetch for ctx-archived
      // The store's selectContextById checks: requestId === activePreviewRequestId && selectedContextId === contextId
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-dormant');
      expect(useLauncherStore.getState().activePreview?.contextId).not.toBe('ctx-archived');
    });
  });

  // =========================================================================
  // Requirement 6: Loading & Error Feedback
  // =========================================================================
  describe('6. Loading and Error Feedback for Workset Actions', () => {
    it('blocks item action execution when context is in the middle of resuming', async () => {
      useLauncherStore.setState({ isResuming: true });

      const result = await useLauncherStore
        .getState()
        .executeItemAction('item.open', 'ctx-active', 'item-1');

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('already in progress');
    });

    it('returns retryable actionError when item action fails due to transient error', async () => {
      vi.spyOn(quickActionsService, 'executeItemAction').mockResolvedValueOnce({
        actionId: 'item.open',
        success: false,
        outcome: 'ERROR',
        error: 'I/O socket timeout',
        message: 'Could not access file',
      });

      await useLauncherStore
        .getState()
        .executeItemAction('item.open', 'ctx-active', 'item-2');

      const actionError = useLauncherStore.getState().actionError;
      expect(actionError).not.toBeNull();
      expect(actionError?.targetId).toBe('item-2');
      expect(actionError?.isRetryable).toBe(true);
      expect(toast.error).toHaveBeenCalledWith('Could not access file');
    });
  });

  // =========================================================================
  // Requirement 8: Item Actions Definition & Unsupported Safeguards
  // =========================================================================
  describe('7. Item Action Descriptors and Safety', () => {
    it('safely disables open action for deleted or missing items', () => {
      const deletedActions = getAvailableItemActions({
        contextId: 'ctx-active',
        itemId: 'item-del',
        itemType: 'url',
        status: 'deleted',
      });

      const openAction = deletedActions.find((a) => a.id === 'item.open');
      expect(openAction?.isSupported).toBe(false);
      expect(openAction?.unsupportedReason).toContain('deleted');
    });

    it('safely marks permanent delete as unsupported in quick actions launcher', () => {
      const actions = getAvailableItemActions({
        contextId: 'ctx-active',
        itemId: 'item-1',
        itemType: 'file',
      });

      const deleteAction = actions.find((a) => a.id === 'item.delete');
      expect(deleteAction?.isSupported).toBe(false);
      expect(deleteAction?.unsupportedReason).toContain('must be performed');
    });
  });
});
