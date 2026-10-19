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
});
