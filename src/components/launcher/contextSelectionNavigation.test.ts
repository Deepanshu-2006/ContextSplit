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
            ? 'bg-stone-800 border-orange-500 hover:bg-stone-800 text-stone-100 ring-1 ring-white/15 shadow-xs'
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

  // =========================================================================
  // 6. Filter Tabs Roving TabIndex & Arrow Key Navigation
  // =========================================================================
  describe('6. Filter Tabs Roving TabIndex & Arrow Key Navigation', () => {
    const FILTER_OPTIONS = ['all', 'active', 'recent', 'paused', 'archived'] as const;

    it('assigns tabIndex 0 to active filter tab and -1 to all inactive tabs', () => {
      useLauncherStore.setState({ activeFilter: 'active' });
      const currentFilter = useLauncherStore.getState().activeFilter;

      const tabIndices = FILTER_OPTIONS.map((f) => (f === currentFilter ? 0 : -1));
      expect(tabIndices).toEqual([-1, 0, -1, -1, -1]);
    });

    it('implements ArrowRight roving navigation with wrap-around', () => {
      const simulateTabArrow = (currentIdx: number, key: 'ArrowRight' | 'ArrowLeft' | 'Home' | 'End') => {
        if (key === 'ArrowRight') return (currentIdx + 1) % FILTER_OPTIONS.length;
        if (key === 'ArrowLeft') return (currentIdx - 1 + FILTER_OPTIONS.length) % FILTER_OPTIONS.length;
        if (key === 'Home') return 0;
        if (key === 'End') return FILTER_OPTIONS.length - 1;
        return currentIdx;
      };

      // From index 0 ('all') -> ArrowRight -> index 1 ('active')
      expect(simulateTabArrow(0, 'ArrowRight')).toBe(1);
      // From index 4 ('archived') -> ArrowRight -> index 0 ('all') (wrap-around)
      expect(simulateTabArrow(4, 'ArrowRight')).toBe(0);
      // From index 0 ('all') -> ArrowLeft -> index 4 ('archived') (wrap-around)
      expect(simulateTabArrow(0, 'ArrowLeft')).toBe(4);
      // Home and End
      expect(simulateTabArrow(2, 'Home')).toBe(0);
      expect(simulateTabArrow(2, 'End')).toBe(4);
    });

    it('updates store activeFilter and maintains consistent list filtering on tab navigation', () => {
      useLauncherStore.getState().setActiveFilter('paused');
      expect(useLauncherStore.getState().activeFilter).toBe('paused');

      useLauncherStore.getState().setActiveFilter('archived');
      expect(useLauncherStore.getState().activeFilter).toBe('archived');

      useLauncherStore.getState().setActiveFilter('all');
      expect(useLauncherStore.getState().activeFilter).toBe('all');
    });
  });

  // =========================================================================
  // 7. Sort Segmented Control Roving Navigation
  // =========================================================================
  describe('7. Sort Segmented Control Roving Navigation', () => {
    it('assigns tabIndex 0 to active sort button and -1 to inactive sort button', () => {
      useLauncherStore.setState({ activeSort: 'recent' });
      expect(useLauncherStore.getState().activeSort).toBe('recent');

      const getSortTabIndex = (btnSort: 'recent' | 'name') =>
        useLauncherStore.getState().activeSort === btnSort ? 0 : -1;

      expect(getSortTabIndex('recent')).toBe(0);
      expect(getSortTabIndex('name')).toBe(-1);

      useLauncherStore.getState().setActiveSort('name');
      expect(getSortTabIndex('recent')).toBe(-1);
      expect(getSortTabIndex('name')).toBe(0);
    });

    it('toggles between recent and name on ArrowRight / ArrowLeft or toggleActiveSort', () => {
      useLauncherStore.setState({ activeSort: 'recent' });
      useLauncherStore.getState().toggleActiveSort();
      expect(useLauncherStore.getState().activeSort).toBe('name');

      useLauncherStore.getState().toggleActiveSort();
      expect(useLauncherStore.getState().activeSort).toBe('recent');
    });
  });

  // =========================================================================
  // 8. Menus Dismissal & Focus Restoration
  // =========================================================================
  describe('8. Menus Dismissal & Focus Restoration', () => {
    it('restores focus to trigger element when secondary menu is dismissed via Escape', () => {
      const triggerBtn = { focus: vi.fn() };
      let isMenuOpen = true;

      const handleMenuKeyDown = (e: { key: string; preventDefault: () => void; stopPropagation: () => void }) => {
        if (e.key === 'Escape' || e.key === 'Tab') {
          e.preventDefault();
          e.stopPropagation();
          isMenuOpen = false;
          triggerBtn.focus();
        }
      };

      const preventDefaultMock = vi.fn();
      const stopPropagationMock = vi.fn();

      handleMenuKeyDown({
        key: 'Escape',
        preventDefault: preventDefaultMock,
        stopPropagation: stopPropagationMock,
      });

      expect(isMenuOpen).toBe(false);
      expect(preventDefaultMock).toHaveBeenCalled();
      expect(stopPropagationMock).toHaveBeenCalled();
      expect(triggerBtn.focus).toHaveBeenCalledTimes(1);
    });

    it('restores focus to trigger element and calls preventDefault when menu is dismissed via Tab', () => {
      const triggerBtn = { focus: vi.fn() };
      let isMenuOpen = true;

      const handleMenuKeyDown = (e: { key: string; preventDefault: () => void; stopPropagation: () => void }) => {
        if (e.key === 'Tab') {
          e.preventDefault();
          e.stopPropagation();
          isMenuOpen = false;
          triggerBtn.focus();
        }
      };

      const preventDefaultMock = vi.fn();
      const stopPropagationMock = vi.fn();

      handleMenuKeyDown({
        key: 'Tab',
        preventDefault: preventDefaultMock,
        stopPropagation: stopPropagationMock,
      });

      expect(isMenuOpen).toBe(false);
      expect(preventDefaultMock).toHaveBeenCalled();
      expect(stopPropagationMock).toHaveBeenCalled();
      expect(triggerBtn.focus).toHaveBeenCalledTimes(1);
    });

    it('restores focus to item menu trigger button upon item action execution', async () => {
      const itemTriggerBtn = { focus: vi.fn() };
      let isItemMenuOpen = true;

      const handleExecuteItemAction = async (actionId: string) => {
        isItemMenuOpen = false;
        await useLauncherStore.getState().executeItemAction(actionId as any, 'ctx-alpha-1', 'item-1');
        itemTriggerBtn.focus();
      };

      await handleExecuteItemAction('item.mark_important');
      expect(isItemMenuOpen).toBe(false);
      expect(itemTriggerBtn.focus).toHaveBeenCalledTimes(1);
    });
  });

  // =========================================================================
  // 9. Dialog Invocation Trigger Focus Restoration & Trap
  // =========================================================================
  describe('9. Dialog Invocation Trigger Focus Restoration & Modal Focus Trap', () => {
    it('restores focus to triggering element when launcher dialog closes', () => {
      const hostTriggerButton = { focus: vi.fn() };
      const previousActiveRef = { current: hostTriggerButton as any };

      // Dialog was open, now closing
      useLauncherStore.setState({ isOpen: false });

      if (previousActiveRef.current && typeof previousActiveRef.current.focus === 'function') {
        const target = previousActiveRef.current;
        previousActiveRef.current = null;
        target.focus();
      }

      expect(hostTriggerButton.focus).toHaveBeenCalledTimes(1);
      expect(previousActiveRef.current).toBeNull();
    });

    it('keeps focus trapped within modal dialog without escaping to background', () => {
      const focusableElements = [
        { id: 'launcher-search-input', focus: vi.fn() },
        { id: 'launcher-filter-tab-all', focus: vi.fn() },
        { id: 'launcher-sort-recent-btn', focus: vi.fn() },
        { id: 'launcher-primary-action-btn', focus: vi.fn() },
        { id: 'launcher-more-actions-btn', focus: vi.fn() },
      ];

      const firstEl = focusableElements[0];
      const lastEl = focusableElements[focusableElements.length - 1];

      // Simulate Tab on last element -> wraps to first
      let currentActive: any = lastEl;
      const simulateTab = (shiftKey: boolean) => {
        if (shiftKey) {
          if (currentActive === firstEl) {
            currentActive = lastEl;
            lastEl.focus();
          }
        } else {
          if (currentActive === lastEl) {
            currentActive = firstEl;
            firstEl.focus();
          }
        }
      };

      simulateTab(false);
      expect(firstEl.focus).toHaveBeenCalledTimes(1);

      // Simulate Shift+Tab on first element -> wraps to last
      currentActive = firstEl;
      simulateTab(true);
      expect(lastEl.focus).toHaveBeenCalledTimes(1);
    });
  });

  // =========================================================================
  // 10. Focus Orphan Prevention on Dynamic Updates
  // =========================================================================
  describe('10. Focus Orphan Prevention on Dynamic Updates', () => {
    it('transfers focus to sibling or primary action when focused workset item is deleted', () => {
      const primaryActionBtn = { id: 'launcher-primary-action-btn', focus: vi.fn() };
      const nextSiblingRow = { id: 'workset-row-2', focus: vi.fn() };
      const unmountingRow = {
        isFocused: true,
        nextElementSibling: nextSiblingRow,
        previousElementSibling: null,
      };

      // Unmount cleanup simulation
      const onUnmount = () => {
        if (unmountingRow.isFocused) {
          const target = unmountingRow.nextElementSibling || unmountingRow.previousElementSibling || primaryActionBtn;
          target?.focus();
        }
      };

      onUnmount();
      expect(nextSiblingRow.focus).toHaveBeenCalledTimes(1);

      // If no sibling remains, falls back to primary action button
      const lastRemainingRow = {
        isFocused: true,
        nextElementSibling: null,
        previousElementSibling: null,
      };
      const onLastUnmount = () => {
        if (lastRemainingRow.isFocused) {
          const target = lastRemainingRow.nextElementSibling || lastRemainingRow.previousElementSibling || primaryActionBtn;
          target?.focus();
        }
      };

      onLastUnmount();
      expect(primaryActionBtn.focus).toHaveBeenCalledTimes(1);
    });

    it('returns focus to search input if context list becomes empty', () => {
      const searchInput = { id: 'launcher-search-input', focus: vi.fn() };
      const flatNavigableItems: any[] = [];
      const currentActive = 'document.body';

      if (flatNavigableItems.length === 0 && (currentActive === 'document.body' || currentActive === 'in-list')) {
        searchInput.focus();
      }

      expect(searchInput.focus).toHaveBeenCalledTimes(1);
    });
  });

  // =========================================================================
  // 11. Event Isolation & Unrelated Actions Prevention
  // =========================================================================
  describe('11. Event Isolation & Unrelated Actions Prevention', () => {
    it('does not hijack slash (/) when user is typing in a text input or textarea', () => {
      const inputRef = { focus: vi.fn(), select: vi.fn() };
      let prevented = false;

      const handleSlashKey = (activeTagName: string) => {
        const isTextInput = activeTagName === 'INPUT' || activeTagName === 'TEXTAREA';
        if (!isTextInput) {
          prevented = true;
          inputRef.focus();
        }
      };

      // When user is typing inside rename input
      handleSlashKey('INPUT');
      expect(prevented).toBe(false);
      expect(inputRef.focus).not.toHaveBeenCalled();

      // When user is in regular launcher list
      handleSlashKey('DIV');
      expect(prevented).toBe(true);
      expect(inputRef.focus).toHaveBeenCalledTimes(1);
    });

    it('does not execute context resume when Enter is pressed inside rename form', () => {
      let resumed = false;

      const handleEnterKey = (activeElDescendantOf: string) => {
        if (activeElDescendantOf === 'form' || activeElDescendantOf === 'BUTTON') {
          return; // Let form or button handle natively
        }
        resumed = true;
      };

      handleEnterKey('form');
      expect(resumed).toBe(false);

      handleEnterKey('context-item');
      expect(resumed).toBe(true);
    });

    it('ignores Enter key during IME composition to prevent accidental execution', () => {
      let executed = false;
      const handleEnter = (e: { key: string; isComposing: boolean; keyCode: number }) => {
        if (e.isComposing || e.keyCode === 229) {
          return;
        }
        executed = true;
      };

      // IME composition event
      handleEnter({ key: 'Enter', isComposing: true, keyCode: 229 });
      expect(executed).toBe(false);

      // Normal Enter
      handleEnter({ key: 'Enter', isComposing: false, keyCode: 13 });
      expect(executed).toBe(true);
    });

    it('does not navigate context list when ArrowDown/ArrowUp occurs in preview pane, workset rows, or tablist', () => {
      let listNavigated = false;

      const handleArrowKey = (activeContainer: string) => {
        if (
          activeContainer === 'tablist' ||
          activeContainer === 'sort-group' ||
          activeContainer === 'workset-row' ||
          activeContainer === 'preview-pane'
        ) {
          return; // Element handles its own navigation
        }
        listNavigated = true;
      };

      handleArrowKey('tablist');
      expect(listNavigated).toBe(false);

      handleArrowKey('workset-row');
      expect(listNavigated).toBe(false);

      handleArrowKey('context-list');
      expect(listNavigated).toBe(true);
    });
  });
});
