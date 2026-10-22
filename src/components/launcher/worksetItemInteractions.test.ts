import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  useLauncherStore,
  invalidateLauncherPreviewCache,
} from '../../store/launcherStore';
import { contextInteractionService } from '../../services/contextInteractionService';
import { contextWorksetService } from '../../services/contextWorkset';
import { db } from '../../services/db';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { toast } from 'sonner';

// Mock Tauri core invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock Tauri plugin opener
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
  revealItemInDir: vi.fn().mockResolvedValue(undefined),
  openPath: vi.fn().mockResolvedValue(undefined),
}));

// Mock Context Interaction Service
vi.mock('../../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getResumeReadiness: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    resumeContext: vi.fn(),
  },
}));

// Mock Context Workset Service
vi.mock('../../services/contextWorkset', () => ({
  contextWorksetService: {
    controlContextMemory: vi.fn().mockResolvedValue(true),
  },
}));

// Mock Database Service
vi.mock('../../services/db', () => ({
  db: {
    getItem: vi.fn(),
    moveItem: vi.fn(),
    updateItem: vi.fn(),
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

describe('Context Workset Item Interactions Integration Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invalidateLauncherPreviewCache();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-active-project',
          name: 'Active Project',
          path: '/projects/active',
          lifecycle_state: 'active',
          created_at: '2026-10-01T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
        } as any,
      ],
      activeWorkspaceId: 'ctx-active-project',
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: 'ctx-active-project',
      activePreview: {
        contextId: 'ctx-active-project',
        name: 'Active Project',
        description: 'Test project description',
        state: 'active',
        readinessBand: 'ready',
        itemCount: 3,
        worksetItems: [
          {
            id: 'item-doc-url',
            contextId: 'ctx-active-project',
            title: 'Design Specification',
            itemType: 'url',
            relevanceTier: 'primary',
            provenance: 'confirmed',
            isSuggestedAddition: false,
            isImportant: false,
            status: 'available',
            domain: 'figma.com',
            snippet: 'https://figma.com/file/12345/design-spec',
          } as any,
          {
            id: 'item-local-file',
            contextId: 'ctx-active-project',
            title: 'Architecture Diagram.drawio',
            itemType: 'file',
            relevanceTier: 'supporting',
            provenance: 'confirmed',
            isSuggestedAddition: false,
            isImportant: true,
            status: 'available',
            snippet: '/projects/active/docs/Architecture.drawio',
          } as any,
          {
            id: 'item-inferred-pr',
            contextId: 'ctx-active-project',
            title: 'PR #42: Workset API',
            itemType: 'url',
            relevanceTier: 'suggested',
            provenance: 'inferred',
            isSuggestedAddition: true,
            isImportant: false,
            status: 'available',
            domain: 'github.com',
            snippet: 'https://github.com/org/repo/pull/42',
          } as any,
        ],
      } as any,
      isLoading: false,
      isLoadingPreview: false,
      isResuming: false,
      isActionExecuting: false,
      executingActionId: null,
      executingTargetId: null,
      actionError: null,
    });
  });

  // =========================================================================
  // 1. Successful Actions & Context Preservation
  // =========================================================================

  it('1. successfully opens a saved URL in default browser and preserves selected context', async () => {
    vi.mocked(db.getItem).mockResolvedValueOnce({
      id: 'item-doc-url',
      workspace_id: 'ctx-active-project',
      session_id: null,
      item_type: 'url',
      content: 'https://figma.com/file/12345/design-spec',
      title: 'Design Specification',
      metadata: '{}',
      created_at: '2026-10-22T08:00:00Z',
      updated_at: '2026-10-22T08:00:00Z',
    });

    const { openUrl } = await import('@tauri-apps/plugin-opener');

    const result = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-active-project', 'item-doc-url');

    expect(result.success).toBe(true);
    expect(result.outcome).toBe('SUCCESS');
    expect(openUrl).toHaveBeenCalledWith('https://figma.com/file/12345/design-spec');
    expect(toast.success).toHaveBeenCalledWith(expect.stringContaining('Opened link in browser'));

    // Requirement 2: Selected context is preserved
    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-active-project');
    expect(useLauncherStore.getState().activePreview?.contextId).toBe('ctx-active-project');

    // Requirement 6: Does not move item or mutate contextual membership
    expect(contextWorksetService.controlContextMemory).not.toHaveBeenCalled();
    expect(db.moveItem).not.toHaveBeenCalled();
  });

  it('2. successfully reveals a valid file in system explorer and preserves selected context', async () => {
    vi.mocked(db.getItem).mockResolvedValueOnce({
      id: 'item-local-file',
      workspace_id: 'ctx-active-project',
      session_id: null,
      item_type: 'file',
      content: '/projects/active/docs/Architecture.drawio',
      title: 'Architecture Diagram.drawio',
      metadata: '{}',
      created_at: '2026-10-22T08:00:00Z',
      updated_at: '2026-10-22T08:00:00Z',
    });

    const { revealItemInDir } = await import('@tauri-apps/plugin-opener');

    const result = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-active-project', 'item-local-file');

    expect(result.success).toBe(true);
    expect(result.outcome).toBe('SUCCESS');
    expect(revealItemInDir).toHaveBeenCalledWith('/projects/active/docs/Architecture.drawio');
    expect(toast.success).toHaveBeenCalledWith(expect.stringContaining('Revealed file in system explorer'));

    // Requirement 2: Selected context is preserved
    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-active-project');
  });

  // =========================================================================
  // 2. Unsupported Item Types (Requirement 5)
  // =========================================================================

  it('3. rejects unsupported item types (e.g. raw text notes) without inventing fake operations', async () => {
    vi.mocked(db.getItem).mockResolvedValueOnce({
      id: 'item-note-raw',
      workspace_id: 'ctx-active-project',
      session_id: null,
      item_type: 'text',
      content: 'Quick note taken in meeting without a URL or file path.',
      title: 'Meeting Notes',
      metadata: '{}',
      created_at: '2026-10-22T08:00:00Z',
      updated_at: '2026-10-22T08:00:00Z',
    });

    const { openUrl, revealItemInDir } = await import('@tauri-apps/plugin-opener');

    const result = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-active-project', 'item-note-raw');

    expect(result.success).toBe(false);
    expect(result.outcome).toBe('UNSUPPORTED');
    expect(result.message).toBe("Opening is not supported for item type 'text'.");
    expect(openUrl).not.toHaveBeenCalled();
    expect(revealItemInDir).not.toHaveBeenCalled();

    // Sets structured actionError in store
    const state = useLauncherStore.getState();
    expect(state.actionError?.title).toBe('Could Not Open Item');
    expect(state.actionError?.message).toBe("Opening is not supported for item type 'text'.");
    expect(state.actionError?.isRetryable).toBe(false);
  });

  // =========================================================================
  // 3. Unavailable & Missing Items (Requirement 3 & 4)
  // =========================================================================

  it('4. validates item availability before executing and handles unavailable records safely', async () => {
    vi.mocked(db.getItem).mockRejectedValueOnce(new Error('Record not found in database'));

    const result = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-active-project', 'item-missing-404');

    expect(result.success).toBe(false);
    expect(result.outcome).toBe('ERROR');
    expect(result.message).toContain('Item is unavailable or has been removed');
    expect(toast.error).toHaveBeenCalledWith('Item is unavailable or has been removed.');

    const state = useLauncherStore.getState();
    expect(state.actionError?.title).toBe('Could Not Open Item');
    expect(state.actionError?.targetId).toBe('item-missing-404');
    expect(state.actionError?.isRetryable).toBe(true);
  });

  it('5. blocks opening deleted items and returns appropriate feedback', async () => {
    vi.mocked(db.getItem).mockResolvedValueOnce({
      id: 'item-deleted-link',
      workspace_id: 'ctx-active-project',
      session_id: null,
      item_type: 'url',
      content: 'https://deleted.com',
      title: 'Deleted Link',
      metadata: JSON.stringify({ is_deleted: true }),
      created_at: '2026-10-22T08:00:00Z',
      updated_at: '2026-10-22T08:00:00Z',
    });

    const result = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-active-project', 'item-deleted-link');

    expect(result.success).toBe(false);
    expect(result.outcome).toBe('BLOCKED');
    expect(result.message).toContain('Item has been deleted and cannot be opened');

    const state = useLauncherStore.getState();
    expect(state.actionError?.isRetryable).toBe(false);
  });

  // =========================================================================
  // 4. Duplicate Execution Guard (Requirement 9)
  // =========================================================================

  it('6. prevents duplicate action execution while another action is in-flight', async () => {
    useLauncherStore.setState({
      isActionExecuting: true,
      executingActionId: 'item.open',
      executingTargetId: 'item-doc-url',
    });

    const result = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-active-project', 'item-doc-url');

    expect(result.outcome).toBe('BLOCKED');
    expect(result.message).toBe('An action is already in progress.');
    expect(db.getItem).not.toHaveBeenCalled();
  });

  // =========================================================================
  // 5. Inferred Relationship Invariance (Requirements 6 & 7)
  // =========================================================================

  it('7. opening an inferred item does not treat viewing as confirmation that its relationship is correct', async () => {
    vi.mocked(db.getItem).mockResolvedValueOnce({
      id: 'item-inferred-pr',
      workspace_id: 'inbox',
      session_id: null,
      item_type: 'url',
      content: 'https://github.com/org/repo/pull/42',
      title: 'PR #42: Workset API',
      metadata: '{}',
      created_at: '2026-10-22T08:00:00Z',
      updated_at: '2026-10-22T08:00:00Z',
    });

    const result = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-active-project', 'item-inferred-pr');

    expect(result.success).toBe(true);

    // Requirement 6: Zero workspace migration
    expect(db.moveItem).not.toHaveBeenCalled();

    // Requirement 7: Zero automatic context memory retention / confirmation
    expect(contextWorksetService.controlContextMemory).not.toHaveBeenCalled();

    // Inferred item in preview remains inferred
    const inferred = useLauncherStore
      .getState()
      .activePreview?.worksetItems.find((i) => i.id === 'item-inferred-pr');
    expect(inferred?.provenance).toBe('inferred');
    expect(inferred?.isSuggestedAddition).toBe(true);
  });

  // =========================================================================
  // 6. Failure Recovery (Requirement 12)
  // =========================================================================

  it('8. supports failure recovery by allowing retrying a previously failed item action', async () => {
    // 8a. First attempt fails due to temporary DB / network timeout
    vi.mocked(db.getItem).mockRejectedValueOnce(new Error('Connection timeout'));

    const failResult = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-active-project', 'item-doc-url');

    expect(failResult.success).toBe(false);
    expect(useLauncherStore.getState().actionError).not.null;
    expect(useLauncherStore.getState().actionError?.targetId).toBe('item-doc-url');

    // 8b. DB recovers and second attempt succeeds
    vi.mocked(db.getItem).mockResolvedValueOnce({
      id: 'item-doc-url',
      workspace_id: 'ctx-active-project',
      session_id: null,
      item_type: 'url',
      content: 'https://figma.com/file/12345/design-spec',
      title: 'Design Specification',
      metadata: '{}',
      created_at: '2026-10-22T08:00:00Z',
      updated_at: '2026-10-22T08:00:00Z',
    });

    const failedActionId = useLauncherStore.getState().actionError?.actionId as any;
    const failedTargetId = useLauncherStore.getState().actionError?.targetId!;

    const retryResult = await useLauncherStore
      .getState()
      .executeItemAction(failedActionId, 'ctx-active-project', failedTargetId);

    expect(retryResult.success).toBe(true);
    expect(retryResult.outcome).toBe('SUCCESS');
    expect(useLauncherStore.getState().actionError).toBeNull();
    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-active-project');
  });
});
