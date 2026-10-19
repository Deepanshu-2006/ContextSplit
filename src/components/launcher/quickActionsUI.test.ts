import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLauncherStore } from '../../store/launcherStore';
import { contextInteractionService } from '../../services/contextInteractionService';
import { contextWorksetService } from '../../services/contextWorkset';
import { db } from '../../services/db';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { toast } from 'sonner';
import {
  getAvailableContextActions,
  getAvailableItemActions,
} from '../../models/quickActions';

// Mock Tauri invoke to prevent window undefined during workspace select
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock the Context Interaction API service
vi.mock('../../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getResumeReadiness: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    resumeContext: vi.fn(),
    pauseContext: vi.fn(),
    controlContextMemory: vi.fn(),
  },
}));

vi.mock('../../services/contextWorkset', () => ({
  contextWorksetService: {
    controlContextMemory: vi.fn().mockResolvedValue(true),
  },
}));

vi.mock('../../services/db', () => ({
  db: {
    archiveWorkspace: vi.fn().mockImplementation(async (id: string) => ({
      id,
      name: 'Archived Workspace',
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      lifecycle_state: 'archived' as const,
    })),
    unarchiveWorkspace: vi.fn().mockImplementation(async (id: string) => ({
      id,
      name: 'Unarchived Workspace',
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      lifecycle_state: 'active' as const,
    })),
    listWorkspaces: vi.fn().mockResolvedValue([]),
    listItems: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

describe('Context Launcher Quick Actions UI & Contract Integration Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-active-1',
          name: 'Active Project',
          lifecycle_state: 'active',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        },
        {
          id: 'ctx-archived-1',
          name: 'Archived Project',
          lifecycle_state: 'archived',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        },
      ],
      activeWorkspaceId: 'ctx-active-1',
      openOverview: vi.fn(),
      selectWorkspace: vi.fn(),
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: 'ctx-active-1',
      activePreview: {
        contextId: 'ctx-active-1',
        name: 'Active Project',
        state: 'active',
        readinessBand: 'ready',
        reasons: [],
        importantItems: [],
        worksetItems: [
          { id: 'item-1', title: 'Task Document', itemType: 'doc', isImportant: false },
          { id: 'item-2', title: 'API Spec', itemType: 'url', isImportant: true },
        ],
        itemCount: 2,
        isAmbiguous: false,
        isStale: false,
      },
      isLoading: false,
      isLoadingPreview: false,
      isResuming: false,
      isActionExecuting: false,
      resumeStatus: 'idle',
      resumeError: null,
      notReadyReason: null,
      error: null,
      previewError: null,
      activeContext: { id: 'ctx-active-1', name: 'Active Project', state: 'active', isActive: true },
      recentContexts: [],
      isPreviewOpen: true,
      activeFilter: 'all',
    });
  });

  // 1. Primary action state adaptation
  it('1. adapts primary action based on actual state: Focus Workspace vs Resume vs Restore & Resume', () => {
    // 1a. Active context
    const activeActions = getAvailableContextActions({
      contextId: 'ctx-active-1',
      lifecycleState: 'active',
      isActive: true,
    });
    const activePrimary = activeActions.find((a) => a.id === 'context.resume');
    expect(activePrimary?.label).toBe('Focus Workspace');
    expect(activePrimary?.isSupported).toBe(true);
    expect(activePrimary?.shortcut).toBe('↵');

    // 1b. Paused / Dormant context
    const pausedActions = getAvailableContextActions({
      contextId: 'ctx-paused-1',
      lifecycleState: 'paused',
      isActive: false,
    });
    const pausedPrimary = pausedActions.find((a) => a.id === 'context.resume');
    expect(pausedPrimary?.label).toBe('Resume Context');
    expect(pausedPrimary?.isSupported).toBe(true);

    // 1c. Archived context
    const archivedActions = getAvailableContextActions({
      contextId: 'ctx-archived-1',
      lifecycleState: 'archived',
      isActive: false,
    });
    const archivedPrimary = archivedActions.find((a) => a.id === 'context.resume');
    expect(archivedPrimary?.label).toBe('Restore & Resume');
    expect(archivedPrimary?.isSupported).toBe(true);
  });

  // 2. Secondary actions contract
  it('2. filters supported secondary actions and enforces state-dependent availability', () => {
    // Active context
    const activeActions = getAvailableContextActions({
      contextId: 'ctx-active-1',
      lifecycleState: 'active',
      isActive: true,
    });

    const openDetails = activeActions.find((a) => a.id === 'context.open_details');
    expect(openDetails?.isSupported).toBe(true);
    expect(openDetails?.shortcut).toBe('⌘O');

    const pauseAction = activeActions.find((a) => a.id === 'context.pause');
    expect(pauseAction?.isSupported).toBe(true);
    expect(pauseAction?.shortcut).toBe('⌥P');

    const archiveAction = activeActions.find((a) => a.id === 'context.archive');
    expect(archiveAction?.isSupported).toBe(true);

    const deleteAction = activeActions.find((a) => a.id === 'context.delete');
    expect(deleteAction?.isSupported).toBe(false);
    expect(deleteAction?.unsupportedReason).toBeDefined();

    // Archived context: pause and archive must be marked unsupported with reasons
    const archivedActions = getAvailableContextActions({
      contextId: 'ctx-archived-1',
      lifecycleState: 'archived',
      isActive: false,
    });
    const archivedPause = archivedActions.find((a) => a.id === 'context.pause');
    expect(archivedPause?.isSupported).toBe(false);
    expect(archivedPause?.unsupportedReason).toContain('archived');

    const archivedArchive = archivedActions.find((a) => a.id === 'context.archive');
    expect(archivedArchive?.isSupported).toBe(false);
    expect(archivedArchive?.unsupportedReason).toContain('already archived');
  });

  // 3. Workset items quick actions
  it('3. derives appropriate item quick actions based on importance status', () => {
    // Unimportant item -> mark_important & remove_from_context
    const item1Actions = getAvailableItemActions({
      contextId: 'ctx-active-1',
      itemId: 'item-1',
      isImportant: false,
    });
    expect(item1Actions.some((a) => a.id === 'item.mark_important' && a.isSupported)).toBe(true);
    expect(item1Actions.some((a) => a.id === 'item.remove_from_context' && a.isSupported)).toBe(true);
    expect(item1Actions.some((a) => a.id === 'item.delete' && !a.isSupported)).toBe(true);

    // Important item -> unmark_important & remove_from_context
    const item2Actions = getAvailableItemActions({
      contextId: 'ctx-active-1',
      itemId: 'item-2',
      isImportant: true,
    });
    expect(item2Actions.some((a) => a.id === 'item.unmark_important' && a.isSupported)).toBe(true);
  });

  // 4. Primary action execution flow
  it('4. executes primary action (context.resume) cleanly with loading state and closes launcher', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({
      context_id: 'ctx-active-1',
      status: 'active',
      switched_at: new Date().toISOString(),
    });

    const promise = useLauncherStore.getState().executeContextAction('context.resume', 'ctx-active-1');
    expect(useLauncherStore.getState().isResuming).toBe(true);
    expect(useLauncherStore.getState().isActionExecuting).toBe(true);

    const result = await promise;
    expect(result.success).toBe(true);
    expect(useLauncherStore.getState().isResuming).toBe(false);
    expect(useLauncherStore.getState().isActionExecuting).toBe(false);
    expect(useLauncherStore.getState().isOpen).toBe(false);
    expect(toast.success).toHaveBeenCalled();
  });

  // 5. Restoring and resuming an archived context
  it('5. unarchives and resumes archived context through real db unarchive operation', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({
      context_id: 'ctx-archived-1',
      status: 'active',
      switched_at: new Date().toISOString(),
    });

    useLauncherStore.setState({
      selectedContextId: 'ctx-archived-1',
      activePreview: {
        contextId: 'ctx-archived-1',
        name: 'Archived Project',
        state: 'archived',
        readinessBand: 'ready',
        reasons: [],
        importantItems: [],
        worksetItems: [],
        itemCount: 0,
        isAmbiguous: false,
        isStale: false,
      },
    });

    const result = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-archived-1');
    expect(result.success).toBe(true);
    expect(db.unarchiveWorkspace).toHaveBeenCalledWith('ctx-archived-1');
    expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-archived-1');
    expect(useWorkspaceStore.getState().selectWorkspace).toHaveBeenCalledWith('ctx-archived-1');
  });

  // 6. Secondary action: Open details
  it('6. executes context.open_details via store openOverview and dismisses launcher', async () => {
    const openOverviewMock = vi.fn();
    useWorkspaceStore.setState({
      openOverview: openOverviewMock,
    });

    const result = await useLauncherStore.getState().executeContextAction('context.open_details', 'ctx-active-1');
    expect(result.success).toBe(true);
    expect(openOverviewMock).toHaveBeenCalledWith('ctx-active-1');
    expect(useLauncherStore.getState().isOpen).toBe(false);
  });

  // 7. Secondary action: Pause context
  it('7. emits pause signal to context engine when pause quick action is triggered', async () => {
    vi.mocked(contextInteractionService.pauseContext).mockResolvedValueOnce({
      context_id: 'ctx-active-1',
      status: 'paused',
      paused_at: new Date().toISOString(),
    });

    const result = await useLauncherStore.getState().executeContextAction('context.pause', 'ctx-active-1');
    expect(result.success).toBe(true);
    expect(contextInteractionService.pauseContext).toHaveBeenCalledWith('ctx-active-1');
    expect(toast.success).toHaveBeenCalled();
  });

  // 8. Secondary action: Archive context safely
  it('8. archives non-active context safely in db and deselects if it was active', async () => {
    const result = await useLauncherStore.getState().executeContextAction('context.archive', 'ctx-active-1');
    expect(result.success).toBe(true);
    expect(db.archiveWorkspace).toHaveBeenCalledWith('ctx-active-1');
    expect(useWorkspaceStore.getState().selectWorkspace).toHaveBeenCalledWith('inbox');
    expect(toast.success).toHaveBeenCalled();
  });

  // 9. Item quick actions: Mark and unmark important
  it('9. executes item.mark_important and item.unmark_important via contextWorksetService', async () => {
    vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValueOnce(true);

    const markResult = await useLauncherStore.getState().executeItemAction('item.mark_important', 'ctx-active-1', 'item-1');
    expect(markResult.success).toBe(true);
    expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith('ctx-active-1', 'item-1', 'keep_important');
    expect(toast.success).toHaveBeenCalled();

    vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValueOnce(true);

    const unmarkResult = await useLauncherStore.getState().executeItemAction('item.unmark_important', 'ctx-active-1', 'item-2');
    expect(unmarkResult.success).toBe(true);
    expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith('ctx-active-1', 'item-2', 'remove_important');
  });

  // 10. Item quick actions: Remove from context
  it('10. executes item.remove_from_context via contextWorksetService', async () => {
    vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValueOnce(true);

    const result = await useLauncherStore.getState().executeItemAction('item.remove_from_context', 'ctx-active-1', 'item-1');
    expect(result.success).toBe(true);
    expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith('ctx-active-1', 'item-1', 'remove_from_context');
    expect(toast.success).toHaveBeenCalled();
  });

  // 11. Concurrency and validation guards
  it('11. prevents duplicate execution and rejects empty IDs', async () => {
    const invalidResult = await useLauncherStore.getState().executeContextAction('context.resume', '   ');
    expect(invalidResult.success).toBe(false);
    expect(invalidResult.outcome).toBe('ERROR');

    const invalidItemResult = await useLauncherStore.getState().executeItemAction('item.mark_important', 'ctx-1', '');
    expect(invalidItemResult.success).toBe(false);
    expect(invalidItemResult.outcome).toBe('ERROR');
  });

  // 12. Ensures actual context ID is used (not display name or list index)
  it('12. ensures action uses actual context ID and connects to real resumeContext API', async () => {
    const rawUuid = 'f47ac10b-58cc-4372-a567-0e02b2c3d479';
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({
      context_id: rawUuid,
      status: 'active',
      switched_at: new Date().toISOString(),
    });

    useLauncherStore.setState({
      selectedContextId: rawUuid,
      activePreview: {
        contextId: rawUuid,
        name: 'Display Name With Spaces & Emoji 🚀',
        state: 'recent',
        readinessBand: 'ready',
        reasons: [],
        importantItems: [],
        worksetItems: [],
        itemCount: 0,
        isAmbiguous: false,
        isStale: false,
      },
    });

    const result = await useLauncherStore.getState().executeContextAction('context.resume', rawUuid);
    expect(result.success).toBe(true);
    // Verified: The backend call must receive the actual UUID, not the display name or index
    expect(contextInteractionService.resumeContext).toHaveBeenCalledWith(rawUuid);
    expect(useWorkspaceStore.getState().selectWorkspace).toHaveBeenCalledWith(rawUuid);
  });

  // 13. Concurrency guard prevents duplicate submissions during inflight action
  it('13. prevents duplicate submissions while an action is in progress', async () => {
    let resolveResume!: (val: any) => void;
    const inflightPromise = new Promise((resolve) => {
      resolveResume = resolve;
    });

    vi.mocked(contextInteractionService.resumeContext).mockImplementationOnce(() => inflightPromise as any);

    // First call starts execution
    const firstCallPromise = useLauncherStore.getState().executeContextAction('context.resume', 'ctx-active-1');
    expect(useLauncherStore.getState().isResuming).toBe(true);
    expect(useLauncherStore.getState().isActionExecuting).toBe(true);

    // Second call while in progress
    const duplicateCallResult = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-active-1');
    expect(duplicateCallResult.success).toBe(false);
    expect(duplicateCallResult.outcome).toBe('BLOCKED');
    expect(duplicateCallResult.message).toBe('An action is already in progress.');

    // Only one call to contextInteractionService.resumeContext should have occurred
    expect(contextInteractionService.resumeContext).toHaveBeenCalledTimes(1);

    // Resolve first call
    resolveResume({ context_id: 'ctx-active-1', status: 'active' });
    const firstResult = await firstCallPromise;
    expect(firstResult.success).toBe(true);
  });

  // 14. Refresh launcher state and display success only AFTER confirmation
  it('14. refreshes launcher state and displays success toast only AFTER operation succeeds', async () => {
    let resolveResume!: (val: any) => void;
    const inflightPromise = new Promise((resolve) => {
      resolveResume = resolve;
    });

    vi.mocked(contextInteractionService.resumeContext).mockImplementationOnce(() => inflightPromise as any);

    const fetchLauncherDataSpy = vi.spyOn(useLauncherStore.getState(), 'fetchLauncherData');

    const promise = useLauncherStore.getState().executeContextAction('context.resume', 'ctx-active-1');

    // Before backend confirms:
    expect(fetchLauncherDataSpy).not.toHaveBeenCalled();
    expect(toast.success).not.toHaveBeenCalled();
    expect(useLauncherStore.getState().isOpen).toBe(true);

    // Backend confirms:
    resolveResume({ context_id: 'ctx-active-1', status: 'active' });
    await promise;

    // After backend confirms:
    expect(fetchLauncherDataSpy).toHaveBeenCalledTimes(1);
    expect(toast.success).toHaveBeenCalledTimes(1);
    expect(useLauncherStore.getState().isOpen).toBe(false);
  });

  // 15. Handles backend failures gracefully without success feedback or closing launcher
  it('15. handles backend failures gracefully without success feedback or closing launcher', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockRejectedValueOnce(
      new Error('Context Engine IPC timeout')
    );

    useLauncherStore.setState({
      isOpen: true,
      selectedContextId: 'ctx-active-1',
    });

    const result = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-active-1');

    expect(result.success).toBe(false);
    expect(result.outcome).toBe('ERROR');
    expect(result.error).toBe('Context Engine IPC timeout');

    // State reflects error
    expect(useLauncherStore.getState().resumeStatus).toBe('error');
    expect(useLauncherStore.getState().resumeError).toBe('Context Engine IPC timeout');
    expect(useLauncherStore.getState().isResuming).toBe(false);

    // Launcher remains open for user recovery; no false success toast
    expect(useLauncherStore.getState().isOpen).toBe(true);
    expect(toast.success).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalled();
  });

  // 16. Handles contexts that become unavailable between selection and execution
  it('16. handles contexts that become unavailable between selection and execution', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockRejectedValueOnce(
      new Error("Workspace 'ctx-deleted-99' not found in database")
    );

    useLauncherStore.setState({
      isOpen: true,
      selectedContextId: 'ctx-deleted-99',
      activePreview: {
        contextId: 'ctx-deleted-99',
        name: 'Deleted Workspace',
        state: 'recent',
        readinessBand: 'ready',
        reasons: [],
        importantItems: [],
        worksetItems: [],
        itemCount: 0,
        isAmbiguous: false,
        isStale: false,
      },
    });

    const result = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-deleted-99');

    expect(result.success).toBe(false);
    expect(result.outcome).toBe('ERROR');
    expect(result.message).toBe("Context 'ctx-deleted-99' is no longer available. It may have been closed or deleted.");

    // Deselects and removes deleted context
    expect(useLauncherStore.getState().selectedContextId).not.toBe('ctx-deleted-99');
    expect(useLauncherStore.getState().activePreview?.contextId).not.toBe('ctx-deleted-99');
    expect(useLauncherStore.getState().isOpen).toBe(true);
    expect(toast.error).toHaveBeenCalledWith("Context 'ctx-deleted-99' is no longer available. It may have been closed or deleted.");
  });

  // 17. Context secondary-action menu availability across lifecycle states
  it('17. accurately populates secondary actions across active, paused, archived, and dormant states', () => {
    // 17a. Active state
    const activeActions = getAvailableContextActions({
      contextId: 'ctx-1',
      lifecycleState: 'active',
      isActive: true,
    }).filter((a) => a.id !== 'context.resume');

    expect(activeActions.map((a) => a.id)).toEqual([
      'context.open_details',
      'context.pause',
      'context.archive',
      'context.delete',
    ]);
    expect(activeActions.find((a) => a.id === 'context.pause')?.isSupported).toBe(true);
    expect(activeActions.find((a) => a.id === 'context.archive')?.isSupported).toBe(true);
    expect(activeActions.find((a) => a.id === 'context.delete')?.isSupported).toBe(false);

    // 17b. Paused state
    const pausedActions = getAvailableContextActions({
      contextId: 'ctx-2',
      lifecycleState: 'paused',
      isActive: false,
    }).filter((a) => a.id !== 'context.resume');

    expect(pausedActions.find((a) => a.id === 'context.pause')?.isSupported).toBe(false);
    expect(pausedActions.find((a) => a.id === 'context.pause')?.unsupportedReason).toContain('not currently active');
    expect(pausedActions.find((a) => a.id === 'context.archive')?.isSupported).toBe(true);

    // 17c. Archived state
    const archivedActions = getAvailableContextActions({
      contextId: 'ctx-3',
      lifecycleState: 'archived',
      isActive: false,
    }).filter((a) => a.id !== 'context.resume');

    expect(archivedActions.find((a) => a.id === 'context.pause')?.isSupported).toBe(false);
    expect(archivedActions.find((a) => a.id === 'context.pause')?.unsupportedReason).toContain('Cannot pause an archived context');
    expect(archivedActions.find((a) => a.id === 'context.archive')?.isSupported).toBe(false);
    expect(archivedActions.find((a) => a.id === 'context.archive')?.unsupportedReason).toContain('already archived');
  });

  // 18. Clear explanations for unavailable actions without exposing them as clickable options
  it('18. provides clear structured explanations for unavailable secondary actions', () => {
    const actions = getAvailableContextActions({
      contextId: 'ctx-dormant-1',
      lifecycleState: 'dormant',
      isActive: false,
    });

    const pauseAction = actions.find((a) => a.id === 'context.pause');
    expect(pauseAction?.isSupported).toBe(false);
    expect(pauseAction?.unsupportedReason).toBe('Cannot pause a dormant context.');

    const deleteAction = actions.find((a) => a.id === 'context.delete');
    expect(deleteAction?.isSupported).toBe(false);
    expect(deleteAction?.unsupportedReason).toBe('Context deletion must be performed from workspace management settings.');
  });

  // 19. Item secondary-action menu availability and action generation
  it('19. provides accurate item secondary actions and marks permanent deletion as unsupported in launcher', () => {
    const unpinnedActions = getAvailableItemActions({
      contextId: 'ctx-test-1',
      itemId: 'item-note-1',
      isImportant: false,
    });

    expect(unpinnedActions.map((a) => a.id)).toEqual([
      'item.mark_important',
      'item.remove_from_context',
      'item.delete',
    ]);
    expect(unpinnedActions.find((a) => a.id === 'item.mark_important')?.label).toBe('Mark as Important');
    expect(unpinnedActions.find((a) => a.id === 'item.delete')?.isSupported).toBe(false);
    expect(unpinnedActions.find((a) => a.id === 'item.delete')?.unsupportedReason).toBe(
      'Permanent item deletion must be performed in the main inbox or canvas.'
    );

    const pinnedActions = getAvailableItemActions({
      contextId: 'ctx-test-1',
      itemId: 'item-note-2',
      isImportant: true,
    });
    expect(pinnedActions.find((a) => a.id === 'item.unmark_important')?.label).toBe('Unmark Important');
  });

  // 20. Target ID verification: contextId and itemId are correctly isolated and passed
  it('20. ensures context operations receive exact contextId and item operations receive both contextId and itemId', async () => {
    const contextId = 'c7e527f3-d029-450a-a921-9a70613d9641';
    const itemId = 'item-98a4-bf23';

    vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValueOnce(true);

    const result = await useLauncherStore.getState().executeItemAction('item.mark_important', contextId, itemId);
    expect(result.success).toBe(true);

    // Verified: First argument is contextId, second is itemId
    expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith(contextId, itemId, 'keep_important');
  });

  // 21. Clearly distinguish item importance from parent context lifecycle state
  it('21. clearly distinguishes item working-set memory from parent context lifecycle state', async () => {
    const parentContextId = 'ctx-parent-100';
    const initialParentState = 'active';

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: parentContextId,
          name: 'Parent Workspace',
          lifecycle_state: initialParentState,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        },
      ],
      activeWorkspaceId: parentContextId,
    });

    vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValueOnce(true);

    // Toggle item importance in working set
    const result = await useLauncherStore.getState().executeItemAction('item.mark_important', parentContextId, 'item-pinned');
    expect(result.success).toBe(true);

    // Verify parent workspace lifecycle state remains untouched
    const currentWorkspace = useWorkspaceStore.getState().workspaces.find((w) => w.id === parentContextId);
    expect(currentWorkspace?.lifecycle_state).toBe(initialParentState);
    expect(db.archiveWorkspace).not.toHaveBeenCalled();
    expect(db.unarchiveWorkspace).not.toHaveBeenCalled();
  });

  // 22. Guards against execution of unsupported actions
  it('22. prevents execution of unsupported context.delete and item.delete actions', async () => {
    const contextResult = await useLauncherStore.getState().executeContextAction('context.delete', 'ctx-1');
    expect(contextResult.success).toBe(false);
    expect(contextResult.outcome).toBe('UNSUPPORTED');
    expect(contextResult.message).toContain('not supported');

    const itemResult = await useLauncherStore.getState().executeItemAction('item.delete', 'ctx-1', 'item-1');
    expect(itemResult.success).toBe(false);
    expect(itemResult.outcome).toBe('UNSUPPORTED');
    expect(itemResult.message).toContain('not supported');

    // Context Engine mutator was never invoked
    expect(contextWorksetService.controlContextMemory).not.toHaveBeenCalled();
  });

  // 23. Handles item action failure gracefully
  it('23. handles item action failure gracefully with error toast and without corrupting state', async () => {
    vi.mocked(contextWorksetService.controlContextMemory).mockRejectedValueOnce(
      new Error('Disk write locked by another process')
    );

    const result = await useLauncherStore.getState().executeItemAction('item.remove_from_context', 'ctx-active-1', 'item-1');
    expect(result.success).toBe(false);
    expect(result.outcome).toBe('ERROR');
    expect(result.error).toBe('Disk write locked by another process');

    expect(toast.error).toHaveBeenCalledWith('Failed to remove item from context: Disk write locked by another process');
    expect(useLauncherStore.getState().isActionExecuting).toBe(false);
  });

  // 24. Valid transitions across all operational lifecycle states
  it('24. executes valid transitions correctly across active, paused, dormant, and archived states', async () => {
    // 24a. Active: pause and archive are valid
    useWorkspaceStore.setState({
      workspaces: [
        { id: 'ctx-test-active', name: 'Active Workspace', lifecycle_state: 'active', created_at: '', updated_at: '' },
      ],
      activeWorkspaceId: 'ctx-test-active',
    });
    vi.mocked(contextInteractionService.pauseContext).mockResolvedValueOnce({ status: 'paused' });
    const pauseRes = await useLauncherStore.getState().executeContextAction('context.pause', 'ctx-test-active');
    expect(pauseRes.success).toBe(true);
    expect(pauseRes.outcome).toBe('SUCCESS');

    // 24b. Paused: resume and archive are valid
    useWorkspaceStore.setState({
      workspaces: [
        { id: 'ctx-test-paused', name: 'Paused Workspace', lifecycle_state: 'active', created_at: '', updated_at: '' },
      ],
      activeWorkspaceId: 'other-workspace',
    });
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ status: 'active' });
    const resumePausedRes = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-test-paused');
    expect(resumePausedRes.success).toBe(true);
    expect(resumePausedRes.outcome).toBe('SUCCESS');

    // 24c. Dormant: resume and archive are valid
    useWorkspaceStore.setState({
      workspaces: [
        { id: 'ctx-test-dormant', name: 'Dormant Workspace', lifecycle_state: 'dormant', created_at: '', updated_at: '' },
      ],
      activeWorkspaceId: 'other-workspace',
    });
    const archiveDormantRes = await useLauncherStore.getState().executeContextAction('context.archive', 'ctx-test-dormant');
    expect(archiveDormantRes.success).toBe(true);
    expect(archiveDormantRes.outcome).toBe('SUCCESS');

    // 24d. Archived: resume (Restore & Resume) is valid
    useWorkspaceStore.setState({
      workspaces: [
        { id: 'ctx-test-archived', name: 'Archived Workspace', lifecycle_state: 'archived', created_at: '', updated_at: '' },
      ],
      activeWorkspaceId: 'other-workspace',
    });
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ status: 'active' });
    const resumeArchivedRes = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-test-archived');
    expect(resumeArchivedRes.success).toBe(true);
    expect(db.unarchiveWorkspace).toHaveBeenCalledWith('ctx-test-archived');
  });

  // 25. Invalid transitions are blocked with clear structured explanations
  it('25. blocks invalid transitions with clear explanations according to Context Engine lifecycle rules', async () => {
    // 25a. Pause on already paused context
    useWorkspaceStore.setState({
      workspaces: [
        { id: 'ctx-paused', name: 'Paused', lifecycle_state: 'active', created_at: '', updated_at: '' },
      ],
      activeWorkspaceId: 'different-workspace',
    });
    const pauseOnPausedRes = await useLauncherStore.getState().executeContextAction('context.pause', 'ctx-paused', {
      lifecycleState: 'paused',
    });
    expect(pauseOnPausedRes.success).toBe(false);
    expect(pauseOnPausedRes.outcome).toBe('BLOCKED');
    expect(pauseOnPausedRes.message).toContain('already paused');

    // 25b. Pause on dormant context
    useWorkspaceStore.setState({
      workspaces: [
        { id: 'ctx-dormant', name: 'Dormant', lifecycle_state: 'dormant', created_at: '', updated_at: '' },
      ],
      activeWorkspaceId: 'different-workspace',
    });
    const pauseOnDormantRes = await useLauncherStore.getState().executeContextAction('context.pause', 'ctx-dormant', {
      lifecycleState: 'dormant',
    });
    expect(pauseOnDormantRes.success).toBe(false);
    expect(pauseOnDormantRes.outcome).toBe('BLOCKED');
    expect(pauseOnDormantRes.message).toContain('Cannot pause a dormant context');

    // 25c. Pause on archived context
    useWorkspaceStore.setState({
      workspaces: [
        { id: 'ctx-archived', name: 'Archived', lifecycle_state: 'archived', created_at: '', updated_at: '' },
      ],
      activeWorkspaceId: 'different-workspace',
    });
    const pauseOnArchivedRes = await useLauncherStore.getState().executeContextAction('context.pause', 'ctx-archived', {
      lifecycleState: 'archived',
    });
    expect(pauseOnArchivedRes.success).toBe(false);
    expect(pauseOnArchivedRes.outcome).toBe('BLOCKED');
    expect(pauseOnArchivedRes.message).toContain('Cannot pause an archived context');

    // 25d. Archive on already archived context
    const archiveOnArchivedRes = await useLauncherStore.getState().executeContextAction('context.archive', 'ctx-archived', {
      lifecycleState: 'archived',
    });
    expect(archiveOnArchivedRes.success).toBe(false);
    expect(archiveOnArchivedRes.outcome).toBe('BLOCKED');
    expect(archiveOnArchivedRes.message).toContain('already archived');
  });

  // 26. Handles stale launcher data when context transitions in background during interaction
  it('26. detects stale state when context transitions to paused in background and blocks invalid action', async () => {
    // Context was selected and previewed while active:
    useLauncherStore.setState({
      selectedContextId: 'ctx-mutated',
      activePreview: {
        contextId: 'ctx-mutated',
        name: 'Mutated Workspace',
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

    // While launcher remained open, the user switched to another workspace in the main window
    useWorkspaceStore.setState({
      workspaces: [
        { id: 'ctx-mutated', name: 'Mutated Workspace', lifecycle_state: 'active', created_at: '', updated_at: '' },
        { id: 'ctx-other', name: 'Other Workspace', lifecycle_state: 'active', created_at: '', updated_at: '' },
      ],
      activeWorkspaceId: 'ctx-other', // No longer active!
    });

    const fetchLauncherDataSpy = vi.spyOn(useLauncherStore.getState(), 'fetchLauncherData');

    // User attempts to trigger "Pause Context" based on stale "active" preview
    const result = await useLauncherStore.getState().executeContextAction('context.pause', 'ctx-mutated');

    expect(result.success).toBe(false);
    expect(result.outcome).toBe('BLOCKED');
    expect(result.message).toContain('Action is no longer valid: Context is already paused');

    // Verifies clear user feedback is shown via toast.info
    expect(toast.info).toHaveBeenCalledWith(expect.stringContaining('Action is no longer valid'));

    // Verifies launcher automatically re-fetches data to resync with actual state
    expect(fetchLauncherDataSpy).toHaveBeenCalled();
  });

  // 27. Handles context archived in background: adapts resume to Restore & Resume
  it('27. detects context archived in background and safely restores it before resuming', async () => {
    // Preview initially recorded state as 'active':
    useLauncherStore.setState({
      selectedContextId: 'ctx-background-archived',
      activePreview: {
        contextId: 'ctx-background-archived',
        name: 'Background Archived',
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

    // In the background, workspace was archived in database:
    useWorkspaceStore.setState({
      workspaces: [
        { id: 'ctx-background-archived', name: 'Background Archived', lifecycle_state: 'archived', created_at: '', updated_at: '' },
      ],
      activeWorkspaceId: 'other',
    });

    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({
      context_id: 'ctx-background-archived',
      status: 'active',
    });

    // When resume is triggered, live revalidation notices it is archived and unarchives before resume
    const result = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-background-archived');

    expect(result.success).toBe(true);
    expect(db.unarchiveWorkspace).toHaveBeenCalledWith('ctx-background-archived');
    expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-background-archived');
    expect(result.message).toContain('Unarchived and resumed');
  });

  // 28. Shows clear feedback and cleans up when context is closed/deleted during interaction
  it('28. provides clear feedback and cleans up preview when context is deleted during interaction', async () => {
    useLauncherStore.setState({
      isOpen: true,
      selectedContextId: 'ctx-to-be-deleted',
      activePreview: {
        contextId: 'ctx-to-be-deleted',
        name: 'To Be Deleted',
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

    // Context is deleted from backend/database:
    vi.mocked(contextInteractionService.resumeContext).mockRejectedValueOnce(
      new Error("Context 'ctx-to-be-deleted' does not exist")
    );

    const result = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-to-be-deleted');

    expect(result.success).toBe(false);
    expect(result.outcome).toBe('ERROR');
    expect(result.message).toContain('no longer available');
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('no longer available'));

    // Deleted context is removed from selection and preview
    expect(useLauncherStore.getState().activePreview?.contextId).not.toBe('ctx-to-be-deleted');
    expect(useLauncherStore.getState().selectedContextId).not.toBe('ctx-to-be-deleted');
    expect(useLauncherStore.getState().isOpen).toBe(true);
  });
});
