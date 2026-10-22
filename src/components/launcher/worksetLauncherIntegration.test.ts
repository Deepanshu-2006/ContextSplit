import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  useLauncherStore,
  invalidateLauncherPreviewCache,
  getLauncherPreviewCache,
  WORKSET_CACHE_TTL_MS,
} from '../../store/launcherStore';
import { contextInteractionService } from '../../services/contextInteractionService';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { quickActionsService } from '../../services/quickActionsService';

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
  },
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

describe('Context Workset Launcher Integration Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invalidateLauncherPreviewCache();
    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-alpha',
          name: 'Alpha Project',
          path: '/projects/alpha',
          lifecycle_state: 'active',
          created_at: '2026-10-01T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
        } as any,
        {
          id: 'ctx-beta',
          name: 'Beta Research',
          path: '/projects/beta',
          lifecycle_state: 'recent',
          created_at: '2026-10-05T10:00:00Z',
          updated_at: '2026-10-22T11:00:00Z',
        } as any,
      ],
      activeWorkspaceId: null,
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: null,
      activePreview: null,
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
      activeContext: null,
      recentContexts: [
        {
          id: 'ctx-alpha',
          name: 'Alpha Project',
          isActive: false,
          state: 'active',
        },
        {
          id: 'ctx-beta',
          name: 'Beta Research',
          isActive: false,
          state: 'recent',
        },
      ],
      isPreviewOpen: true,
    });
  });

  // =========================================================================
  // 1. Context Switching & Selected Context Source
  // =========================================================================

  it('1. loads workset for currently selected context and refreshes when selection changes', async () => {
    vi.mocked(contextInteractionService.getContextState).mockImplementation(async (id) => ({
      context_id: id,
      context_name: id === 'ctx-alpha' ? 'Alpha Project' : 'Beta Research',
      state: 'active',
      task_title: id === 'ctx-alpha' ? 'Implement Auth' : 'Analyze Dataset',
    } as any));

    vi.mocked(contextInteractionService.getContextWorkset).mockImplementation(async (id) => ({
      context_id: id,
      context_name: id === 'ctx-alpha' ? 'Alpha Project' : 'Beta Research',
      task_title: id === 'ctx-alpha' ? 'Implement Auth' : 'Analyze Dataset',
      items: id === 'ctx-alpha'
        ? [
            {
              id: 'item-alpha-1',
              title: 'auth_handler.rs',
              item_type: 'code',
              app_name: 'VS Code',
              relevance: 'primary',
              relevance_score: 0.95,
              reasons: ['Actively open in editor'],
              provenance: 'confirmed',
              created_at: '2026-10-22T08:00:00Z',
              content_snippet: 'pub fn authenticate()',
              is_suggested_addition: false,
            },
          ]
        : [
            {
              id: 'item-beta-1',
              title: 'data_pipeline.py',
              item_type: 'code',
              app_name: 'PyCharm',
              relevance: 'primary',
              relevance_score: 0.92,
              reasons: ['Modified recently'],
              provenance: 'confirmed',
              created_at: '2026-10-22T09:00:00Z',
              content_snippet: 'def run_pipeline():',
              is_suggested_addition: false,
            },
            {
              id: 'item-beta-2',
              title: 'dataset_spec.md',
              item_type: 'file',
              app_name: 'Obsidian',
              relevance: 'supporting',
              relevance_score: 0.75,
              reasons: ['Reference specification'],
              provenance: 'confirmed',
              created_at: '2026-10-22T09:30:00Z',
              content_snippet: '# Dataset Schema',
              is_suggested_addition: false,
            },
          ],
    } as any));

    vi.mocked(contextInteractionService.getResumeReadiness).mockImplementation(async (id) => ({
      context_id: id,
      readiness: {
        readiness: 'ready',
        formatted_explanation: id === 'ctx-alpha' ? 'Ready with auth workset' : 'Ready with dataset workset',
      },
    } as any));

    // 1a. Select Context Alpha
    useLauncherStore.getState().selectContextById('ctx-alpha');
    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha');
    expect(useLauncherStore.getState().isLoadingPreview).toBe(true);

    // Wait for debounce and async resolution
    await new Promise((resolve) => setTimeout(resolve, 60));

    const stateAlpha = useLauncherStore.getState();
    expect(stateAlpha.selectedContextId).toBe('ctx-alpha');
    expect(stateAlpha.isLoadingPreview).toBe(false);
    expect(stateAlpha.activePreview?.contextId).toBe('ctx-alpha');
    expect(stateAlpha.activePreview?.name).toBe('Alpha Project');
    expect(stateAlpha.activePreview?.taskTitle).toBe('Implement Auth');
    expect(stateAlpha.activePreview?.worksetItems).toHaveLength(1);
    expect(stateAlpha.activePreview?.worksetItems[0].id).toBe('item-alpha-1');
    expect(stateAlpha.activePreview?.worksetItems[0].title).toBe('auth_handler.rs');
    expect(stateAlpha.activePreview?.worksetPresentation?.populationState).toBe('complete');

    // 1b. Switch to Context Beta
    useLauncherStore.getState().selectContextById('ctx-beta');
    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-beta');
    expect(useLauncherStore.getState().isLoadingPreview).toBe(true);

    await new Promise((resolve) => setTimeout(resolve, 60));

    const stateBeta = useLauncherStore.getState();
    expect(stateBeta.selectedContextId).toBe('ctx-beta');
    expect(stateBeta.isLoadingPreview).toBe(false);
    expect(stateBeta.activePreview?.contextId).toBe('ctx-beta');
    expect(stateBeta.activePreview?.name).toBe('Beta Research');
    expect(stateBeta.activePreview?.taskTitle).toBe('Analyze Dataset');
    expect(stateBeta.activePreview?.worksetItems).toHaveLength(2);
    expect(stateBeta.activePreview?.worksetItems[0].id).toBe('item-beta-1');
    expect(stateBeta.activePreview?.worksetItems[1].id).toBe('item-beta-2');
    expect(stateBeta.activePreview?.worksetPresentation?.populationState).toBe('complete');
  });

  // =========================================================================
  // 2. Stale Asynchronous Response Prevention (Requirement 6)
  // =========================================================================

  it('2. discards delayed asynchronous responses from previous context selections', async () => {
    let resolveContextAWorkset: (value: any) => void;
    const slowContextAPromise = new Promise((resolve) => {
      resolveContextAWorkset = resolve;
    });

    vi.mocked(contextInteractionService.getContextState).mockImplementation(async (id) => ({
      context_id: id,
      context_name: id === 'ctx-alpha' ? 'Slow Context Alpha' : 'Fast Context Beta',
      state: 'active',
    } as any));

    vi.mocked(contextInteractionService.getContextWorkset).mockImplementation(async (id) => {
      if (id === 'ctx-alpha') {
        // Deliberately delay context A
        return (await slowContextAPromise) as any;
      }
      return {
        context_id: 'ctx-beta',
        context_name: 'Fast Context Beta',
        items: [
          {
            id: 'item-beta-fast',
            title: 'fast_beta.rs',
            item_type: 'code',
            relevance: 'primary',
            relevance_score: 0.9,
            provenance: 'confirmed',
          },
        ],
      } as any;
    });

    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValue({
      readiness: { readiness: 'ready' },
    } as any);

    // 2a. Select Context Alpha (slow response in-flight)
    useLauncherStore.getState().selectContextById('ctx-alpha');
    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha');

    // Wait for Context Alpha's debounce timer to fire its async request
    await new Promise((resolve) => setTimeout(resolve, 50));

    // 2b. Rapidly switch to Context Beta while Alpha request is still suspended
    useLauncherStore.getState().selectContextById('ctx-beta');
    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-beta');

    // Wait for Context Beta to resolve and update store
    await new Promise((resolve) => setTimeout(resolve, 60));

    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-beta');
    expect(useLauncherStore.getState().activePreview?.contextId).toBe('ctx-beta');
    expect(useLauncherStore.getState().activePreview?.name).toBe('Beta Research');
    expect(useLauncherStore.getState().activePreview?.worksetItems[0].id).toBe('item-beta-fast');

    // 2c. Now finally resolve the stale Context Alpha response
    resolveContextAWorkset!({
      context_id: 'ctx-alpha',
      context_name: 'Slow Context Alpha',
      items: [
        {
          id: 'item-alpha-stale',
          title: 'stale_alpha.rs',
          item_type: 'code',
          relevance: 'primary',
          relevance_score: 0.9,
          provenance: 'confirmed',
        },
      ],
    });

    // Wait a tick for microtasks
    await new Promise((resolve) => setTimeout(resolve, 20));

    // Stale response must be discarded: store still holds Context Beta!
    const finalState = useLauncherStore.getState();
    expect(finalState.selectedContextId).toBe('ctx-beta');
    expect(finalState.activePreview?.contextId).toBe('ctx-beta');
    expect(finalState.activePreview?.name).toBe('Beta Research');
    expect(finalState.activePreview?.worksetItems[0].id).toBe('item-beta-fast');
  });

  // =========================================================================
  // 3. Cache Deduplication and Invalidation (Requirement 9)
  // =========================================================================

  it('3. avoids fetching identical data repeatedly within TTL and invalidates on actions', async () => {
    const fetchSpy = vi.fn().mockImplementation(async (id: string) => ({
      context_id: id,
      context_name: 'Cached Context',
      items: [
        {
          id: 'item-cached-1',
          title: 'doc.md',
          item_type: 'file',
          relevance: 'primary',
          relevance_score: 0.9,
          provenance: 'confirmed',
        },
      ],
    }));

    vi.mocked(contextInteractionService.getContextWorkset).mockImplementation(fetchSpy);
    vi.mocked(contextInteractionService.getContextState).mockResolvedValue({
      context_id: 'ctx-alpha',
      state: 'active',
    } as any);
    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValue({
      readiness: { readiness: 'ready' },
    } as any);

    // Initial load: populates cache
    useLauncherStore.getState().selectContextById('ctx-alpha');
    await new Promise((resolve) => setTimeout(resolve, 60));

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(WORKSET_CACHE_TTL_MS).toBe(25000);
    expect(getLauncherPreviewCache().has('ctx-alpha')).toBe(true);

    // Second selection of the same context: served from cache immediately without new fetch
    fetchSpy.mockClear();
    useLauncherStore.getState().selectContextById('ctx-alpha');

    // Does not re-fetch from backend
    expect(fetchSpy).toHaveBeenCalledTimes(0);
    expect(useLauncherStore.getState().activePreview?.contextId).toBe('ctx-alpha');
    expect(useLauncherStore.getState().isLoadingPreview).toBe(false);

    // Mutating action on an item invalidates cache and triggers fresh fetch
    const itemActionSpy = vi.spyOn(quickActionsService, 'executeItemAction').mockResolvedValueOnce({
      actionId: 'item.mark_important',
      success: true,
      outcome: 'SUCCESS',
      message: 'Item pinned in context memory',
    });

    await useLauncherStore.getState().executeItemAction('item.mark_important', 'ctx-alpha', 'item-cached-1');
    expect(itemActionSpy).toHaveBeenCalled();

    // Cache entry for ctx-alpha was invalidated
    // Wait for the re-fetch triggered by executeItemAction to settle
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  // =========================================================================
  // 4. Error Recovery & Graceful Fallback (Requirements 7 & 8)
  // =========================================================================

  it('4. handles retrieval service errors gracefully without blocking context resumption', async () => {
    vi.mocked(contextInteractionService.getContextWorkset).mockRejectedValue(
      new Error('Context Engine IPC connection refused')
    );
    vi.mocked(contextInteractionService.getContextState).mockRejectedValue(
      new Error('Context Engine IPC connection refused')
    );
    vi.mocked(contextInteractionService.getResumeReadiness).mockRejectedValue(
      new Error('Context Engine IPC connection refused')
    );
    vi.spyOn(await import('../../services/contextWorksetRetrieval'), 'retrieveContextWorkset').mockRejectedValueOnce(
      new Error('Context Engine IPC connection refused')
    );

    useLauncherStore.getState().selectContextById('ctx-alpha');
    await new Promise((resolve) => setTimeout(resolve, 60));

    const state = useLauncherStore.getState();
    expect(state.isLoadingPreview).toBe(false);
    expect(state.previewError).toBe('Context Engine IPC connection refused');
    expect(state.activePreview).not.toBeNull();
    expect(state.activePreview?.contextId).toBe('ctx-alpha');
    expect(state.activePreview?.name).toBe('Alpha Project');
    expect(state.activePreview?.worksetItems).toEqual([]);
    expect(state.activePreview?.worksetPresentation?.populationState).toBe('empty');

    // Context resumption action is NOT blocked by workset retrieval error
    const resumeSpy = vi.spyOn(quickActionsService, 'executeContextAction').mockResolvedValueOnce({
      actionId: 'context.resume',
      success: true,
      outcome: 'SUCCESS',
      message: 'Resumed Alpha Project',
      resolvedContextId: 'ctx-alpha',
    });

    const resumeResult = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-alpha');
    expect(resumeResult.success).toBe(true);
    expect(resumeSpy).toHaveBeenCalledWith('context.resume', 'ctx-alpha', expect.anything());
  });

  // =========================================================================
  // 5. Empty and Partial Worksets (Requirement 7)
  // =========================================================================

  it('5. handles empty and partial worksets without fabricating information', async () => {
    // 5a. Empty Workset: No items, no active task
    vi.mocked(contextInteractionService.getContextState).mockResolvedValueOnce({
      context_id: 'ctx-alpha',
      context_name: 'Empty Workspace',
      state: 'active',
      task_title: null,
      last_meaningful_activity: null,
    } as any);

    vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValueOnce({
      context_id: 'ctx-alpha',
      context_name: 'Empty Workspace',
      task_title: null,
      items: [],
    } as any);

    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
      readiness: { readiness: 'partial' },
    } as any);

    useLauncherStore.getState().selectContextById('ctx-alpha');
    await new Promise((resolve) => setTimeout(resolve, 60));

    const emptyPreview = useLauncherStore.getState().activePreview;
    expect(emptyPreview?.worksetItems).toHaveLength(0);
    expect(emptyPreview?.taskTitle).toBeNull();
    expect(emptyPreview?.activitySummary).toBeNull();
    expect(emptyPreview?.worksetPresentation?.populationState).toBe('empty');
    expect(emptyPreview?.worksetPresentation?.hasTask).toBe(false);
    expect(emptyPreview?.worksetPresentation?.hasItems).toBe(false);

    // 5b. Partial Workset: Contains items, but no active task and some suggested additions
    invalidateLauncherPreviewCache('ctx-beta');
    vi.mocked(contextInteractionService.getContextState).mockResolvedValueOnce({
      context_id: 'ctx-beta',
      context_name: 'Partial Workspace',
      state: 'active',
      task_title: null,
    } as any);

    vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValueOnce({
      context_id: 'ctx-beta',
      context_name: 'Partial Workspace',
      task_title: null,
      items: [
        {
          id: 'item-suggested-inbox',
          title: 'Unfiled Notes.txt',
          item_type: 'text',
          relevance: 'related',
          relevance_score: 0.85,
          is_suggested_addition: true,
          provenance: 'inferred',
          created_at: '2026-10-22T08:00:00Z',
          content_snippet: 'Quick thoughts on architecture',
        },
      ],
    } as any);

    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
      readiness: { readiness: 'ready' },
    } as any);

    useLauncherStore.getState().selectContextById('ctx-beta');
    await new Promise((resolve) => setTimeout(resolve, 60));

    const partialPreview = useLauncherStore.getState().activePreview;
    expect(partialPreview?.taskTitle).toBeNull();
    expect(partialPreview?.worksetItems).toHaveLength(1);
    expect(partialPreview?.worksetItems[0].isSuggestedAddition).toBe(true);
    expect(partialPreview?.worksetItems[0].provenance).toBe('inferred');
    expect(partialPreview?.worksetPresentation?.populationState).toBe('partial');
    expect(partialPreview?.worksetPresentation?.hasSuggestions).toBe(true);
  });

  // =========================================================================
  // 6. Non-Blocking Context Selection and Resumption (Requirement 8)
  // =========================================================================

  it('6. allows immediate selection and resume actions without waiting for workset data to load', async () => {
    // Deliberately delay backend responses
    vi.mocked(contextInteractionService.getContextWorkset).mockImplementation(
      () => new Promise((resolve) => setTimeout(resolve, 200))
    );
    vi.mocked(contextInteractionService.getContextState).mockImplementation(
      () => new Promise((resolve) => setTimeout(resolve, 200))
    );

    const resumeActionSpy = vi.spyOn(quickActionsService, 'executeContextAction').mockResolvedValueOnce({
      actionId: 'context.resume',
      success: true,
      outcome: 'SUCCESS',
      message: 'Resumed context immediately',
      resolvedContextId: 'ctx-alpha',
    });

    // 6a. Select context: selectedContextId is set synchronously
    useLauncherStore.getState().selectContextById('ctx-alpha');
    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha');
    expect(useLauncherStore.getState().isLoadingPreview).toBe(true);

    // 6b. Dispatch resume immediately while preview is still loading
    const resumeResult = await useLauncherStore.getState().executeContextAction('context.resume', 'ctx-alpha');
    expect(resumeResult.success).toBe(true);
    expect(resumeActionSpy).toHaveBeenCalledWith('context.resume', 'ctx-alpha', expect.anything());
  });
});
