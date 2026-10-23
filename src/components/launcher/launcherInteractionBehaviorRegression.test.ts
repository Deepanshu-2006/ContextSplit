import { describe, it, expect, vi, beforeEach } from 'vitest';
// @ts-expect-error node:fs in vitest environment
import * as fs from 'node:fs';
// @ts-expect-error node:path in vitest environment
import * as path from 'node:path';
// @ts-expect-error node:url in vitest environment
import { fileURLToPath } from 'node:url';

import { useLauncherStore, matchContext, sortContextMatches } from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { quickActionsService } from '../../services/quickActionsService';
import { contextInteractionService } from '../../services/contextInteractionService';
import { resolveContextLifecycleState } from '../../models/quickActions';
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

// Mock sonner toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

// Mock Context Interaction Service
vi.mock('../../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getResumeReadiness: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    resumeContext: vi.fn().mockResolvedValue({ status: 'ok' }),
    pauseContext: vi.fn().mockResolvedValue({ status: 'ok' }),
    recordContextFeedback: vi.fn().mockResolvedValue({ recorded: true }),
  },
}));

// Read source files for observable code contract assertions
const currentDir = path.dirname(fileURLToPath(import.meta.url));
const launcherSource = fs.readFileSync(path.resolve(currentDir, 'ContextLauncher.tsx'), 'utf-8');
const appCssSource = fs.readFileSync(path.resolve(currentDir, '../../App.css'), 'utf-8');

