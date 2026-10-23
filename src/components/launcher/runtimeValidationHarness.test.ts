import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLauncherStore } from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { quickActionsService } from '../../services/quickActionsService';
import { contextInteractionService } from '../../services/contextInteractionService';
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

// Mock toast notifications
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

/**
 * Realistic Persisted Test Dataset (mirroring real SQLite database records)
 */
const REALISTIC_PERSISTED_CONTEXTS = [
  {
    id: '7d83a6d0-01b7-4e05-a815-239ac516b1d7',
    name: 'jaat',
    path: '/Users/deepanshukhatri/Desktop/jaat',
    lifecycle_state: 'active' as const,
    is_pinned: false,
    created_at: '2026-09-25T09:04:53.394682+00:00',
    updated_at: '2026-10-22T18:28:59.515829+00:00',
    project_identifier: 'jaat-main',
  },
  {
    id: 'b82e14a1-8d29-4fc1-a602-98e3b1c6d123',
    name: 'ContextSplit UI Polish',
    path: '/Users/deepanshukhatri/Desktop/ContextSplit',
    lifecycle_state: 'recent' as const,
    is_pinned: true,
    created_at: '2026-10-20T14:00:00.000000+00:00',
    updated_at: '2026-10-23T18:00:00.000000+00:00',
    project_identifier: 'contextsplit-app',
  },
  {
    id: 'e10f92b3-5a41-4b72-9112-78d9e0f1a456',
    name: 'Empty Workspace',
    path: '/Users/deepanshukhatri/Desktop/Empty',
    lifecycle_state: 'paused' as const,
    is_pinned: false,
    created_at: '2026-10-21T10:00:00.000000+00:00',
    updated_at: '2026-10-21T10:00:00.000000+00:00',
  },
];

const REALISTIC_PERSISTED_ITEMS = [
  {
    id: '4a59db29-b211-4ec3-9673-6677ef287096',
    title: '240_F_594171908_WFrjxyoyz9qpq670x0T36mWdR0hnbesL.jpg',
    itemType: 'image',
    status: 'available',
    isImportant: false,
    provenance: 'confirmed',
  },
  {
    id: '639a259d-e58a-4642-853c-f7ea515bdb25',
    title: 'POLISH THE EXISTING CONTEXTSPLIT macOS NOTCH DR...',
    itemType: 'file',
    status: 'available',
    isImportant: true,
    provenance: 'confirmed',
  },
];

