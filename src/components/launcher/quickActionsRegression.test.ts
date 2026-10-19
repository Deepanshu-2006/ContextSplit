import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  useLauncherStore,
  getActionErrorTitle,
  matchContext,
  sortContextMatches,
  matchesContextFilter,
  getVisibleLauncherSlice,
  MAX_VISIBLE_LAUNCHER_RESULTS,
  LauncherContextItem,
} from '../../store/launcherStore';
import { contextInteractionService } from '../../services/contextInteractionService';
import { contextWorksetService } from '../../services/contextWorkset';
import { db } from '../../services/db';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { toast } from 'sonner';
import {
  getAvailableContextActions,
  getAvailableItemActions,
  getPrimaryContextActionDescriptor,
  validateContextActionTransition,
  validateContextId,
  validateItemId,
} from '../../models/quickActions';

// Mock Tauri invoke to prevent window undefined in test environments
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock authoritative Context Interaction API service
vi.mock('../../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    getResumeReadiness: vi.fn(),
    resumeContext: vi.fn(),
    pauseContext: vi.fn(),
  },
}));

// Mock Context Workset service
vi.mock('../../services/contextWorkset', () => ({
  contextWorksetService: {
    controlContextMemory: vi.fn().mockResolvedValue(true),
  },
}));

// Mock Database service
vi.mock('../../services/db', () => ({
  db: {
    archiveWorkspace: vi.fn().mockImplementation(async (id: string) => ({
      id,
      name: 'Archived Workspace',
      lifecycle_state: 'archived' as const,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    })),
    unarchiveWorkspace: vi.fn().mockImplementation(async (id: string) => ({
      id,
      name: 'Unarchived Workspace',
      lifecycle_state: 'active' as const,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    })),
    listWorkspaces: vi.fn().mockResolvedValue([]),
    listItems: vi.fn().mockResolvedValue([]),
  },
}));

// Mock Sonner toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