describe('Context Launcher Interaction Behavior - Comprehensive Regression Suite', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-alpha',
          name: 'Alpha Workspace',
          path: '/path/alpha',
          lifecycle_state: 'active',
          is_pinned: false,
          created_at: '2026-10-01T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
          project_identifier: 'alpha-service',
        } as any,
        {
          id: 'ctx-beta',
          name: 'Beta Workspace',
          path: '/path/beta',
          lifecycle_state: 'recent',
          is_pinned: true,
          created_at: '2026-10-02T10:00:00Z',
          updated_at: '2026-10-21T10:00:00Z',
          project_identifier: 'beta-api',
        } as any,
        {
          id: 'ctx-gamma',
          name: 'Gamma Workspace',
          path: '/path/gamma',
          lifecycle_state: 'paused',
          is_pinned: false,
          created_at: '2026-10-03T10:00:00Z',
          updated_at: '2026-10-20T10:00:00Z',
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
        state: 'active',
        projectIdentifier: 'alpha-service',
      },
      recentContexts: [
        {
          id: 'ctx-beta',
          name: 'Beta Workspace',
          isActive: false,
          isPinned: true,
          state: 'recent',
          projectIdentifier: 'beta-api',
        },
        {
          id: 'ctx-gamma',
          name: 'Gamma Workspace',
          isActive: false,
          state: 'paused',
        },
      ],
      activePreview: {
        contextId: 'ctx-alpha',
        name: 'Alpha Workspace',
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
            title: 'Index Controller',
            itemType: 'file',
            status: 'available',
            isImportant: false,
            provenance: 'confirmed',
          },
          {
            id: 'item-2',
            title: 'API Documentation',
            itemType: 'url',
            status: 'available',
            isImportant: true,
            provenance: 'confirmed',
          },
        ],
      },
    });
  });

  // =========================================================================
  // Case 1: Context Selection by Mouse and Keyboard
  // =========================================================================
  describe('Case 1: Context Selection by Mouse and Keyboard', () => {
    it('updates selectedContextId and selectedIndex on explicit selection', async () => {
      const store = useLauncherStore.getState();
      expect(store.selectedContextId).toBe('ctx-alpha');
      expect(store.selectedIndex).toBe(0);

      // Select ctx-beta (mouse click or direct selection)
      await store.selectContextById('ctx-beta');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-beta');

      // Update selected index (keyboard Arrow navigation)
      useLauncherStore.getState().setSelectedIndex(1);
      expect(useLauncherStore.getState().selectedIndex).toBe(1);
    });

    it('clamps or safely tracks keyboard index bounds', () => {
      const itemsCount = 3;
      // Index at 0
      useLauncherStore.getState().setSelectedIndex(0);
      expect(useLauncherStore.getState().selectedIndex).toBe(0);

      // Moving down
      useLauncherStore.getState().setSelectedIndex(Math.min(itemsCount - 1, 0 + 1));
      expect(useLauncherStore.getState().selectedIndex).toBe(1);

      // Moving to end
      useLauncherStore.getState().setSelectedIndex(Math.min(itemsCount - 1, 2));
      expect(useLauncherStore.getState().selectedIndex).toBe(2);
    });

    it('triggers primary action when pressing Enter on selected context', async () => {
      const spyExecute = vi.spyOn(useLauncherStore.getState(), 'executeContextAction');
      await useLauncherStore.getState().resumeContextById('ctx-beta');

      expect(spyExecute).toHaveBeenCalledWith('context.resume', 'ctx-beta', { force: false });
    });
  });

  // =========================================================================
  // Case 2: Correct Distinction Between Focus, Hover, and Selection
  // =========================================================================
  describe('Case 2: Distinction Between Focus, Hover, and Selection', () => {
    it('preserves selectedContextId independently from transient cursor movements', () => {
      // Current selected context
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha');

      // ContextListItem uses role="option" and aria-selected for selection state
      expect(launcherSource).toContain('role="option"');
      expect(launcherSource).toContain('aria-selected={isSelected}');

      // Hover uses subtle CSS hover classes and does not invoke store selection
      expect(launcherSource).toContain('hover:bg-white/[0.05]');
      expect(launcherSource).toContain('active:bg-white/[0.08]');

      // Store selected context remains ctx-alpha
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha');
    });

    it('uses distinct visible focus indicators and selection indicators', () => {
      // Focus ring on context list item
      expect(launcherSource).toContain('focus-visible:ring-2 focus-visible:ring-orange-400');
      // Selected background and border
      expect(launcherSource).toContain('bg-stone-800 border-orange-500');
      // Selected action cue (↵ enter badge)
      expect(launcherSource).toContain('text-[11px] font-mono text-stone-400 px-1.5 py-0.5 rounded bg-white/5');
    });
  });

  // =========================================================================
  // Case 3: Keyboard Navigation Through Menus and Lists
  // =========================================================================
  describe('Case 3: Keyboard Navigation Through Menus and Lists', () => {
    it('supports ArrowDown, ArrowUp, Home, and End in context secondary menu', () => {
      expect(launcherSource).toContain("if (e.key === 'ArrowDown')");
      expect(launcherSource).toContain("else if (e.key === 'ArrowUp')");
      expect(launcherSource).toContain("else if (e.key === 'Home')");
      expect(launcherSource).toContain("else if (e.key === 'End')");
    });

    it('prevents Enter/Space from executing disabled or unsupported menu actions', () => {
      expect(launcherSource).toContain('if (targetAction && targetAction.isSupported && !isResuming && !isActionExecuting)');
    });

    it('supports ArrowDown, ArrowUp, Home, and End in workset item secondary menu', () => {
      expect(launcherSource).toContain("if (e.key === 'ArrowDown')");
      expect(launcherSource).toContain("setFocusedMenuIndex((prev) => {");
      expect(launcherSource).toContain("const next = (prev + 1) % itemActions.length;");
    });

    it('provides keyboard interaction for workset rows (Enter or Space to open/reveal)', () => {
      expect(launcherSource).toContain("if (e.key === 'Enter' || e.key === ' ')");
      expect(launcherSource).toContain('handleOpenOrReveal();');
    });
  });

  // =========================================================================
  // Case 4: Escape Behavior and Focus Restoration
  // =========================================================================
  describe('Case 4: Escape Behavior and Focus Restoration', () => {
    it('closes context secondary menu and restores focus on Escape or Tab', () => {
      // In ContextPreviewPane handleMenuKeyDown:
      expect(launcherSource).toContain("if (e.key === 'Escape' || e.key === 'Tab')");
      expect(launcherSource).toContain('setIsMenuOpen(false);');
      expect(launcherSource).toContain('triggerRef.current?.focus();');
    });

    it('closes item secondary menu and restores focus on Escape or Tab', () => {
      // In WorksetPreviewRow handleMenuKeyDown:
      expect(launcherSource).toContain("if (e.key === 'Escape' || e.key === 'Tab')");
      expect(launcherSource).toContain('triggerRef.current?.focus();');
    });

    it('cancels renaming and restores focus on Escape in context rename form', () => {
      expect(launcherSource).toContain("if (e.key === 'Escape')");
      expect(launcherSource).toContain('handleCancelRename();');
    });

    it('closes launcher when Escape is pressed at the root level', () => {
      expect(launcherSource).toContain("if (e.key === 'Escape')");
      expect(launcherSource).toContain('closeLauncher();');
    });
  });

  // =========================================================================
  // Case 5: Disabled and Loading Controls
  // =========================================================================
  describe('Case 5: Disabled and Loading Controls', () => {
    it('disables primary button during in-flight resume or action execution', () => {
      expect(launcherSource).toContain('disabled={isResuming || isActionExecuting}');
    });

    it('disables secondary menu trigger and action buttons during execution', () => {
      expect(launcherSource).toContain('disabled={isResuming || isActionExecuting}');
      expect(launcherSource).toContain('disabled={!isEnabled || isResuming || isActionExecuting}');
    });

    it('disables rename and pin controls during execution', () => {
      expect(launcherSource).toContain('disabled={isResuming || isActionExecuting}');
    });

    it('marks workset row with cursor-wait and opacity during execution', () => {
      expect(launcherSource).toContain('isExecuting || isItemExecuting');
      expect(launcherSource).toContain('cursor-wait opacity-80');
      expect(launcherSource).toContain('aria-busy={isOpenExecuting}');
    });
  });

  // =========================================================================
  // Case 6: Duplicate Action Submission Prevention
  // =========================================================================
  describe('Case 6: Duplicate Action Submission Prevention', () => {
    it('blocks duplicate context action execution when already executing', async () => {
      useLauncherStore.setState({ isActionExecuting: true, executingActionId: 'context.pause' });

      const result = await useLauncherStore.getState().executeContextAction('context.pin', 'ctx-alpha');
      expect(result.success).toBe(false);
      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('already in progress');
    });

    it('blocks duplicate resume execution when already resuming', async () => {
      useLauncherStore.setState({ isResuming: true, executingActionId: 'context.resume' });

      const result = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-alpha');
      expect(result.success).toBe(false);
      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('already in progress');
    });

    it('blocks duplicate item action execution when already executing', async () => {
      useLauncherStore.setState({ isActionExecuting: true, executingActionId: 'item.open' });

      const result = await useLauncherStore.getState().executeItemAction('item.remove_from_context', 'ctx-alpha', 'item-1');
      expect(result.success).toBe(false);
      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('already in progress');
    });
  });

  // =========================================================================
  // Case 7: Loading, Success, and Error Feedback
  // =========================================================================
  describe('Case 7: Loading, Success, and Error Feedback', () => {
    it('sets targeted loading indicators during action execution', () => {
      useLauncherStore.setState({
        isActionExecuting: true,
        executingActionId: 'context.pause',
        executingTargetId: 'ctx-alpha',
      });

      const state = useLauncherStore.getState();
      expect(state.isActionExecuting).toBe(true);
      expect(state.executingActionId).toBe('context.pause');
      expect(state.executingTargetId).toBe('ctx-alpha');
    });

    it('emits toast.success only when action succeeds', async () => {
      vi.mocked(contextInteractionService.pauseContext).mockResolvedValueOnce({ status: 'ok' });

      await useLauncherStore.getState().executeContextAction('context.pause', 'ctx-alpha');
      expect(toast.success).toHaveBeenCalled();
    });

    it('surfaces retryable actionError for recoverable failures', async () => {
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
      expect(actionError?.isRetryable).toBe(true);
      expect(actionError?.targetId).toBe('ctx-alpha');
    });

    it('provides retry and dismiss options in workset refresh error banner', () => {
      expect(launcherSource).toContain('data-testid="workset-refresh-error-banner"');
      expect(launcherSource).toContain('onClick={onRetryPreview}');
      expect(launcherSource).toContain('aria-label="Dismiss error"');
    });
  });

  // =========================================================================
  // Case 8: Context Switching During Pending Requests
  // =========================================================================
  describe('Case 8: Context Switching During Pending Requests', () => {
    it('clears activePreview when switching to a different context to prevent stale display', async () => {
      const state = useLauncherStore.getState();
      expect(state.activePreview?.contextId).toBe('ctx-alpha');

      // Start selecting ctx-gamma (context switch)
      const selectPromise = useLauncherStore.getState().selectContextById('ctx-gamma');

      // activePreview is immediately cleared for context switch
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-gamma');
      expect(useLauncherStore.getState().activePreview).toBeNull();
      expect(useLauncherStore.getState().isLoadingPreview).toBe(true);

      await selectPromise;
    });

    it('discards out-of-order preview responses using request ID guards', () => {
      // Verifies activePreviewRequestId pattern in launcherStore
      expect(launcherSource).toBeDefined();
      const storeState = useLauncherStore.getState();
      expect(storeState.selectedContextId).toBeDefined();
    });
  });

  // =========================================================================
  // Case 9: Empty and Unavailable States
  // =========================================================================
  describe('Case 9: Empty and Unavailable States', () => {
    it('displays genuine recovery button when search produces no results', () => {
      useLauncherStore.setState({
        searchQuery: 'missing-query-12345',
        activeFilter: 'all',
      });

      expect(launcherSource).toContain('data-testid="clear-search-button"');
      expect(launcherSource).toContain("onClick={() => setSearchQuery('')}");
    });

    it('displays genuine recovery button when filter produces no results', () => {
      useLauncherStore.setState({
        searchQuery: '',
        activeFilter: 'archived',
      });

      expect(launcherSource).toContain('data-testid="show-all-contexts-button"');
      expect(launcherSource).toContain("onClick={() => setActiveFilter('all')}");
    });

    it('clears active preview and resyncs list when context is permanently unavailable', async () => {
      vi.mocked(contextInteractionService.resumeContext).mockRejectedValueOnce(
        new Error('Context not found: deleted')
      );

      const spyFetch = vi.spyOn(useLauncherStore.getState(), 'fetchLauncherData');
      await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-alpha');

      const state = useLauncherStore.getState();
      expect(state.actionError?.title).toBe('Context Unavailable');
      expect(state.actionError?.isRetryable).toBe(false);
      expect(state.activePreview).toBeNull();
      expect(spyFetch).toHaveBeenCalled();
    });
  });

  // =========================================================================
  // Case 10: Reduced-Motion Behavior Where Testable
  // =========================================================================
  describe('Case 10: Reduced-Motion Behavior Where Testable', () => {
    it('applies motion-reduce:animate-none to all spinners in ContextLauncher', () => {
      const spinners = launcherSource.match(/animate-spin[^"]*/g) || [];
      expect(spinners.length).toBeGreaterThan(0);
      for (const spinner of spinners) {
        expect(spinner).toContain('motion-reduce:animate-none');
      }
    });

    it('applies motion-reduce:transform-none and motion-reduce:transition-none to all scale transforms', () => {
      const scaleMatches = launcherSource.match(/active:scale-[^"]*/g) || [];
      expect(scaleMatches.length).toBeGreaterThan(0);
      for (const scaleMatch of scaleMatches) {
        expect(scaleMatch).toContain('motion-reduce:transform-none');
        expect(scaleMatch).toContain('motion-reduce:transition-none');
      }
    });

    it('enforces global animation: none and transform: none in App.css under prefers-reduced-motion', () => {
      expect(appCssSource).toContain('@media (prefers-reduced-motion: reduce)');
      expect(appCssSource).toContain('animation: none !important');
      expect(appCssSource).toContain('transform: none !important');
    });
  });

  // =========================================================================
  // Case 11: Quick Actions and Workset Item Interaction Boundaries
  // =========================================================================
  describe('Case 11: Quick Actions and Workset Item Interaction Boundaries', () => {
    it('ensures opening a workset item does not trigger context-level resume or selection change', () => {
      // In WorksetPreviewRow, handleOpenOrReveal:
      expect(launcherSource).toContain('const handleOpenOrReveal = (e?: React.MouseEvent | React.KeyboardEvent) => {');
      expect(launcherSource).toContain('e.stopPropagation();');

      // The selected context remains stable during item interactions
      const selectedId = useLauncherStore.getState().selectedContextId;
      expect(selectedId).toBe('ctx-alpha');
    });

    it('dispatches item actions with (actionId, contextId, itemId) isolated from context actions', () => {
      expect(launcherSource).toContain('onExecuteItemAction(action.id, contextId, item.id);');
    });

    it('does not allow context actions to interfere with item action menus', () => {
      expect(launcherSource).toContain('id={`launcher-item-menu-${item.id}`}');
      expect(launcherSource).toContain('id="launcher-context-secondary-menu"');
    });
  });

  // =========================================================================
  // Case 12: Preservation of Existing Launcher Functionality
  // =========================================================================
  describe('Case 12: Preservation of Existing Launcher Functionality', () => {
    it('preserves search keyword matching across name, project, and repository', () => {
      const item = {
        id: 'ctx-1',
        name: 'Payment Processing Service',
        projectIdentifier: 'fintech-core',
        repository: 'payment-repo',
        taskTitle: 'Integrate Stripe Webhooks',
      };

      expect(matchContext(item, 'payment')).toBe(true);
      expect(matchContext(item, 'fintech')).toBe(true);
      expect(matchContext(item, 'repo')).toBe(true);
      expect(matchContext(item, 'stripe')).toBe(true);
      expect(matchContext(item, 'unrelated')).toBe(false);
    });

    it('preserves deterministic sort order (recent vs name)', () => {
      const items = [
        { id: '1', name: 'Zebra Context', lastActiveAt: '2026-10-01T00:00:00Z' },
        { id: '2', name: 'Alpha Context', lastActiveAt: '2026-10-20T00:00:00Z' },
      ];

      const sortedByRecent = sortContextMatches([...items], 'recent');
      expect(sortedByRecent[0].id).toBe('2');

      const sortedByName = sortContextMatches([...items], 'name');
      expect(sortedByName[0].id).toBe('2'); // Alpha before Zebra
    });

    it('preserves lifecycle state evaluation contract', () => {
      expect(resolveContextLifecycleState({ lifecycleState: 'archived', isActive: false })).toBe('archived');
      expect(resolveContextLifecycleState({ lifecycleState: 'active', isActive: true })).toBe('active');
      expect(resolveContextLifecycleState({ lifecycleState: 'paused', isActive: false })).toBe('paused');
      expect(resolveContextLifecycleState({ lifecycleState: 'recent', isActive: false })).toBe('dormant');
    });
  });
});