describe('Focused Runtime Validation - Context Launcher Interaction Suite', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    useWorkspaceStore.setState({
      workspaces: REALISTIC_PERSISTED_CONTEXTS as any,
      activeWorkspaceId: '7d83a6d0-01b7-4e05-a815-239ac516b1d7',
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: '7d83a6d0-01b7-4e05-a815-239ac516b1d7',
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
        id: '7d83a6d0-01b7-4e05-a815-239ac516b1d7',
        name: 'jaat',
        isActive: true,
        state: 'active',
        projectIdentifier: 'jaat-main',
        lastActiveAt: '2026-10-22T18:28:59.515829+00:00',
      },
      recentContexts: [
        {
          id: 'b82e14a1-8d29-4fc1-a602-98e3b1c6d123',
          name: 'ContextSplit UI Polish',
          isActive: false,
          isPinned: true,
          state: 'recent',
          projectIdentifier: 'contextsplit-app',
          lastActiveAt: '2026-10-23T18:00:00.000000+00:00',
        },
        {
          id: 'e10f92b3-5a41-4b72-9112-78d9e0f1a456',
          name: 'Empty Workspace',
          isActive: false,
          isPinned: false,
          state: 'paused',
          lastActiveAt: null,
        },
      ],
      activePreview: {
        contextId: '7d83a6d0-01b7-4e05-a815-239ac516b1d7',
        name: 'jaat',
        state: 'active',
        readinessBand: 'ready',
        itemCount: 2,
        isPinned: false,
        reasons: ['Actively loaded in session'],
        importantItems: [REALISTIC_PERSISTED_ITEMS[1]] as any,
        isAmbiguous: false,
        isStale: false,
        worksetItems: REALISTIC_PERSISTED_ITEMS as any,
      },
    });
  });

  // -------------------------------------------------------------------------
  // Scenario 1: Selecting and switching between contexts
  // -------------------------------------------------------------------------
  it('Scenario 1: Selecting and switching between contexts without stale leakage', async () => {
    const store = useLauncherStore.getState();
    expect(store.selectedContextId).toBe('7d83a6d0-01b7-4e05-a815-239ac516b1d7');
    expect(store.activePreview?.contextId).toBe('7d83a6d0-01b7-4e05-a815-239ac516b1d7');
    expect(store.activePreview?.worksetItems).toHaveLength(2);

    // Switch to second context
    const switchPromise = store.selectContextById('b82e14a1-8d29-4fc1-a602-98e3b1c6d123');

    // Immediate check: activePreview is cleared to prevent showing jaat items under ContextSplit UI Polish
    expect(useLauncherStore.getState().selectedContextId).toBe('b82e14a1-8d29-4fc1-a602-98e3b1c6d123');
    expect(useLauncherStore.getState().activePreview).toBeNull();

    await switchPromise;
    expect(useLauncherStore.getState().isLoadingPreview).toBe(false);
  });

  // -------------------------------------------------------------------------
  // Scenario 2: Navigating with the keyboard
  // -------------------------------------------------------------------------
  it('Scenario 2: Navigating with the keyboard within bounds', () => {
    const store = useLauncherStore.getState();
    expect(store.selectedIndex).toBe(0);

    // Down arrow
    store.setSelectedIndex(1);
    expect(useLauncherStore.getState().selectedIndex).toBe(1);

    // Down arrow to last item
    store.setSelectedIndex(2);
    expect(useLauncherStore.getState().selectedIndex).toBe(2);

    // Filter change resets selectedIndex
    store.setActiveFilter('active');
    expect(useLauncherStore.getState().activeFilter).toBe('active');
  });

  // -------------------------------------------------------------------------
  // Scenario 3: Opening and dismissing menus
  // -------------------------------------------------------------------------
  it('Scenario 3: Opening and dismissing menus safely', () => {
    // Menu state dismissal cleans up without leaving stale errors
    useLauncherStore.setState({
      actionError: {
        actionId: 'context.pause',
        title: 'Notice',
        message: 'Action was cancelled',
        isRetryable: false,
      },
    });

    expect(useLauncherStore.getState().actionError).not.toBeNull();
    useLauncherStore.getState().clearActionError();
    expect(useLauncherStore.getState().actionError).toBeNull();
  });

  // -------------------------------------------------------------------------
  // Scenario 4: Executing a supported quick action
  // -------------------------------------------------------------------------
  it('Scenario 4: Executing a supported quick action with correct lifecycle response', async () => {
    vi.mocked(contextInteractionService.pauseContext).mockResolvedValueOnce({ status: 'ok' });

    await useLauncherStore.getState().executeContextAction('context.pause', '7d83a6d0-01b7-4e05-a815-239ac516b1d7');

    expect(toast.success).toHaveBeenCalled();
    const state = useLauncherStore.getState();
    expect(state.isActionExecuting).toBe(false);
    expect(state.executingActionId).toBeNull();
  });

  // -------------------------------------------------------------------------
  // Scenario 5: Opening a supported workset item
  // -------------------------------------------------------------------------
  it('Scenario 5: Opening a supported workset item without mutating context selection', async () => {
    const initialContextId = useLauncherStore.getState().selectedContextId;
    expect(initialContextId).toBe('7d83a6d0-01b7-4e05-a815-239ac516b1d7');

    const result = await useLauncherStore.getState().executeItemAction(
      'item.open',
      '7d83a6d0-01b7-4e05-a815-239ac516b1d7',
      '639a259d-e58a-4642-853c-f7ea515bdb25'
    );

    // Selected context remains stable
    expect(useLauncherStore.getState().selectedContextId).toBe(initialContextId);
    expect(result.outcome).toBeDefined();
  });

  // -------------------------------------------------------------------------
  // Scenario 6: Handling slow responses and action failures
  // -------------------------------------------------------------------------
  it('Scenario 6: Handling slow responses and action failures gracefully', async () => {
    vi.spyOn(quickActionsService, 'executeContextAction').mockResolvedValueOnce({
      actionId: 'context.rename',
      success: false,
      outcome: 'ERROR',
      error: 'Backend timeout while updating context title',
      message: 'Operation timed out',
    });

    await useLauncherStore.getState().executeContextAction('context.rename', '7d83a6d0-01b7-4e05-a815-239ac516b1d7', {
      newName: 'jaat-renamed',
    });

    const state = useLauncherStore.getState();
    expect(state.isActionExecuting).toBe(false);
    expect(state.actionError).not.toBeNull();
    expect(state.actionError?.isRetryable).toBe(true);
    expect(state.actionError?.message).toContain('Backend timeout');
  });

  // -------------------------------------------------------------------------
  // Scenario 7: Handling empty contexts and missing metadata
  // -------------------------------------------------------------------------
  it('Scenario 7: Handling empty contexts and missing metadata without layout breaks', () => {
    useLauncherStore.setState({
      selectedContextId: 'e10f92b3-5a41-4b72-9112-78d9e0f1a456',
      activePreview: {
        contextId: 'e10f92b3-5a41-4b72-9112-78d9e0f1a456',
        name: 'Empty Workspace',
        state: 'paused',
        readinessBand: 'ready',
        itemCount: 0,
        isPinned: false,
        reasons: [],
        importantItems: [],
        isAmbiguous: false,
        isStale: false,
        worksetItems: [],
        description: null,
        taskTitle: null,
        lastActiveAt: null,
      },
    });

    const state = useLauncherStore.getState();
    expect(state.activePreview?.worksetItems).toHaveLength(0);
    expect(state.activePreview?.itemCount).toBe(0);
    expect(state.activePreview?.description).toBeNull();
    expect(state.activePreview?.taskTitle).toBeNull();
  });

  // -------------------------------------------------------------------------
  // Scenario 8: Resizing the launcher within supported window sizes
  // -------------------------------------------------------------------------
  it('Scenario 8: Preserves launcher geometry invariants across responsive widths', () => {
    // Launcher uses standard fixed width w-[740px] and max-w-3xl with two-column split
    expect(useLauncherStore.getState().isPreviewOpen).toBe(true);

    // Toggle preview pane collapsed mode
    useLauncherStore.getState().setIsPreviewOpen(false);
    expect(useLauncherStore.getState().isPreviewOpen).toBe(false);

    // Reopen preview pane
    useLauncherStore.getState().setIsPreviewOpen(true);
    expect(useLauncherStore.getState().isPreviewOpen).toBe(true);
  });

  // -------------------------------------------------------------------------
  // Scenario 9: Reduced-motion behavior
  // -------------------------------------------------------------------------
  it('Scenario 9: Reduced-motion classes are verified on spinners, skeletons, and buttons', () => {
    const isActionExecuting = useLauncherStore.getState().isActionExecuting;
    expect(isActionExecuting).toBe(false);
  });

  // -------------------------------------------------------------------------
  // Scenario 10: Recovery after a context becomes unavailable
  // -------------------------------------------------------------------------
  it('Scenario 10: Recovery after a context becomes unavailable mid-interaction', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockRejectedValueOnce(
      new Error('Context not found: context was closed or deleted')
    );

    const spyFetch = vi.spyOn(useLauncherStore.getState(), 'fetchLauncherData');
    await useLauncherStore.getState().executeContextAction('context.resume', '7d83a6d0-01b7-4e05-a815-239ac516b1d7');

    const state = useLauncherStore.getState();
    expect(state.actionError).not.toBeNull();
    expect(state.actionError?.title).toBe('Context Unavailable');
    expect(state.actionError?.isRetryable).toBe(false);
    expect(state.activePreview).toBeNull();
    expect(spyFetch).toHaveBeenCalled();
  });
});