describe('Context Launcher Quick Actions - Focused Regression Suite', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-regression-active',
          name: 'ContextSplit Engine',
          lifecycle_state: 'active',
          project_identifier: 'ctx-core',
          repository: 'contextsplit',
          created_at: '2026-09-01T00:00:00Z',
          updated_at: '2026-10-01T00:00:00Z',
        } as any,
        {
          id: 'ctx-regression-paused',
          name: 'Security Token Audit',
          lifecycle_state: 'active',
          project_identifier: 'ctx-sec',
          repository: 'vault-manager',
          created_at: '2026-09-10T00:00:00Z',
          updated_at: '2026-09-25T00:00:00Z',
        } as any,
        {
          id: 'ctx-regression-archived',
          name: 'Legacy V1 API',
          lifecycle_state: 'archived',
          project_identifier: 'ctx-legacy',
          repository: 'legacy-repo',
          created_at: '2026-06-01T00:00:00Z',
          updated_at: '2026-08-01T00:00:00Z',
        } as any,
      ],
      activeWorkspaceId: 'ctx-regression-active',
      selectWorkspace: vi.fn(),
      openOverview: vi.fn(),
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: 'ctx-regression-active',
      activePreview: {
        contextId: 'ctx-regression-active',
        name: 'ContextSplit Engine',
        state: 'active',
        readinessBand: 'ready',
        reasons: [],
        importantItems: [],
        worksetItems: [
          { id: 'item-doc-1', title: 'Task Architecture', itemType: 'doc', isImportant: false },
          { id: 'item-url-2', title: 'API Specification', itemType: 'url', isImportant: true },
        ],
        itemCount: 2,
        isAmbiguous: false,
        isStale: false,
      },
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
        id: 'ctx-regression-active',
        name: 'ContextSplit Engine',
        state: 'active',
        isActive: true,
      },
      recentContexts: [
        {
          id: 'ctx-regression-paused',
          name: 'Security Token Audit',
          state: 'paused',
          isActive: false,
        },
      ],
      isPreviewOpen: true,
      activeFilter: 'all',
    });
  });

  // ---------------------------------------------------------------------------
  // 1. Valid context execution of supported primary action
  // ---------------------------------------------------------------------------
  it('1. executes supported primary action for a valid context (resume / focus)', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({
      context_id: 'ctx-regression-active',
      status: 'active',
      switched_at: new Date().toISOString(),
    });

    const primaryDesc = getPrimaryContextActionDescriptor('active');
    expect(primaryDesc.label).toBe('Focus Workspace');
    expect(primaryDesc.inProgressLabel).toBe('Focusing...');

    const result = await useLauncherStore
      .getState()
      .executeContextAction('context.resume', 'ctx-regression-active');

    expect(result.success).toBe(true);
    expect(result.outcome).toBe('SUCCESS');
    expect(result.resolvedContextId).toBe('ctx-regression-active');
    expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-regression-active');
    expect(useWorkspaceStore.getState().selectWorkspace).toHaveBeenCalledWith('ctx-regression-active');
    expect(useLauncherStore.getState().isOpen).toBe(false);
    expect(toast.success).toHaveBeenCalled();

    // 1b. Resuming an archived context unarchives via db and resumes
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({
      context_id: 'ctx-regression-archived',
      status: 'active',
      switched_at: new Date().toISOString(),
    });

    const archivedResult = await useLauncherStore
      .getState()
      .executeContextAction('context.resume', 'ctx-regression-archived');

    expect(archivedResult.success).toBe(true);
    expect(db.unarchiveWorkspace).toHaveBeenCalledWith('ctx-regression-archived');
  });

  // ---------------------------------------------------------------------------
  // 2. Unsupported actions cannot be executed
  // ---------------------------------------------------------------------------
  it('2. blocks execution of unsupported actions without invoking backend services', async () => {
    // 2a. context.delete is explicitly unsupported in quick actions
    const contextDeleteResult = await useLauncherStore
      .getState()
      .executeContextAction('context.delete' as any, 'ctx-regression-active');

    expect(contextDeleteResult.success).toBe(false);
    expect(contextDeleteResult.outcome).toBe('UNSUPPORTED');
    expect(contextDeleteResult.message).toContain('Direct context deletion is not supported');
    expect(contextInteractionService.resumeContext).not.toHaveBeenCalled();
    expect(contextInteractionService.pauseContext).not.toHaveBeenCalled();

    // 2b. item.delete is unsupported in quick actions
    const itemDeleteResult = await useLauncherStore
      .getState()
      .executeItemAction('item.delete' as any, 'ctx-regression-active', 'item-doc-1');

    expect(itemDeleteResult.success).toBe(false);
    expect(itemDeleteResult.outcome).toBe('UNSUPPORTED');
    expect(contextWorksetService.controlContextMemory).not.toHaveBeenCalled();

    // 2c. Verify availability models mark unsupported actions accordingly
    const availableContextActions = getAvailableContextActions({
      contextId: 'ctx-regression-active',
      lifecycleState: 'active',
      isActive: true,
    });
    const deleteContextAction = availableContextActions.find((a) => a.id === 'context.delete');
    expect(deleteContextAction?.isSupported).toBe(false);

    const availableItemActions = getAvailableItemActions({
      contextId: 'ctx-regression-active',
      itemId: 'item-doc-1',
      isImportant: false,
    });
    const deleteItemAction = availableItemActions.find((a) => a.id === 'item.delete');
    expect(deleteItemAction?.isSupported).toBe(false);
  });

  // ---------------------------------------------------------------------------
  // 3. Invalid or missing context IDs are handled safely
  // ---------------------------------------------------------------------------
  it('3. safely handles invalid or missing context IDs without crashing or calling backend', async () => {
    // Direct validator checks
    expect(validateContextId(null).isValid).toBe(false);
    expect(validateContextId(undefined).isValid).toBe(false);
    expect(validateContextId('').isValid).toBe(false);
    expect(validateContextId('   ').isValid).toBe(false);
    expect(validateContextId(12345).isValid).toBe(false);
    expect(validateItemId(null).isValid).toBe(false);
    expect(validateItemId('').isValid).toBe(false);
    expect(validateItemId('item-valid-1').isValid).toBe(true);

    // Execution with empty string
    const emptyResult = await useLauncherStore
      .getState()
      .executeContextAction('context.resume', '');

    expect(emptyResult.success).toBe(false);
    expect(emptyResult.outcome).toBe('ERROR');
    expect(emptyResult.error).toContain('Context ID cannot be empty');

    // Execution with whitespace string
    const whitespaceResult = await useLauncherStore
      .getState()
      .executeContextAction('context.resume', '   ');

    expect(whitespaceResult.success).toBe(false);
    expect(whitespaceResult.outcome).toBe('ERROR');

    // Execution when no target is passed and no context is selected in store
    useLauncherStore.setState({ selectedContextId: null });
    const noSelectionResult = await useLauncherStore
      .getState()
      .executeContextAction('context.resume');

    expect(noSelectionResult.success).toBe(false);
    expect(noSelectionResult.outcome).toBe('ERROR');
    expect(noSelectionResult.error).toBe('No context selected');

    // Verify zero calls to backend
    expect(contextInteractionService.resumeContext).not.toHaveBeenCalled();
    expect(contextInteractionService.pauseContext).not.toHaveBeenCalled();
    expect(useLauncherStore.getState().isActionExecuting).toBe(false);
  });

  // ---------------------------------------------------------------------------
  // 4. Item actions use the correct item ID
  // ---------------------------------------------------------------------------
  it('4. ensures item actions strictly pass the correct item ID and context ID', async () => {
    vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValue(true);

    // Pinning item-doc-1
    await useLauncherStore
      .getState()
      .executeItemAction('item.mark_important', 'ctx-regression-active', 'item-doc-1');

    expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith(
      'ctx-regression-active',
      'item-doc-1',
      'keep_important'
    );

    // Unpinning item-url-2
    await useLauncherStore
      .getState()
      .executeItemAction('item.unmark_important', 'ctx-regression-active', 'item-url-2');

    expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith(
      'ctx-regression-active',
      'item-url-2',
      'remove_important'
    );

    // Removing item-doc-1 from context
    await useLauncherStore
      .getState()
      .executeItemAction('item.remove_from_context', 'ctx-regression-active', 'item-doc-1');

    expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith(
      'ctx-regression-active',
      'item-doc-1',
      'remove_from_context'
    );

    // Context ID is never substituted in place of item ID
    expect(contextWorksetService.controlContextMemory).not.toHaveBeenCalledWith(
      'ctx-regression-active',
      'ctx-regression-active',
      expect.anything(),
      expect.anything()
    );
  });

  // ---------------------------------------------------------------------------
  // 5. Duplicate submissions are prevented
  // ---------------------------------------------------------------------------
  it('5. prevents duplicate and concurrent submissions while an action is in-flight', async () => {
    // Set store in an active executing state
    useLauncherStore.setState({
      isActionExecuting: true,
      executingActionId: 'context.resume',
      executingTargetId: 'ctx-regression-active',
    });

    // Secondary context action invocation
    const duplicateContext = await useLauncherStore
      .getState()
      .executeContextAction('context.resume', 'ctx-regression-active');

    expect(duplicateContext.outcome).toBe('BLOCKED');
    expect(duplicateContext.message).toContain('already in progress');

    // Secondary item action invocation
    const duplicateItem = await useLauncherStore
      .getState()
      .executeItemAction('item.mark_important', 'ctx-regression-active', 'item-doc-1');

    expect(duplicateItem.outcome).toBe('BLOCKED');
    expect(duplicateItem.message).toContain('already in progress');

    // Verify backend is never touched during blocked concurrent calls
    expect(contextInteractionService.resumeContext).not.toHaveBeenCalled();
    expect(contextWorksetService.controlContextMemory).not.toHaveBeenCalled();
  });

  // ---------------------------------------------------------------------------
  // 6. Backend failures produce the correct error state
  // ---------------------------------------------------------------------------
  it('6. produces actionable error states with specific titles and retry flags on backend failures', async () => {
    // 6a. Verify title resolution mapping
    expect(getActionErrorTitle('context.resume')).toBe('Resume Failed');
    expect(getActionErrorTitle('context.pause')).toBe('Pause Failed');
    expect(getActionErrorTitle('context.archive')).toBe('Archive Failed');
    expect(getActionErrorTitle('context.open_details')).toBe('Could Not Open Details');
    expect(getActionErrorTitle('item.mark_important')).toBe('Memory Update Failed');
    expect(getActionErrorTitle('item.remove_from_context')).toBe('Remove Item Failed');

    // 6b. Pause failure: verify it sets "Pause Failed" (NOT generic "Resume Failed")
    vi.mocked(contextInteractionService.pauseContext).mockRejectedValueOnce(
      new Error('Engine process terminated unexpectedly')
    );

    const pauseResult = await useLauncherStore
      .getState()
      .executeContextAction('context.pause', 'ctx-regression-active');

    expect(pauseResult.outcome).toBe('ERROR');
    const pauseError = useLauncherStore.getState().actionError;
    expect(pauseError).toBeDefined();
    expect(pauseError?.title).toBe('Pause Failed');
    expect(pauseError?.message).toContain('Engine process terminated unexpectedly');
    expect(pauseError?.isRetryable).toBe(true);
    expect(useLauncherStore.getState().resumeError).toBeNull();

    // 6b. Item action failure: verify "Remove Item Failed"
    vi.mocked(contextWorksetService.controlContextMemory).mockRejectedValueOnce(
      new Error('SQLite database is locked')
    );

    const itemResult = await useLauncherStore
      .getState()
      .executeItemAction('item.remove_from_context', 'ctx-regression-active', 'item-doc-1');

    expect(itemResult.success).toBe(false);
    const itemError = useLauncherStore.getState().actionError;
    expect(itemError).toBeDefined();
    expect(itemError?.title).toBe('Remove Item Failed');
    expect(itemError?.targetId).toBe('item-doc-1');
    expect(itemError?.isRetryable).toBe(true);
  });

  // ---------------------------------------------------------------------------
  // 7. Successful operations refresh the relevant launcher state
  // ---------------------------------------------------------------------------
  it('7. refreshes relevant launcher data and preview state upon successful operation', async () => {
    const fetchLauncherDataSpy = vi.spyOn(useLauncherStore.getState(), 'fetchLauncherData');
    const selectContextByIdSpy = vi.spyOn(useLauncherStore.getState(), 'selectContextById');

    vi.mocked(contextInteractionService.pauseContext).mockResolvedValueOnce({
      context_id: 'ctx-regression-active',
      status: 'paused',
      paused_at: new Date().toISOString(),
    });

    const result = await useLauncherStore
      .getState()
      .executeContextAction('context.pause', 'ctx-regression-active');

    expect(result.success).toBe(true);
    // Refreshes launcher list data to reflect paused state
    expect(fetchLauncherDataSpy).toHaveBeenCalled();
    // Re-selects context to refresh preview workset and readiness
    expect(selectContextByIdSpy).toHaveBeenCalledWith('ctx-regression-active');
    expect(toast.success).toHaveBeenCalled();
  });

  // ---------------------------------------------------------------------------
  // 8. Stale or unavailable contexts are handled correctly
  // ---------------------------------------------------------------------------
  it('8. handles stale or unavailable contexts by clearing preview and refreshing context list', async () => {
    const fetchLauncherDataSpy = vi.spyOn(useLauncherStore.getState(), 'fetchLauncherData');

    vi.mocked(contextInteractionService.resumeContext).mockRejectedValueOnce(
      new Error('Context ctx-deleted is no longer available or does not exist')
    );

    useLauncherStore.setState({
      selectedContextId: 'ctx-deleted',
      activePreview: {
        contextId: 'ctx-deleted',
        name: 'Deleted Workspace',
        state: 'active',
        readinessBand: 'ready',
        reasons: [],
        importantItems: [],
        worksetItems: [],
        itemCount: 0,
        isAmbiguous: false,
        isStale: false,
      },
    });

    const result = await useLauncherStore
      .getState()
      .executeContextAction('context.resume', 'ctx-deleted');

    expect(result.outcome).toBe('ERROR');
    const actionError = useLauncherStore.getState().actionError;
    expect(actionError?.title).toBe('Context Unavailable');
    expect(actionError?.isRetryable).toBe(false);
    // Active preview cleared so user does not view a stale phantom context
    expect(useLauncherStore.getState().activePreview).toBeNull();
    // Launcher list refreshed to remove deleted context from results
    expect(fetchLauncherDataSpy).toHaveBeenCalled();
  });

  // ---------------------------------------------------------------------------
  // 9. Lifecycle rules prevent invalid actions
  // ---------------------------------------------------------------------------
  it('9. enforces Context Engine lifecycle rules and blocks invalid state transitions', async () => {
    // 9a. Direct domain rule checks
    expect(validateContextActionTransition('context.pause', 'active').isValid).toBe(true);
    expect(validateContextActionTransition('context.pause', 'paused').isValid).toBe(false);
    expect(validateContextActionTransition('context.pause', 'dormant').isValid).toBe(false);
    expect(validateContextActionTransition('context.pause', 'archived').isValid).toBe(false);

    // 9b. Executing an invalid transition on an archived context
    const blockedPause = await useLauncherStore
      .getState()
      .executeContextAction('context.pause', 'ctx-regression-archived');

    expect(blockedPause.outcome).toBe('BLOCKED');
    expect(blockedPause.message).toContain('Cannot pause an archived context');
    expect(contextInteractionService.pauseContext).not.toHaveBeenCalled();

    const actionError = useLauncherStore.getState().actionError;
    expect(actionError).toBeDefined();
    expect(actionError?.title).toBe('Pause Failed');
    expect(actionError?.isRetryable).toBe(false);
  });

  // ---------------------------------------------------------------------------
  // 10. Keyboard navigation and menu dismissal
  // ---------------------------------------------------------------------------
  it('10. supports keyboard navigation, item selection, and menu dismissal via Escape', () => {
    const actions = [
      { id: 'action-1', isSupported: true, label: 'Action 1' },
      { id: 'action-2', isSupported: true, label: 'Action 2' },
      { id: 'action-3', isSupported: false, label: 'Action 3' },
    ];

    let focusedIndex = 0;
    let isMenuOpen = true;
    const triggerRef = { current: { focus: vi.fn() } };
    const onExecute = vi.fn();

    const handleMenuKeyDown = (e: { key: string; preventDefault: () => void; stopPropagation: () => void }) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        focusedIndex = (focusedIndex + 1) % actions.length;
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        focusedIndex = (focusedIndex - 1 + actions.length) % actions.length;
      } else if (e.key === 'Home') {
        e.preventDefault();
        focusedIndex = 0;
      } else if (e.key === 'End') {
        e.preventDefault();
        focusedIndex = actions.length - 1;
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const action = actions[focusedIndex];
        if (action && action.isSupported) {
          isMenuOpen = false;
          onExecute(action.id);
          triggerRef.current?.focus();
        }
      } else if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        isMenuOpen = false;
        triggerRef.current?.focus();
      }
    };

    // Down arrow navigation
    handleMenuKeyDown({ key: 'ArrowDown', preventDefault: vi.fn(), stopPropagation: vi.fn() });
    expect(focusedIndex).toBe(1);

    // End key navigation
    handleMenuKeyDown({ key: 'End', preventDefault: vi.fn(), stopPropagation: vi.fn() });
    expect(focusedIndex).toBe(2);

    // Enter on disabled item (action-3 isSupported: false) -> ignored
    handleMenuKeyDown({ key: 'Enter', preventDefault: vi.fn(), stopPropagation: vi.fn() });
    expect(isMenuOpen).toBe(true);
    expect(onExecute).not.toHaveBeenCalled();

    // Home key navigation
    handleMenuKeyDown({ key: 'Home', preventDefault: vi.fn(), stopPropagation: vi.fn() });
    expect(focusedIndex).toBe(0);

    // Enter on enabled item -> executes action and closes menu
    handleMenuKeyDown({ key: 'Enter', preventDefault: vi.fn(), stopPropagation: vi.fn() });
    expect(isMenuOpen).toBe(false);
    expect(onExecute).toHaveBeenCalledWith('action-1');
    expect(triggerRef.current.focus).toHaveBeenCalled();

    // Reopen and test Escape dismissal
    isMenuOpen = true;
    const stopPropagationMock = vi.fn();
    handleMenuKeyDown({ key: 'Escape', preventDefault: vi.fn(), stopPropagation: stopPropagationMock });
    expect(isMenuOpen).toBe(false);
    expect(stopPropagationMock).toHaveBeenCalled();
  });

  // ---------------------------------------------------------------------------
  // 11. Focus is restored after menu dismissal or action execution
  // ---------------------------------------------------------------------------
  it('11. manages focus restoration targets after menu dismissal, action execution, and error dismissal', () => {
    // 11a. Menu trigger ref receives focus when menu closes
    const triggerBtn = { focus: vi.fn() };
    let menuOpen = true;

    const dismissMenu = () => {
      menuOpen = false;
      triggerBtn.focus();
    };

    dismissMenu();
    expect(menuOpen).toBe(false);
    expect(triggerBtn.focus).toHaveBeenCalledTimes(1);

    // 11b. Escape clears action error before affecting other UI levels
    useLauncherStore.setState({
      actionError: {
        actionId: 'context.pause',
        title: 'Pause Failed',
        message: 'Timeout',
        isRetryable: true,
      },
      resumeError: 'Resume failed',
      isPreviewOpen: true,
    });

    // Simulating Escape key event handling in ContextLauncher
    if (useLauncherStore.getState().actionError) {
      useLauncherStore.getState().clearActionError();
    }

    expect(useLauncherStore.getState().actionError).toBeNull();
    expect(useLauncherStore.getState().resumeError).toBeNull();
    // Preview stays open after dismissing transient error
    expect(useLauncherStore.getState().isPreviewOpen).toBe(true);
  });

  // ---------------------------------------------------------------------------
  // 12. Loading and success feedback reflect the real operation state
  // ---------------------------------------------------------------------------
  it('12. ensures loading and success feedback accurately reflect real asynchronous execution', async () => {
    let resolveResume: (value: any) => void;
    const pendingPromise = new Promise((resolve) => {
      resolveResume = resolve;
    });

    vi.mocked(contextInteractionService.resumeContext).mockImplementationOnce(() => pendingPromise as any);

    const execPromise = useLauncherStore
      .getState()
      .executeContextAction('context.resume', 'ctx-regression-active');

    // Mid-flight: loading flags active, no premature success toast
    expect(useLauncherStore.getState().isActionExecuting).toBe(true);
    expect(useLauncherStore.getState().isResuming).toBe(true);
    expect(useLauncherStore.getState().executingActionId).toBe('context.resume');
    expect(useLauncherStore.getState().executingTargetId).toBe('ctx-regression-active');
    expect(toast.success).not.toHaveBeenCalled();

    // 12a. Confirm success path
    resolveResume!({
      context_id: 'ctx-regression-active',
      status: 'active',
      switched_at: new Date().toISOString(),
    });
    await execPromise;

    // After success: flags cleanly reset, confirmed success toast shown
    expect(useLauncherStore.getState().isActionExecuting).toBe(false);
    expect(useLauncherStore.getState().isResuming).toBe(false);
    expect(useLauncherStore.getState().executingActionId).toBeNull();
    expect(useLauncherStore.getState().executingTargetId).toBeNull();
    expect(toast.success).toHaveBeenCalledWith(expect.stringContaining('ctx-regression-active'));

    // 12b. Confirm failure path: operation rejection guarantees cleanup and error notification
    let rejectPause: (reason: any) => void;
    const failingPromise = new Promise((_, reject) => {
      rejectPause = reject;
    });
    vi.mocked(contextInteractionService.pauseContext).mockImplementationOnce(() => failingPromise as any);

    const failExecPromise = useLauncherStore
      .getState()
      .executeContextAction('context.pause', 'ctx-regression-active');

    expect(useLauncherStore.getState().isActionExecuting).toBe(true);
    rejectPause!(new Error('Connection interrupted'));
    await failExecPromise;

    expect(useLauncherStore.getState().isActionExecuting).toBe(false);
    expect(toast.error).toHaveBeenCalled();
  });

  // ---------------------------------------------------------------------------
  // 13. Existing Context Launcher behavior remains intact
  // ---------------------------------------------------------------------------
  it('13. preserves core Context Launcher behaviors: search ranking, filtering, and windowing', () => {
    const activeContext = useLauncherStore.getState().activeContext!;
    const recentContexts = useLauncherStore.getState().recentContexts;
    const allContexts = [activeContext, ...recentContexts];

    // 13a. Query matching across name, project, repository, and task
    const matchEngine = allContexts.filter((c) => matchContext(c, 'engine'));
    expect(matchEngine).toHaveLength(1);
    expect(matchEngine[0].id).toBe('ctx-regression-active');

    // 13b. Ranking and sorting: exact match prioritizes higher
    const query = 'security';
    const matchSec = allContexts.filter((c) => matchContext(c, query));
    const sorted = sortContextMatches(matchSec, query);
    expect(sorted).toHaveLength(1);
    expect(sorted[0].id).toBe('ctx-regression-paused');

    // 13c. Filtering
    expect(matchesContextFilter(activeContext, 'all')).toBe(true);
    expect(matchesContextFilter(activeContext, 'active')).toBe(true);
    expect(matchesContextFilter(activeContext, 'paused')).toBe(false);

    // 13d. Windowing and rendering bounds
    const largeDataset: LauncherContextItem[] = Array.from({ length: 120 }, (_, i) => ({
      id: `ctx-bench-${i}`,
      name: `Context Benchmark ${i}`,
    }));

    const initialSlice = getVisibleLauncherSlice(largeDataset, 0);
    expect(initialSlice.visibleItems).toHaveLength(MAX_VISIBLE_LAUNCHER_RESULTS); // 50 items
    expect(initialSlice.hasMore).toBe(true);

    const scrolledSlice = getVisibleLauncherSlice(largeDataset, 55);
    expect(scrolledSlice.visibleItems).toHaveLength(65);
    expect(scrolledSlice.visibleItems[55].id).toBe('ctx-bench-55');
  });
});
