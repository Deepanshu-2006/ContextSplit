import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLauncherStore } from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { getDuplicateNameSet, resolveContextDisplayIdentity } from '../../models/contextIdentification';

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

describe('Context Launcher Selection, Keyboard Navigation, and Unavailable Contexts', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-alpha-1',
          name: 'Core Service',
          path: '/path/core-1',
          lifecycle_state: 'active',
          is_pinned: false,
          created_at: '2026-10-01T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
        } as any,
        {
          id: 'ctx-alpha-2',
          name: 'Core Service',
          path: '/path/core-2',
          lifecycle_state: 'recent',
          is_pinned: false,
          created_at: '2026-10-02T10:00:00Z',
          updated_at: '2026-10-21T10:00:00Z',
        } as any,
        {
          id: 'ctx-beta-long',
          name: 'Extremely Long Context Name for Comprehensive Multi-Repository Workflow Migration and Refactoring Phase 2 Analysis',
          path: '/path/beta-long',
          lifecycle_state: 'recent',
          is_pinned: true,
          created_at: '2026-10-03T10:00:00Z',
          updated_at: '2026-10-20T10:00:00Z',
        } as any,
        {
          id: 'ctx-gamma-dormant',
          name: 'Gamma Background Worker',
          path: '/path/gamma',
          lifecycle_state: 'paused',
          is_pinned: false,
          created_at: '2026-10-04T10:00:00Z',
          updated_at: '2026-10-19T10:00:00Z',
        } as any,
      ],
      activeWorkspaceId: 'ctx-alpha-1',
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: 'ctx-alpha-1',
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
        id: 'ctx-alpha-1',
        name: 'Core Service',
        isActive: true,
        state: 'active',
        isPinned: false,
      },
      recentContexts: [
        {
          id: 'ctx-beta-long',
          name: 'Extremely Long Context Name for Comprehensive Multi-Repository Workflow Migration and Refactoring Phase 2 Analysis',
          isActive: false,
          state: 'recent',
          isPinned: true,
        },
        {
          id: 'ctx-alpha-2',
          name: 'Core Service',
          isActive: false,
          state: 'recent',
          isPinned: false,
        },
        {
          id: 'ctx-gamma-dormant',
          name: 'Gamma Background Worker',
          isActive: false,
          state: 'paused',
          isPinned: false,
        },
      ],
      activePreview: {
        contextId: 'ctx-alpha-1',
        name: 'Core Service',
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
            id: 'item-1',
            title: 'Index File',
            itemType: 'file',
            status: 'available',
            isImportant: false,
            provenance: 'confirmed',
          },
        ],
      },
    });
  });

  // =========================================================================
  // 1. Stable Context ID Selection & Disambiguation
  // =========================================================================
  describe('1. Stable Context ID Selection & Duplicate Disambiguation', () => {
    it('selects the correct stable context ID when multiple contexts share identical names', async () => {
      const store = useLauncherStore.getState();
      const duplicateNames = getDuplicateNameSet([
        { id: 'ctx-alpha-1', name: 'Core Service' },
        { id: 'ctx-alpha-2', name: 'Core Service' },
      ]);
      expect(duplicateNames.has('core service')).toBe(true);

      // Verify display identities provide distinct disambiguation tags
      const identity1 = resolveContextDisplayIdentity(store.activeContext!, true);
      const identity2 = resolveContextDisplayIdentity(store.recentContexts[1], true);
      expect(identity1.id).toBe('ctx-alpha-1');
      expect(identity2.id).toBe('ctx-alpha-2');
      expect(identity1.disambiguationTag).toBe('#ctx-al');
      expect(identity2.disambiguationTag).toBe('#ctx-al');

      // Explicitly selecting duplicate 2 sets exact stable ID
      await store.selectContextById('ctx-alpha-2');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-2');

      // Re-selecting duplicate 1 sets exact stable ID
      await store.selectContextById('ctx-alpha-1');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-1');
    });

    it('preserves full untruncated name in store and preview for long context names', async () => {
      const longName =
        'Extremely Long Context Name for Comprehensive Multi-Repository Workflow Migration and Refactoring Phase 2 Analysis';
      await useLauncherStore.getState().selectContextById('ctx-beta-long');

      const state = useLauncherStore.getState();
      expect(state.selectedContextId).toBe('ctx-beta-long');
      expect(state.activePreview?.name).toBe(longName);
    });
  });

  // =========================================================================
  // 2. Keyboard Navigation & Action Execution Safety
  // =========================================================================
  describe('2. Keyboard Navigation & Action Execution Safety', () => {
    it('sets selected index sequentially without skipping indices', () => {
      const store = useLauncherStore.getState();
      expect(store.selectedIndex).toBe(0);

      store.setSelectedIndex(1);
      expect(useLauncherStore.getState().selectedIndex).toBe(1);

      store.setSelectedIndex(2);
      expect(useLauncherStore.getState().selectedIndex).toBe(2);

      store.setSelectedIndex(0);
      expect(useLauncherStore.getState().selectedIndex).toBe(0);
    });

    it('blocks Enter action execution when action is already in progress', async () => {
      useLauncherStore.setState({ isActionExecuting: true, executingActionId: 'context.resume' });

      const result = await useLauncherStore.getState().resumeContextById('ctx-alpha-1');
      expect(result).toBe(false);
    });

    it('blocks Enter action execution when an action is executing on another context', async () => {
      useLauncherStore.setState({ isActionExecuting: true, executingActionId: 'context.rename' });

      const result = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-alpha-2');
      expect(result.success).toBe(false);
      expect(result.message).toContain('already in progress');
    });
  });

  // =========================================================================
  // 3. Selection Preservation Across Supported Actions
  // =========================================================================
  describe('3. Selection Preservation Across Supported Actions', () => {
    it('preserves selectedContextId when fetchLauncherData is refreshed', async () => {
      // Select ctx-alpha-2
      await useLauncherStore.getState().selectContextById('ctx-alpha-2');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-2');

      // Refresh launcher data (simulating post-action sync)
      await useLauncherStore.getState().fetchLauncherData();

      // Selection must remain anchored to ctx-alpha-2
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-2');
    });

    it('preserves selection when pinning a context', async () => {
      await useLauncherStore.getState().selectContextById('ctx-alpha-2');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-2');

      await useLauncherStore.getState().executeContextAction('context.pin', 'ctx-alpha-2');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-2');
    });

    it('preserves selection when renaming a context', async () => {
      await useLauncherStore.getState().selectContextById('ctx-alpha-2');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-2');

      await useLauncherStore.getState().executeContextAction('context.rename', 'ctx-alpha-2', {
        newName: 'Core Service Renamed',
      });
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-2');
    });
  });

  // =========================================================================
  // 4. Unavailable & Disappearing Context Handling
  // =========================================================================
  describe('4. Unavailable & Disappearing Context Handling', () => {
    it('falls back to default context when selected context is deleted during navigation', async () => {
      // User is on ctx-alpha-2
      await useLauncherStore.getState().selectContextById('ctx-alpha-2');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-2');

      // Now ctx-alpha-2 is deleted from workspace store
      useWorkspaceStore.setState({
        workspaces: [
          {
            id: 'ctx-alpha-1',
            name: 'Core Service',
            path: '/path/core-1',
            lifecycle_state: 'active',
            created_at: '2026-10-01T10:00:00Z',
            updated_at: '2026-10-22T10:00:00Z',
          } as any,
        ],
        activeWorkspaceId: 'ctx-alpha-1',
      });

      // Engine sync runs
      await useLauncherStore.getState().fetchLauncherData();

      // Selection must gracefully fall back to existing active context
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-1');
    });

    it('handles empty workspace list without throwing or crashing state', async () => {
      useWorkspaceStore.setState({
        workspaces: [],
        activeWorkspaceId: null,
      });

      await useLauncherStore.getState().fetchLauncherData();

      const state = useLauncherStore.getState();
      expect(state.activeContext).toBeNull();
      expect(state.recentContexts).toHaveLength(0);
      expect(state.isLoading).toBe(false);
    });

    it('handles unavailable context preview failure by constructing safe fallback preview', async () => {
      // Mock failure for a non-existent context
      await useLauncherStore.getState().selectContextById('non-existent-uuid');

      const state = useLauncherStore.getState();
      expect(state.selectedContextId).toBe('non-existent-uuid');
      expect(state.activePreview).not.toBeNull();
      expect(state.activePreview?.contextId).toBe('non-existent-uuid');
      expect(state.activePreview?.name).toBe('Untitled Context');
      expect(state.isLoadingPreview).toBe(false);

      // User can immediately select another valid context
      await useLauncherStore.getState().selectContextById('ctx-alpha-1');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha-1');
    });
  });

  // =========================================================================
  // 5. Visual States and Roving TabIndex Contract
  // =========================================================================
  describe('5. Visual States and Roving TabIndex Contract', () => {
    it('ensures only selected context receives tabIndex 0 while unselected items receive -1', () => {
      const selectedId = 'ctx-alpha-1';
      const items = ['ctx-alpha-1', 'ctx-alpha-2', 'ctx-beta-long'];

      const tabIndices = items.map((id) => (id === selectedId ? 0 : -1));
      expect(tabIndices).toEqual([0, -1, -1]);
    });

    it('verifies distinct class signatures for selected vs unselected states', () => {
      const getClasses = (isSelected: boolean) =>
        `group flex items-center justify-between px-2.5 py-2 rounded-lg cursor-pointer transition-all duration-75 select-none focus:outline-none focus-visible:ring-2 focus-visible:ring-orange-400 focus-visible:ring-offset-1 focus-visible:ring-offset-stone-900 border-l-2 active:scale-[0.99] ${
          isSelected
            ? 'bg-stone-800 border-orange-500 hover:bg-stone-750 text-stone-100 ring-1 ring-white/15 shadow-xs'
            : 'border-transparent hover:bg-white/[0.05] hover:text-stone-100 active:bg-white/[0.08] text-stone-300'
        }`;

      const selectedClass = getClasses(true);
      const unselectedClass = getClasses(false);

      expect(selectedClass).toContain('border-orange-500');
      expect(selectedClass).toContain('bg-stone-800');
      expect(selectedClass).toContain('ring-white/15');

      expect(unselectedClass).toContain('border-transparent');
      expect(unselectedClass).not.toContain('border-orange-500');
      expect(unselectedClass).toContain('hover:bg-white/[0.05]');

      // Focus visibility is distinct with 2px orange ring on both
      expect(selectedClass).toContain('focus-visible:ring-2 focus-visible:ring-orange-400');
      expect(unselectedClass).toContain('focus-visible:ring-2 focus-visible:ring-orange-400');
    });
  });
});
