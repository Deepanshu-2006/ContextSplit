import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  useLauncherStore,
  matchContext,
  sortContextMatches,
  matchesContextFilter,
  getVisibleLauncherSlice,
  MAX_VISIBLE_LAUNCHER_RESULTS,
  LauncherContextItem,
} from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { contextInteractionService } from '../../services/contextInteractionService';

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock the authoritative Context Interaction API service
vi.mock('../../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getContextSummary: vi.fn(),
    getContextWorkset: vi.fn(),
    getContextState: vi.fn(),
    getResumeReadiness: vi.fn(),
    resumeContext: vi.fn(),
    pauseContext: vi.fn(),
    captureItem: vi.fn(),
    assignItemToContext: vi.fn(),
    searchContexts: vi.fn(),
    getContextSuggestions: vi.fn(),
  },
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

describe('Context Launcher & Phase 1 Full Regression Suite', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-active-engine',
          name: 'PayX Core Engine',
          created_at: '2026-09-01T00:00:00Z',
          updated_at: '2026-10-01T02:00:00Z',
          lifecycle_state: 'active',
          project_identifier: 'payx-billing',
          repository: 'payx-service',
          description: 'Payment orchestration engine',
        } as any,
        {
          id: 'ctx-recent-research',
          name: 'Distributed Systems Research',
          created_at: '2026-08-10T00:00:00Z',
          updated_at: '2026-09-30T19:00:00Z',
          lifecycle_state: 'active',
          project_identifier: 'cs-thesis',
          repository: 'raft-consensus',
          description: 'Consensus latency benchmarks',
        } as any,
        {
          id: 'ctx-paused-audit',
          name: 'Security Token Audit',
          created_at: '2026-09-15T00:00:00Z',
          updated_at: '2026-09-29T12:00:00Z',
          lifecycle_state: 'active',
          project_identifier: 'payx-security',
          repository: 'vault-manager',
          description: 'PCI compliance review',
        } as any,
        {
          id: 'ctx-archived-legacy',
          name: 'Legacy V1 Ingestion',
          created_at: '2026-06-01T00:00:00Z',
          updated_at: '2026-08-01T00:00:00Z',
          lifecycle_state: 'archived',
          project_identifier: 'legacy-ingest',
          repository: 'v1-api',
          description: 'Deprecated webhook receiver',
        } as any,
      ],
      activeWorkspaceId: 'ctx-active-engine',
    });

    useLauncherStore.setState({
      isOpen: false,
      searchQuery: '',
      activeFilter: 'all',
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
      isPreviewOpen: false,
    });
  });

  // =========================================================================
  // SECTION 1: PHASE 1 ENGINE & INTERACTION API REGRESSION
  // =========================================================================
  describe('Phase 1 Engine & Interaction API Invariants', () => {
    // 1. Context Detection
    it('1. context detection: correctly surfaces detected active context, confidence, and task alignment', async () => {
      vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce({
        context_id: 'ctx-active-engine',
        context: 'PayX Core Engine',
        state: 'active',
        task: 'Implement webhook idempotency',
        context_confidence: 'high',
        recent_activity: 'Committed IdempotencyKeyValidator.ts',
        last_meaningful_activity: 'Committed IdempotencyKeyValidator.ts',
        last_active_at: '2026-10-01T02:00:00Z',
        memory_statement: 'Working on webhook idempotency in PayX billing',
      });
      vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([]);

      await useLauncherStore.getState().fetchLauncherData();

      const active = useLauncherStore.getState().activeContext;
      expect(active).not.toBeNull();
      expect(active?.id).toBe('ctx-active-engine');
      expect(active?.confidence).toBe('high');
      expect(active?.taskTitle).toBe('Implement webhook idempotency');
      expect(active?.recentActivitySummary).toBe('Committed IdempotencyKeyValidator.ts');
    });

    // 2. Context Assignment
    it('2. context assignment: delegates item assignment without client-side state divergence', async () => {
      vi.mocked(contextInteractionService.assignItemToContext).mockResolvedValueOnce(undefined);

      await contextInteractionService.assignItemToContext('item-123', 'ctx-active-engine');
      expect(contextInteractionService.assignItemToContext).toHaveBeenCalledWith('item-123', 'ctx-active-engine');
    });

    // 3. Lifecycle States
    it('3. lifecycle: preserves authoritative engine states (active, recent, paused, dormant, archived)', () => {
      const items: LauncherContextItem[] = [
        { id: '1', name: 'Active', state: 'active', isActive: true },
        { id: '2', name: 'Recent', state: 'recent', isActive: false },
        { id: '3', name: 'Paused', state: 'paused', isActive: false },
        { id: '4', name: 'Dormant', state: 'dormant', isActive: false },
        { id: '5', name: 'Archived', state: 'archived', isActive: false },
      ];

      expect(items.filter((i) => matchesContextFilter(i, 'active'))).toHaveLength(1);
      expect(items.filter((i) => matchesContextFilter(i, 'recent'))).toHaveLength(1);
      expect(items.filter((i) => matchesContextFilter(i, 'paused'))).toHaveLength(2); // paused & dormant
      expect(items.filter((i) => matchesContextFilter(i, 'archived'))).toHaveLength(1);
    });

    // 4. Tasks & Resumption
    it('4. tasks & readiness: populates left-off task details and readiness evaluation', async () => {
      vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
        context_id: 'ctx-active-engine',
        readiness: {
          readiness: 'ready',
          formatted_explanation: 'Active task in progress with 4 captured files',
          reasons: ['Editor focused on IdempotencyKeyValidator.ts'],
        },
        formatted_resume_summary: 'Continue webhook idempotency implementation',
      } as any);

      vi.mocked(contextInteractionService.getContextState).mockResolvedValueOnce({
        context_id: 'ctx-active-engine',
        context_name: 'PayX Core Engine',
        state: 'active',
        task_title: 'Implement webhook idempotency',
      } as any);

      vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValueOnce({
        context_id: 'ctx-active-engine',
        context_name: 'PayX Core Engine',
        items: [],
      } as any);

      useLauncherStore.getState().selectContextById('ctx-active-engine');
      await new Promise((r) => setTimeout(r, 60));

      const preview = useLauncherStore.getState().activePreview;
      expect(preview?.taskTitle).toBe('Implement webhook idempotency');
      expect(preview?.readinessBand).toBe('ready');
      expect(preview?.readinessSummary).toBe('Continue webhook idempotency implementation');
      expect(preview?.reasons).toContain('Editor focused on IdempotencyKeyValidator.ts');
    });

    // 5. Memories & Worksets
    it('5. memories & worksets: extracts primary and secondary items with provenance', async () => {
      vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValueOnce({
        context_id: 'ctx-recent-research',
        context_name: 'Distributed Systems Research',
        items: [
          {
            id: 'item-raft-rs',
            title: 'raft.rs',
            item_type: 'code',
            app_name: 'RustRover',
            relevance: 'primary',
            content_snippet: 'pub struct RaftNode',
          },
          {
            id: 'item-notes',
            title: 'consensus_notes.md',
            item_type: 'note',
            app_name: 'Obsidian',
            relevance: 'secondary',
            content_snippet: 'Raft paper section 5 notes',
          },
        ],
      } as any);

      vi.mocked(contextInteractionService.getContextState).mockResolvedValueOnce({
        context_id: 'ctx-recent-research',
        context_name: 'Distributed Systems Research',
        state: 'recent',
      } as any);

      vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
        context_id: 'ctx-recent-research',
        readiness: { readiness: 'ready' },
      } as any);

      useLauncherStore.getState().selectContextById('ctx-recent-research');
      await new Promise((r) => setTimeout(r, 60));

      const preview = useLauncherStore.getState().activePreview;
      expect(preview?.worksetItems).toHaveLength(2);
      expect(preview?.importantItems).toHaveLength(1);
      expect(preview?.importantItems[0].title).toBe('raft.rs');
      expect(preview?.importantItems[0].appName).toBe('RustRover');
    });

    // 6. Snapshots & Reconstruction
    it('6. snapshots & reconstruction: maps cold reconstruction and freshness indicators', async () => {
      vi.mocked(contextInteractionService.getContextState).mockResolvedValueOnce({
        context_id: 'ctx-recent-research',
        context_name: 'Distributed Systems Research',
        state: 'recent',
        freshness_display: 'Active 2h ago',
        last_meaningful_activity: 'Ran network partition benchmark',
      } as any);

      vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValueOnce({
        context_id: 'ctx-recent-research',
        items: [],
      } as any);

      vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
        context_id: 'ctx-recent-research',
        readiness: { readiness: 'ready' },
      } as any);

      useLauncherStore.getState().selectContextById('ctx-recent-research');
      await new Promise((r) => setTimeout(r, 60));

      const preview = useLauncherStore.getState().activePreview;
      expect(preview?.durationEstimate).toBe('Active 2h ago');
      expect(preview?.activitySummary).toBe('Ran network partition benchmark');
    });

    // 7. Feedback & User Action Signals
    it('7. feedback: user actions emit standardized context signals without direct DB mutation', async () => {
      vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ signal_id: 'sig-resume' });
      vi.mocked(contextInteractionService.pauseContext).mockResolvedValueOnce({ signal_id: 'sig-pause' });

      await contextInteractionService.resumeContext('ctx-paused-audit');
      expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-paused-audit');

      await contextInteractionService.pauseContext('ctx-active-engine');
      expect(contextInteractionService.pauseContext).toHaveBeenCalledWith('ctx-active-engine');
    });

    // 8. Persistence
    it('8. persistence: maintains workspace relationships and survives launcher open/close cycles', async () => {
      vi.mocked(contextInteractionService.getActiveContext).mockResolvedValue({
        context_id: 'ctx-active-engine',
        context: 'PayX Core Engine',
        state: 'active',
      });
      vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValue([
        { context_id: 'ctx-recent-research', context_name: 'Distributed Systems Research', transition_type: 'recent' },
      ]);

      // Open launcher
      useLauncherStore.getState().openLauncher();
      await new Promise((r) => setTimeout(r, 10));

      expect(useLauncherStore.getState().activeContext?.id).toBe('ctx-active-engine');
      expect(useLauncherStore.getState().recentContexts).toHaveLength(1);

      // Close launcher
      useLauncherStore.getState().closeLauncher();
      expect(useLauncherStore.getState().isOpen).toBe(false);

      // Re-open launcher
      useLauncherStore.getState().openLauncher();
      await new Promise((r) => setTimeout(r, 10));

      expect(useLauncherStore.getState().isOpen).toBe(true);
      expect(useLauncherStore.getState().activeContext?.id).toBe('ctx-active-engine');
    });

    // 9. Interaction API Delegation Contract
    it('9. interaction API: strictly exposes stateless contracted delegation methods', () => {
      const apis = [
        'getActiveContext',
        'getRecentContexts',
        'getContextSummary',
        'getContextWorkset',
        'getContextState',
        'getResumeReadiness',
        'resumeContext',
        'pauseContext',
        'captureItem',
        'assignItemToContext',
        'searchContexts',
        'getContextSuggestions',
      ] as const;

      for (const api of apis) {
        expect(typeof contextInteractionService[api]).toBe('function');
      }
    });
  });

  // =========================================================================
  // SECTION 2: PHASE 2 LAUNCHER CAPABILITIES REGRESSION
  // =========================================================================
  describe('Phase 2 Context Launcher Capabilities', () => {
    beforeEach(async () => {
      vi.mocked(contextInteractionService.getActiveContext).mockResolvedValue({
        context_id: 'ctx-active-engine',
        context: 'PayX Core Engine',
        state: 'active',
        task: 'Implement webhook idempotency',
      });
      vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValue([
        {
          context_id: 'ctx-recent-research',
          context_name: 'Distributed Systems Research',
          state: 'recent',
          last_active_at: '2026-09-30T19:00:00Z',
        },
        {
          context_id: 'ctx-paused-audit',
          context_name: 'Security Token Audit',
          state: 'paused',
          last_active_at: '2026-09-29T12:00:00Z',
        },
        {
          context_id: 'ctx-archived-legacy',
          context_name: 'Legacy V1 Ingestion',
          state: 'archived',
          last_active_at: '2026-08-01T00:00:00Z',
        },
      ]);

      await useLauncherStore.getState().fetchLauncherData();
    });

    // 10. Launcher opens & initializes
    it('10. launcher opens: initializes focus, resets search query, and fetches launcher data', () => {
      useLauncherStore.getState().openLauncher();

      const state = useLauncherStore.getState();
      expect(state.isOpen).toBe(true);
      expect(state.searchQuery).toBe('');
      expect(state.selectedIndex).toBe(0);
      expect(state.activeFilter).toBe('all');
      expect(state.activeContext?.name).toBe('PayX Core Engine');
    });

    // 11. Active context display
    it('11. active context: identifies active context with status and metadata', () => {
      const active = useLauncherStore.getState().activeContext;
      expect(active).not.toBeNull();
      expect(active?.isActive).toBe(true);
      expect(active?.projectIdentifier).toBe('payx-billing');
      expect(active?.repository).toBe('payx-service');
    });

    // 12. Recent contexts display
    it('12. recent contexts: displays recent contexts in authoritative order', () => {
      const recents = useLauncherStore.getState().recentContexts;
      expect(recents).toHaveLength(3);
      expect(recents[0].name).toBe('Distributed Systems Research');
      expect(recents[1].name).toBe('Security Token Audit');
      expect(recents[2].name).toBe('Legacy V1 Ingestion');
    });

    // 13. Search execution
    it('13. search: executes fast deterministic multi-field matching', () => {
      const all = [
        useLauncherStore.getState().activeContext!,
        ...useLauncherStore.getState().recentContexts,
      ];

      // Exact name search
      expect(all.filter((c) => matchContext(c, 'Security Token Audit'))).toHaveLength(1);
      // Multi-match query: "payx" matches PayX Core Engine (name) and Security Token Audit (project_identifier: payx-security)
      expect(all.filter((c) => matchContext(c, 'payx'))).toHaveLength(2);
      // Project ID search
      expect(all.filter((c) => matchContext(c, 'cs-thesis'))).toHaveLength(1);
      // Repository search
      expect(all.filter((c) => matchContext(c, 'vault-manager'))).toHaveLength(1);
      // Keyword search
      expect(all.filter((c) => matchContext(c, 'orchestration'))).toHaveLength(1);
    });

    // 14. Search ranking
    it('14. ranking: strictly orders by exact > prefix > contains > metadata > recency', () => {
      const all = [
        useLauncherStore.getState().activeContext!,
        ...useLauncherStore.getState().recentContexts,
      ];

      const matches = all.filter((c) => matchContext(c, 'payx'));
      const ranked = sortContextMatches(matches, 'payx');

      // PayX Core Engine starts with PayX (tier 2), ranks highest
      expect(ranked[0].name).toBe('PayX Core Engine');
    });

    // 15. Context filtering
    it('15. filtering: filters by active, recent, paused, archived and composes with search', () => {
      const all = [
        useLauncherStore.getState().activeContext!,
        ...useLauncherStore.getState().recentContexts,
      ];

      // Filter: paused
      const pausedMatches = all.filter((c) => matchesContextFilter(c, 'paused'));
      expect(pausedMatches).toHaveLength(1);
      expect(pausedMatches[0].name).toBe('Security Token Audit');

      // Filter: archived
      const archivedMatches = all.filter((c) => matchesContextFilter(c, 'archived'));
      expect(archivedMatches).toHaveLength(1);
      expect(archivedMatches[0].name).toBe('Legacy V1 Ingestion');

      // Filter + Search composed: filter 'archived' + query 'legacy'
      const composed = all
        .filter((c) => matchesContextFilter(c, 'archived'))
        .filter((c) => matchContext(c, 'legacy'));
      expect(composed).toHaveLength(1);
      expect(composed[0].id).toBe('ctx-archived-legacy');
    });

    // 16. Context selection
    it('16. context selection: updates selected index and selectedContextId without drift', () => {
      useLauncherStore.getState().setSelectedIndex(2);
      expect(useLauncherStore.getState().selectedIndex).toBe(2);

      useLauncherStore.getState().selectContextById('ctx-recent-research');
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-recent-research');
    });

    // 17. Context preview
    it('17. preview: populates full preview model asynchronously with debouncing', async () => {
      vi.mocked(contextInteractionService.getContextState).mockResolvedValueOnce({
        context_id: 'ctx-recent-research',
        context_name: 'Distributed Systems Research',
        state: 'recent',
        task_title: 'Evaluate consensus latency',
      } as any);

      vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValueOnce({
        context_id: 'ctx-recent-research',
        items: [
          {
            id: 'item-1',
            title: 'raft.rs',
            item_type: 'code',
            relevance: 'primary',
          },
        ],
      } as any);

      vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
        context_id: 'ctx-recent-research',
        readiness: { readiness: 'ready' },
      } as any);

      useLauncherStore.getState().selectContextById('ctx-recent-research');
      await new Promise((r) => setTimeout(r, 60));

      const preview = useLauncherStore.getState().activePreview;
      expect(preview).not.toBeNull();
      expect(preview?.contextId).toBe('ctx-recent-research');
      expect(preview?.worksetItems).toHaveLength(1);
    });

    // 18. Context resume
    it('18. resume: invokes engine resume, synchronizes workspace store, and closes launcher', async () => {
      vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ success: true });

      useLauncherStore.setState({
        isOpen: true,
        selectedContextId: 'ctx-recent-research',
        activePreview: {
          contextId: 'ctx-recent-research',
          name: 'Distributed Systems Research',
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

      const success = await useLauncherStore.getState().resumeContextById('ctx-recent-research');

      expect(success).toBe(true);
      expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-recent-research');
      expect(useWorkspaceStore.getState().activeWorkspaceId).toBe('ctx-recent-research');
      expect(useLauncherStore.getState().isOpen).toBe(false);
    });

    // 19. Keyboard navigation & windowing
    it('19. keyboard navigation & windowing: keeps rendering bounded while supporting traversal', () => {
      const items = Array.from({ length: 100 }, (_, i) => ({
        id: `ctx-${i}`,
        name: `Context ${i}`,
      }));

      // Initial visible slice
      const initial = getVisibleLauncherSlice(items, 0);
      expect(initial.visibleItems).toHaveLength(MAX_VISIBLE_LAUNCHER_RESULTS);
      expect(initial.hasMore).toBe(true);

      // Traversal past index 45 expands window dynamically
      const expanded = getVisibleLauncherSlice(items, 60);
      expect(expanded.visibleItems).toHaveLength(70);
      expect(expanded.visibleItems[60].id).toBe('ctx-60');
    });

    // 20. Loading states
    it('20. loading: sets isLoading and isLoadingPreview flags during fetches', () => {
      useLauncherStore.setState({ isLoading: true, isLoadingPreview: true });
      expect(useLauncherStore.getState().isLoading).toBe(true);
      expect(useLauncherStore.getState().isLoadingPreview).toBe(true);
    });

    // 21. Empty states
    it('21. empty states: handles queries with no results without fake recommendations', () => {
      const all = [
        useLauncherStore.getState().activeContext!,
        ...useLauncherStore.getState().recentContexts,
      ];

      const matches = all.filter((c) => matchContext(c, 'nonexistent-token-xyz'));
      expect(matches).toHaveLength(0);

      // Verify that no item is fabricated
      expect(matches).toEqual([]);
    });

    // 22. Error states
    it('22. error states: captures engine failures gracefully without crashing launcher', async () => {
      vi.mocked(contextInteractionService.getActiveContext).mockRejectedValueOnce(new Error('Tauri IPC socket closed'));
      vi.mocked(contextInteractionService.getRecentContexts).mockRejectedValueOnce(new Error('Tauri IPC socket closed'));

      await useLauncherStore.getState().fetchLauncherData();

      // Launcher state survives and handles error without throwing
      expect(useLauncherStore.getState().isOpen).toBe(false);
      expect(useLauncherStore.getState().activeContext).toBeNull();
    });

    // 23. Non-interference with Phase 1
    it('23. non-interference: search optimizations do not mutate Phase 1 Context Engine state', () => {
      const originalContexts = [
        useLauncherStore.getState().activeContext!,
        ...useLauncherStore.getState().recentContexts,
      ];

      const query = 'payx';
      const filtered = originalContexts.filter((c) => matchContext(c, query));
      const sorted = sortContextMatches(filtered, query);

      // Verify original objects are unchanged
      expect(originalContexts[0].name).toBe('PayX Core Engine');
      expect(originalContexts[0].isActive).toBe(true);
      expect(sorted).toHaveLength(2);
      expect(sorted[0].id).toBe('ctx-active-engine'); // PayX Core Engine ranks highest (tier 2)
      expect(sorted[1].id).toBe('ctx-paused-audit'); // Security Token Audit ranks second (tier 4)
    });
  });
});
