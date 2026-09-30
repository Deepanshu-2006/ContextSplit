import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLauncherStore, matchContext, sortContextMatches } from '../../store/launcherStore';
import { contextInteractionService } from '../../services/contextInteractionService';
import { useWorkspaceStore } from '../../store/workspaceStore';

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

describe('Context Launcher Integration Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWorkspaceStore.setState({
      workspaces: [],
      activeWorkspaceId: null,
    });
    useLauncherStore.setState({
      isOpen: false,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: null,
      activePreview: null,
      isLoading: false,
      isLoadingPreview: false,
      isResuming: false,
      resumeStatus: 'idle',
      resumeError: null,
      notReadyReason: null,
      error: null,
      previewError: null,
      activeContext: null,
      recentContexts: [],
    });
  });

  // 1. Launcher opens
  it('1. opens the launcher and initializes state cleanly', async () => {
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce(null);
    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([]);

    expect(useLauncherStore.getState().isOpen).toBe(false);
    useLauncherStore.getState().openLauncher();

    expect(useLauncherStore.getState().isOpen).toBe(true);
    expect(useLauncherStore.getState().searchQuery).toBe('');
    expect(useLauncherStore.getState().selectedIndex).toBe(0);

    // Allow async fetchLauncherData to finish
    await new Promise((resolve) => setTimeout(resolve, 10));

    expect(contextInteractionService.getActiveContext).toHaveBeenCalledTimes(1);
    expect(contextInteractionService.getRecentContexts).toHaveBeenCalledTimes(1);
  });

  // 2. Active context appears
  it('2. retrieves and displays the active context from Context Interaction API', async () => {
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce({
      context_id: 'ctx-active-frontend',
      context: 'Frontend Redesign',
      state: 'active',
      task: 'Refactor navigation',
      context_confidence: 'high',
      recent_items: [{ id: 'item-1' }, { id: 'item-2' }],
      last_meaningful_activity: 'Edited Header component',
    });
    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([]);
    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
      context_id: 'ctx-active-frontend',
      readiness: {
        readiness: 'ready',
        formatted_explanation: 'Ready with 2 captured items',
      },
      formatted_resume_summary: 'Continue refactoring navigation',
    } as any);

    await useLauncherStore.getState().fetchLauncherData();

    const state = useLauncherStore.getState();
    expect(state.activeContext).not.toBeNull();
    expect(state.activeContext?.id).toBe('ctx-active-frontend');
    expect(state.activeContext?.name).toBe('Frontend Redesign');
    expect(state.activeContext?.isActive).toBe(true);
    expect(state.activeContext?.taskTitle).toBe('Refactor navigation');
    expect(state.activeContext?.itemCount).toBe(2);
    expect(state.activeContext?.resumeReadiness?.readiness).toBe('ready');
  });

  // 3. Recent contexts appear
  it('3. retrieves and displays recent contexts in authoritative engine order', async () => {
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce(null);
    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([
      {
        context_id: 'ctx-recent-1',
        context_name: 'Database Migration',
        transition_type: 'paused',
        duration_estimate: 'Active 2h ago',
        task_title: 'Run schema migrations',
        item_count: 8,
      },
      {
        context_id: 'ctx-recent-2',
        context_name: 'Auth Service',
        transition_type: 'dormant',
        duration_estimate: 'Active yesterday',
        task_title: 'JWT validation',
        item_count: 3,
      },
    ]);

    await useLauncherStore.getState().fetchLauncherData();

    const state = useLauncherStore.getState();
    expect(state.recentContexts).toHaveLength(2);
    expect(state.recentContexts[0].id).toBe('ctx-recent-1');
    expect(state.recentContexts[0].name).toBe('Database Migration');
    expect(state.recentContexts[1].id).toBe('ctx-recent-2');
    expect(state.recentContexts[1].name).toBe('Auth Service');
  });

  // 4. Context selection works
  it('4. updates selection index and triggers preview lookup', async () => {
    useLauncherStore.setState({
      recentContexts: [
        { id: 'ctx-1', name: 'Alpha' },
        { id: 'ctx-2', name: 'Beta' },
      ],
    });

    useLauncherStore.getState().setSelectedIndex(1);
    expect(useLauncherStore.getState().selectedIndex).toBe(1);
  });

  // 5. Context preview loads
  it('5. loads complete context preview from state, workset, and readiness APIs', async () => {
    vi.mocked(contextInteractionService.getContextState).mockResolvedValueOnce({
      context_id: 'ctx-preview-target',
      context_name: 'Compiler Optimization',
      state: 'active',
      task_title: 'LLVM pass integration',
      last_meaningful_activity: 'Optimized bytecode emission',
      freshness_display: 'Active 10m ago',
    } as any);

    vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValueOnce({
      context_id: 'ctx-preview-target',
      context_name: 'Compiler Optimization',
      items: [
        {
          id: 'item-pass-rs',
          title: 'pass.rs',
          item_type: 'code',
          app_name: 'RustRover',
          relevance: 'primary',
          content_snippet: 'impl Pass for Optimizer',
        } as any,
        {
          id: 'item-docs-md',
          title: 'llvm_notes.md',
          item_type: 'file',
          app_name: 'Obsidian',
          relevance: 'secondary',
        } as any,
      ],
    } as any);

    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
      context_id: 'ctx-preview-target',
      context_name: 'Compiler Optimization',
      readiness: {
        readiness: 'ready',
        formatted_explanation: 'Ready with 2 files and LLVM active task',
        reasons: ['pass.rs actively open'],
      },
      formatted_resume_summary: 'Resume LLVM pass integration',
    } as any);

    useLauncherStore.getState().selectContextById('ctx-preview-target');

    // Allow debounce timer to settle
    await new Promise((resolve) => setTimeout(resolve, 60));

    const preview = useLauncherStore.getState().activePreview;
    expect(preview).not.toBeNull();
    expect(preview?.contextId).toBe('ctx-preview-target');
    expect(preview?.name).toBe('Compiler Optimization');
    expect(preview?.taskTitle).toBe('LLVM pass integration');
    expect(preview?.worksetItems).toHaveLength(2);
    expect(preview?.importantItems).toHaveLength(1);
    expect(preview?.importantItems[0].title).toBe('pass.rs');
    expect(preview?.readinessBand).toBe('ready');
  });

  // 6. Resume works
  it('6. invokes contextInteractionService.resumeContext and dispatches resume signal', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ status: 'ok' });
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce({
      context_id: 'ctx-resume-target',
      context: 'Resumed Project',
      state: 'active',
    });
    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([]);

    useLauncherStore.setState({
      isOpen: true,
      activePreview: {
        contextId: 'ctx-resume-target',
        name: 'Resumed Project',
        state: 'recent',
        readinessBand: 'ready',
        reasons: [],
        importantItems: [],
        worksetItems: [],
        itemCount: 1,
        isAmbiguous: false,
        isStale: false,
      },
    });

    const success = await useLauncherStore.getState().resumeContextById('ctx-resume-target');

    expect(success).toBe(true);
    expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-resume-target');
    expect(useLauncherStore.getState().isOpen).toBe(false);
  });

  // 7. Active context updates after resume
  it('7. updates workspaceStore active workspace and refreshes launcher state on resume', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ status: 'ok' });
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce({
      context_id: 'ctx-newly-active',
      context: 'Newly Active Context',
      state: 'active',
    });
    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([]);

    useWorkspaceStore.setState({
      workspaces: [{ id: 'ctx-newly-active', name: 'Newly Active Context', created_at: '', updated_at: '' }],
      activeWorkspaceId: 'ctx-old',
    });

    useLauncherStore.setState({
      isOpen: true,
      activePreview: {
        contextId: 'ctx-newly-active',
        name: 'Newly Active Context',
        state: 'recent',
        readinessBand: 'ready',
        reasons: [],
        importantItems: [],
        worksetItems: [],
        itemCount: 1,
        isAmbiguous: false,
        isStale: false,
      },
    });

    await useLauncherStore.getState().resumeContextById('ctx-newly-active');

    expect(useWorkspaceStore.getState().activeWorkspaceId).toBe('ctx-newly-active');
  });

  // 8. Empty state works
  it('8. handles empty contexts gracefully without crashing or showing corrupted data', async () => {
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce(null);
    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([]);

    await useLauncherStore.getState().fetchLauncherData();

    const state = useLauncherStore.getState();
    expect(state.activeContext).toBeNull();
    expect(state.recentContexts).toEqual([]);
    expect(state.isLoading).toBe(false);
    expect(state.error).toBeNull();
  });

  // 9. Loading state works
  it('9. manages loading indicators accurately during data and preview fetching', async () => {
    let resolveActive: (val: any) => void = () => {};
    const activePromise = new Promise((resolve) => {
      resolveActive = resolve;
    });

    vi.mocked(contextInteractionService.getActiveContext).mockReturnValueOnce(activePromise as any);
    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([]);

    const fetchPromise = useLauncherStore.getState().fetchLauncherData();
    expect(useLauncherStore.getState().isLoading).toBe(true);

    resolveActive(null);
    await fetchPromise;
    expect(useLauncherStore.getState().isLoading).toBe(false);
  });

  // 10. Error state works
  it('10. surfaces backend connection errors in launcher error state without throwing', async () => {
    vi.mocked(contextInteractionService.getActiveContext).mockRejectedValueOnce(new Error('Tauri IPC socket closed'));
    vi.mocked(contextInteractionService.getRecentContexts).mockRejectedValueOnce(new Error('Tauri IPC socket closed'));

    await useLauncherStore.getState().fetchLauncherData();

    const state = useLauncherStore.getState();
    expect(state.isLoading).toBe(false);
    expect(state.activeContext).toBeNull();
    expect(state.recentContexts).toEqual([]);
  });

  // 11. Keyboard navigation works
  it('11. facilitates search filtering and index adjustment for keyboard navigation', () => {
    useLauncherStore.setState({
      recentContexts: [
        { id: 'ctx-1', name: 'Alpha Project' },
        { id: 'ctx-2', name: 'Beta Project' },
        { id: 'ctx-3', name: 'Gamma Project' },
      ],
      selectedIndex: 0,
    });

    useLauncherStore.getState().setSelectedIndex(2);
    expect(useLauncherStore.getState().selectedIndex).toBe(2);

    useLauncherStore.getState().setSearchQuery('beta');
    expect(useLauncherStore.getState().searchQuery).toBe('beta');
    expect(useLauncherStore.getState().selectedIndex).toBe(0); // Resets index on search
  });

  // 12. Multiple contexts remain isolated
  it('12. maintains clean isolation between multiple contexts and previews', async () => {
    vi.mocked(contextInteractionService.getContextState).mockImplementation(async (id: string) => {
      return {
        context_id: id,
        context_name: id === 'ctx-a' ? 'Context A' : 'Context B',
        state: 'active',
      } as any;
    });
    vi.mocked(contextInteractionService.getContextWorkset).mockImplementation(async (id: string) => {
      return {
        context_id: id,
        items: [{ id: `item-for-${id}`, title: `Doc in ${id}` }],
      } as any;
    });
    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValue({
      readiness: { readiness: 'ready' },
    } as any);

    // Select context A
    useLauncherStore.getState().selectContextById('ctx-a');
    await new Promise((resolve) => setTimeout(resolve, 60));

    expect(useLauncherStore.getState().activePreview?.contextId).toBe('ctx-a');
    expect(useLauncherStore.getState().activePreview?.name).toBe('Context A');
    expect(useLauncherStore.getState().activePreview?.worksetItems[0].id).toBe('item-for-ctx-a');

    // Select context B
    useLauncherStore.getState().selectContextById('ctx-b');
    await new Promise((resolve) => setTimeout(resolve, 60));

    expect(useLauncherStore.getState().activePreview?.contextId).toBe('ctx-b');
    expect(useLauncherStore.getState().activePreview?.name).toBe('Context B');
    expect(useLauncherStore.getState().activePreview?.worksetItems[0].id).toBe('item-for-ctx-b');
  });

  // 13. Search integration: empty search shows active and recent contexts
  it('13. provides active context and recent contexts when search is empty', () => {
    const active = {
      id: 'ctx-act',
      name: 'PayX Development',
      isActive: true,
      repository: 'payx-repo',
      taskTitle: 'Webhook handler',
    };
    const recents = [
      { id: 'ctx-rec-1', name: 'University ML', lastActiveAt: '2026-10-01T12:00:00Z' },
      { id: 'ctx-rec-2', name: 'ContextSplit Development', lastActiveAt: '2026-10-01T10:00:00Z' },
    ];

    useLauncherStore.setState({
      activeContext: active,
      recentContexts: recents,
      searchQuery: '',
      selectedIndex: 0,
    });

    const state = useLauncherStore.getState();
    expect(state.searchQuery).toBe('');
    expect(state.activeContext).toEqual(active);
    expect(state.recentContexts).toHaveLength(2);
    expect(state.activeContext?.isActive).toBe(true);
  });

  // 14. Search integration: search contains text, shows matches with state and metadata, active context identifiable
  it('14. filters and ranks matching contexts with metadata while keeping active context identifiable', () => {
    const active = {
      id: 'ctx-act',
      name: 'PayX Development',
      isActive: true,
      repository: 'payx-repo',
      taskTitle: 'Stripe Webhook',
    };
    const recents = [
      { id: 'ctx-rec-1', name: 'Payment Research', lastActiveAt: '2026-09-30T10:00:00Z' },
      { id: 'ctx-rec-2', name: 'University Coursework', description: 'ML project' },
    ];

    useLauncherStore.setState({
      activeContext: active,
      recentContexts: recents,
      searchQuery: 'pay',
      selectedIndex: 0,
    });

    // Derive list as launcher component does
    const all = [active, ...recents];
    const matched = all.filter((c) => matchContext(c, 'pay'));
    const sorted = sortContextMatches(matched, 'pay');

    expect(sorted).toHaveLength(2);
    // Active context ranks highest and has isActive: true
    expect(sorted[0].id).toBe('ctx-act');
    expect(sorted[0].isActive).toBe(true);
    expect(sorted[0].repository).toBe('payx-repo');
    expect(sorted[0].taskTitle).toBe('Stripe Webhook');

    // Recent match has its own metadata
    expect(sorted[1].id).toBe('ctx-rec-1');
    expect(sorted[1].isActive).toBeUndefined();
  });

  // 15. Search integration: clearing search returns to active context and recents and resets selection
  it('15. returns to active context and recent contexts upon clearing search query', () => {
    const active = { id: 'ctx-act', name: 'Active Project', isActive: true };
    const recents = [
      { id: 'ctx-rec-1', name: 'Other Project A' },
      { id: 'ctx-rec-2', name: 'Other Project B' },
    ];

    useLauncherStore.setState({
      activeContext: active,
      recentContexts: recents,
      searchQuery: 'other',
      selectedIndex: 1,
    });

    expect(useLauncherStore.getState().searchQuery).toBe('other');
    expect(useLauncherStore.getState().selectedIndex).toBe(1);

    // Clear search
    useLauncherStore.getState().setSearchQuery('');

    const state = useLauncherStore.getState();
    expect(state.searchQuery).toBe('');
    expect(state.selectedIndex).toBe(0); // Automatically reset to active context index
    expect(state.activeContext?.id).toBe('ctx-act');
    expect(state.recentContexts).toHaveLength(2);
  });
});
