import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLauncherStore } from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { getAvailableContextActions, getAvailableItemActions } from '../../models/quickActions';

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

describe('Context Launcher Standardized Interaction States', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-1',
          name: 'Primary Workspace',
          path: '/path/to/primary',
          lifecycle_state: 'active',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        } as any,
        {
          id: 'ctx-2',
          name: 'Secondary Workspace',
          path: '/path/to/secondary',
          lifecycle_state: 'recent',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        } as any,
      ],
      activeWorkspaceId: 'ctx-1',
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: 'ctx-1',
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
        id: 'ctx-1',
        name: 'Primary Workspace',
        isActive: true,
        state: 'active',
      },
      recentContexts: [
        {
          id: 'ctx-2',
          name: 'Secondary Workspace',
          isActive: false,
          state: 'recent',
        },
      ],
      activePreview: {
        contextId: 'ctx-1',
        name: 'Primary Workspace',
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
            title: 'Project Roadmap',
            itemType: 'file',
            status: 'available',
            isImportant: false,
            provenance: 'confirmed',
          },
          {
            id: 'item-2',
            title: 'Design Specification',
            itemType: 'url',
            status: 'available',
            isImportant: true,
            provenance: 'inferred',
          },
        ],
      },
    });
  });

  describe('1. Selection vs Hover Independence', () => {
    it('maintains selectedContextId independently from hover interactions', () => {
      const state = useLauncherStore.getState();
      expect(state.selectedContextId).toBe('ctx-1');
      expect(state.selectedIndex).toBe(0);

      // Simulating selecting a context via explicit click or keyboard navigation
      useLauncherStore.getState().selectContextById('ctx-2');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-2');
    });
  });

  describe('2. Disabled Controls & Concurrency Guarding', () => {
    it('prevents initiating a new context action while another action is executing', async () => {
      useLauncherStore.setState({ isActionExecuting: true, executingActionId: 'context.pause' });

      const store = useLauncherStore.getState();
      const result = await store.executeContextAction('context.pin', 'ctx-1');

      expect(result.success).toBe(false);
      expect(result.message).toContain('already in progress');
    });

    it('prevents initiating an item action while an action is executing', async () => {
      useLauncherStore.setState({ isActionExecuting: true, executingActionId: 'item.open' });

      const store = useLauncherStore.getState();
      const result = await store.executeItemAction('item.delete', 'ctx-1', 'item-1');

      expect(result.success).toBe(false);
      expect(result.message).toContain('already in progress');
    });

    it('marks unsupported context actions as disabled with descriptive reason', () => {
      const activeActions = getAvailableContextActions({
        contextId: 'ctx-1',
        lifecycleState: 'active',
        isActive: true,
        readinessBand: 'ready',
      });

      const pauseAction = activeActions.find((a) => a.id === 'context.pause');
      expect(pauseAction?.isSupported).toBe(true);

      const deleteAction = activeActions.find((a) => a.id === 'context.delete');
      expect(deleteAction?.isSupported).toBe(false);
      expect(deleteAction?.unsupportedReason).toBeDefined();
    });

    it('marks deleted or missing items as disabled for open action', () => {
      const deletedItemActions = getAvailableItemActions({
        contextId: 'ctx-1',
        itemId: 'item-deleted',
        itemType: 'file',
        status: 'deleted',
      });

      const openAction = deletedItemActions.find((a) => a.id === 'item.open');
      expect(openAction?.isSupported).toBe(false);
      expect(openAction?.unsupportedReason).toContain('deleted');
    });
  });

  describe('3. Keyboard Navigation & Short-Circuiting', () => {
    it('supports quick filter switching via store action', () => {
      const store = useLauncherStore.getState();
      store.setActiveFilter('paused');
      expect(useLauncherStore.getState().activeFilter).toBe('paused');

      store.setActiveFilter('all');
      expect(useLauncherStore.getState().activeFilter).toBe('all');
    });

    it('supports sort order toggling', () => {
      const store = useLauncherStore.getState();
      expect(store.activeSort).toBe('recent');

      store.setActiveSort('name');
      expect(useLauncherStore.getState().activeSort).toBe('name');

      store.setActiveSort('recent');
      expect(useLauncherStore.getState().activeSort).toBe('recent');
    });

    it('clears errors gracefully without closing the launcher', () => {
      useLauncherStore.setState({
        actionError: {
          actionId: 'context.rename',
          title: 'Action Failed',
          message: 'Network timeout',
          isRetryable: true,
        },
      });

      expect(useLauncherStore.getState().actionError).not.toBeNull();
      useLauncherStore.getState().clearActionError();
      expect(useLauncherStore.getState().actionError).toBeNull();
      expect(useLauncherStore.getState().isOpen).toBe(true);
    });
  });

  describe('4. Context Renaming Interaction & Validation', () => {
    it('rejects empty or whitespace-only context name gracefully', async () => {
      const store = useLauncherStore.getState();
      const result = await store.executeContextAction('context.rename', 'ctx-1', { newName: '   ' });

      expect(result.success).toBe(false);
      expect(result.error).toContain('cannot be empty');
    });

    it('rejects excessively long context names (>100 characters)', async () => {
      const longName = 'A'.repeat(101);
      const store = useLauncherStore.getState();
      const result = await store.executeContextAction('context.rename', 'ctx-1', { newName: longName });

      expect(result.success).toBe(false);
      expect(result.error).toContain('100 characters');
    });
  });
});
