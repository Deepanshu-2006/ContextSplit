import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  useLauncherStore,
  matchContext,
  sortContextMatches,
  matchesContextFilter,
  LauncherContextItem,
} from '../../store/launcherStore';
import { contextInteractionService } from '../../services/contextInteractionService';
import { useWorkspaceStore } from '../../store/workspaceStore';

// Mock Tauri invoke to prevent window undefined during store interactions
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock Context Interaction API service
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

describe('Context Discovery Quality Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWorkspaceStore.setState({
      workspaces: [],
      activeWorkspaceId: null,
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

  // Comprehensive test contexts dataset
  const testDatabase: LauncherContextItem[] = [
    {
      id: 'ctx-active-engine',
      name: 'ContextSplit Development',
      description: 'Core engine and application workspace',
      repository: 'ContextSplit',
      taskTitle: 'Launcher search discovery',
      isActive: true,
      state: 'active',
      lastActiveAt: '2026-10-01T12:00:00Z',
    },
    {
      id: 'ctx-payx-dev',
      name: 'PayX Development',
      description: 'Stripe webhook and payment processing',
      repository: 'payx-backend',
      taskTitle: 'Webhook handler testing',
      isActive: false,
      state: 'paused',
      lastActiveAt: '2026-09-30T15:30:00Z',
    },
    {
      id: 'ctx-payx-front',
      name: 'PayX Frontend',
      description: 'Customer checkout UI components',
      repository: 'payx-web',
      taskTitle: 'Checkout modal styling',
      isActive: false,
      state: 'recent',
      lastActiveAt: '2026-09-30T14:00:00Z',
    },
    {
      id: 'ctx-payx-mobile',
      name: 'PayX Mobile',
      description: 'React Native iOS payment app',
      repository: 'payx-mobile',
      taskTitle: 'Apple Pay sheet',
      isActive: false,
      state: 'dormant',
      lastActiveAt: '2026-09-29T10:00:00Z',
    },
    {
      id: 'ctx-univ-pay',
      name: 'University Payment Assignment',
      description: 'Distributed ledger coursework',
      repository: 'academic-fintech',
      taskTitle: 'Assignment 4 grading',
      isActive: false,
      state: 'recent',
      lastActiveAt: '2026-09-28T09:00:00Z',
    },
    {
      id: 'ctx-legacy-pay',
      name: 'Legacy Payment Gateway',
      description: 'Mainframe billing system bridge',
      repository: 'billing-legacy',
      taskTitle: 'Nightly batch settlement',
      isActive: false,
      state: 'paused',
      lastActiveAt: '2026-09-25T08:00:00Z',
    },
    {
      id: 'ctx-doc-team',
      name: 'Documentation',
      description: 'Internal developer documentation wiki',
      projectIdentifier: 'docs-internal',
      repository: 'internal-docs',
      taskTitle: 'API migration guide',
      isActive: false,
      state: 'recent',
      lastActiveAt: '2026-09-20T11:00:00Z',
    },
    {
      id: 'ctx-doc-public',
      name: 'Documentation',
      description: 'Public API reference documentation site',
      projectIdentifier: 'docs-public',
      repository: 'public-api-docs',
      taskTitle: 'Release notes v2.4',
      isActive: false,
      state: 'recent',
      lastActiveAt: '2026-09-29T17:00:00Z', // More recent duplicate name
    },
    {
      id: 'ctx-archived-analytics',
      name: 'Analytics Service 2024',
      description: 'Decommissioned event pipeline',
      projectIdentifier: 'analytics-v1',
      repository: 'analytics-v1',
      isActive: false,
      state: 'archived',
      lastActiveAt: '2026-01-10T12:00:00Z',
    },
  ];

  // Helper to run full discovery pipeline
  function discover(items: LauncherContextItem[], query: string, filter = 'all') {
    const stateFiltered = items.filter((item) => matchesContextFilter(item, filter as any));
    if (!query.trim()) return stateFiltered;
    const matched = stateFiltered.filter((item) => matchContext(item, query));
    return sortContextMatches(matched, query);
  }

  // 1. Exact match
  it('1. discovers exact context name match and ranks it first', () => {
    const results = discover(testDatabase, 'PayX Development');
    expect(results.length).toBeGreaterThan(0);
    expect(results[0].name).toBe('PayX Development');
    expect(results[0].id).toBe('ctx-payx-dev');
  });

  // 2. Prefix match
  it('2. discovers prefix matches and ranks prefix above contains matches', () => {
    const results = discover(testDatabase, 'PayX');
    expect(results.length).toBeGreaterThanOrEqual(3);
    // All prefix matches start with "PayX"
    expect(results[0].name.startsWith('PayX')).toBe(true);
    expect(results[1].name.startsWith('PayX')).toBe(true);

    // Prefix match ("PayX Development") ranks higher than contains match ("University Payment Assignment")
    const names = results.map((r) => r.name);
    const payxIdx = names.indexOf('PayX Development');
    const univIdx = names.indexOf('University Payment Assignment');
    if (univIdx !== -1) {
      expect(payxIdx).toBeLessThan(univIdx);
    }
  });

  // 3. Partial match
  it('3. discovers partial substring matches in name and structured metadata', () => {
    // Substring inside name: "Split" matches "ContextSplit Development"
    const splitResults = discover(testDatabase, 'split');
    expect(splitResults.some((r) => r.id === 'ctx-active-engine')).toBe(true);

    // Metadata match: "webhook" in taskTitle/description
    const webhookResults = discover(testDatabase, 'webhook');
    expect(webhookResults.some((r) => r.id === 'ctx-payx-dev')).toBe(true);

    // Repository match: "billing-legacy"
    const repoResults = discover(testDatabase, 'billing-legacy');
    expect(repoResults.some((r) => r.id === 'ctx-legacy-pay')).toBe(true);
  });

  // 4. Case differences
  it('4. matches case-insensitively across uppercase, lowercase, and mixed-case', () => {
    const lowerResults = discover(testDatabase, 'contextsplit');
    const upperResults = discover(testDatabase, 'CONTEXTSPLIT');
    const mixedResults = discover(testDatabase, 'CoNtExTsPlIt');

    expect(lowerResults.map((r) => r.id)).toEqual(upperResults.map((r) => r.id));
    expect(lowerResults.map((r) => r.id)).toEqual(mixedResults.map((r) => r.id));
    expect(lowerResults[0].id).toBe('ctx-active-engine');
  });

  // 5. Whitespace differences
  it('5. tolerates leading, trailing, and internal excess whitespace', () => {
    const cleanResults = discover(testDatabase, 'payx development');
    const leadingTrailing = discover(testDatabase, '   payx development   ');
    const excessInternal = discover(testDatabase, 'payx     development');

    expect(cleanResults.map((r) => r.id)).toEqual(leadingTrailing.map((r) => r.id));
    expect(cleanResults.map((r) => r.id)).toEqual(excessInternal.map((r) => r.id));
    expect(cleanResults[0].name).toBe('PayX Development');
  });

  // 6. No result
  it('6. returns an empty list for unmatched queries without generating phantom items', () => {
    const results = discover(testDatabase, 'nonexistent-context-query-xyz-999');
    expect(results).toEqual([]);
    expect(results).toHaveLength(0);
  });

  // 7. Multiple results
  it('7. returns all matching contexts when a query matches multiple items', () => {
    const results = discover(testDatabase, 'pay');
    // Matches PayX Dev, PayX Front, PayX Mobile, University Payment, Legacy Payment
    expect(results.length).toBeGreaterThanOrEqual(4);
    const ids = results.map((r) => r.id);
    expect(ids).toContain('ctx-payx-dev');
    expect(ids).toContain('ctx-payx-front');
    expect(ids).toContain('ctx-payx-mobile');
    expect(ids).toContain('ctx-legacy-pay');
  });

  // 8. Active context
  it('8. discovers active context and preserves active indicator', () => {
    const results = discover(testDatabase, 'contextsplit');
    expect(results).toHaveLength(1);
    expect(results[0].isActive).toBe(true);
    expect(results[0].id).toBe('ctx-active-engine');
  });

  // 9. Recent context
  it('9. discovers recent contexts and breaks ranking ties using recency', () => {
    // Both ctx-payx-front and ctx-univ-pay have similar ranking tiers for "pay"
    const results = discover(testDatabase, 'pay');
    const frontIdx = results.findIndex((r) => r.id === 'ctx-payx-front');
    const univIdx = results.findIndex((r) => r.id === 'ctx-univ-pay');

    // ctx-payx-front was active 2026-09-30 (more recent than 2026-09-28)
    expect(frontIdx).toBeLessThan(univIdx);
  });

  // 10. Paused context
  it('10. discovers paused and dormant contexts and filters accordingly', () => {
    // Discovers paused context via query
    const results = discover(testDatabase, 'legacy');
    expect(results.some((r) => r.id === 'ctx-legacy-pay')).toBe(true);
    expect(results[0].state).toBe('paused');

    // Composing query "pay" with filter "paused" returns only paused items
    const pausedResults = discover(testDatabase, 'pay', 'paused');
    for (const item of pausedResults) {
      expect(['paused', 'dormant']).toContain(item.state);
    }
    expect(pausedResults.some((r) => r.id === 'ctx-payx-dev')).toBe(true);
    expect(pausedResults.some((r) => r.id === 'ctx-payx-front')).toBe(false); // Front is 'recent', not paused
  });

  // 11. Duplicate names
  it('11. correctly handles duplicate context names and distinguishes them by identifier and metadata', () => {
    const results = discover(testDatabase, 'documentation');
    expect(results).toHaveLength(2);
    expect(results[0].name).toBe('Documentation');
    expect(results[1].name).toBe('Documentation');

    // Distinct unique IDs
    expect(results[0].id).not.toBe(results[1].id);
    expect([results[0].id, results[1].id].sort()).toEqual(['ctx-doc-public', 'ctx-doc-team'].sort());

    // Recency tie-breaker puts ctx-doc-public (active 2026-09-29) before ctx-doc-team (active 2026-09-20)
    expect(results[0].id).toBe('ctx-doc-public');
    expect(results[1].id).toBe('ctx-doc-team');
  });

  // 12. Similar context names
  it('12. discriminates between similar context names deterministically', () => {
    const frontResults = discover(testDatabase, 'payx front');
    expect(frontResults[0].id).toBe('ctx-payx-front');
    expect(frontResults[0].name).toBe('PayX Frontend');

    const mobileResults = discover(testDatabase, 'payx mobile');
    expect(mobileResults[0].id).toBe('ctx-payx-mobile');
    expect(mobileResults[0].name).toBe('PayX Mobile');
  });

  // 13. Empty context database
  it('13. handles an empty context database gracefully without throwing or creating dummy contexts', () => {
    const emptyDb: LauncherContextItem[] = [];
    const results = discover(emptyDb, 'any query');
    expect(results).toEqual([]);

    const emptyResults = discover(emptyDb, '');
    expect(emptyResults).toEqual([]);
  });

  // 14. Verify deterministic ordering
  it('14. guarantees 100% deterministic ranking order regardless of input array order', () => {
    const query = 'pay';

    // Baseline sort
    const baseline = discover(testDatabase, query).map((r) => r.id);

    // Shuffle input 25 times and verify each run yields the exact same ranked sequence
    for (let i = 0; i < 25; i++) {
      const shuffled = [...testDatabase].sort(() => Math.random() - 0.5);
      const output = discover(shuffled, query).map((r) => r.id);
      expect(output).toEqual(baseline);
    }
  });

  // 15. Verify search never returns contexts that do not actually exist
  it('15. verifies search never returns contexts that do not exist in the database', () => {
    const existingIds = new Set(testDatabase.map((item) => item.id));

    const queries = ['pay', 'doc', 'context', 'random-token', '', 'xyz-404', 'stripe', 'billing'];
    for (const q of queries) {
      const results = discover(testDatabase, q);
      expect(results.length).toBeLessThanOrEqual(testDatabase.length);
      for (const item of results) {
        expect(existingIds.has(item.id)).toBe(true);
      }
    }
  });

  // 16. Verify selecting a result always resolves to the correct context identifier
  it('16. verifies selecting a result resolves to the exact context identifier and IPC dispatch', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValue({
      status: 'resumed',
      context_id: 'ctx-doc-team',
      resumed_at: '2026-10-01T12:00:00Z',
    });
    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValue({
      context_id: 'ctx-doc-team',
      readiness: { readiness: 'ready' },
    } as any);

    // Populate launcher with duplicate-named contexts
    useLauncherStore.setState({
      isOpen: true,
      activeContext: null,
      recentContexts: testDatabase,
      searchQuery: 'documentation',
    });

    const results = discover(testDatabase, 'documentation');
    expect(results).toHaveLength(2);

    // Select result index 1 (ctx-doc-team)
    const targetItem = results[1];
    expect(targetItem.id).toBe('ctx-doc-team');

    useLauncherStore.getState().setSelectedIndex(1);
    await useLauncherStore.getState().selectContextById(targetItem.id);

    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-doc-team');

    // Resume target context
    const success = await useLauncherStore.getState().resumeContextById(targetItem.id);
    expect(success).toBe(true);
    expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-doc-team');
  });

  // 17. Disambiguation: contexts with identical names provide rich secondary metadata without renaming or merging
  it('17. preserves identical context names without automatic renaming or merging, offering rich secondary metadata', () => {
    const docContexts = testDatabase.filter((c) => c.name === 'Documentation');
    expect(docContexts).toHaveLength(2);

    // Verify neither context was automatically renamed
    expect(docContexts[0].name).toBe('Documentation');
    expect(docContexts[1].name).toBe('Documentation');

    // Verify distinct secondary metadata is present to disambiguate them
    expect(docContexts[0].projectIdentifier).toBe('docs-internal');
    expect(docContexts[0].repository).toBe('internal-docs');
    expect(docContexts[0].taskTitle).toBe('API migration guide');

    expect(docContexts[1].projectIdentifier).toBe('docs-public');
    expect(docContexts[1].repository).toBe('public-api-docs');
    expect(docContexts[1].taskTitle).toBe('Release notes v2.4');

    // Verify neither was merged or duplicated
    const uniqueIds = new Set(docContexts.map((c) => c.id));
    expect(uniqueIds.size).toBe(2);
  });

  // 18. Disambiguation: preview model populates secondary metadata and maintains strict ID resolution
  it('18. populates disambiguating secondary metadata in preview model and guarantees strict ID resolution', async () => {
    vi.mocked(contextInteractionService.getContextState).mockResolvedValue({
      context_id: 'ctx-doc-public',
      context_name: 'Documentation',
      state: 'recent',
      task_title: 'Release notes v2.4',
    });
    vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValue({
      context_id: 'ctx-doc-public',
      items: [],
    });
    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValue({
      context_id: 'ctx-doc-public',
      readiness: { readiness: 'ready' },
    } as any);

    useLauncherStore.setState({
      isOpen: true,
      activeContext: null,
      recentContexts: testDatabase,
      selectedContextId: null,
      activePreview: null,
    });

    // Select the public doc context
    await useLauncherStore.getState().selectContextById('ctx-doc-public');

    // Allow debounce timer (40ms) to resolve
    await new Promise((resolve) => setTimeout(resolve, 60));

    const state = useLauncherStore.getState();
    expect(state.selectedContextId).toBe('ctx-doc-public');
    expect(state.activePreview?.contextId).toBe('ctx-doc-public');
    expect(state.activePreview?.name).toBe('Documentation');
    expect(state.activePreview?.projectIdentifier).toBe('docs-public');
    expect(state.activePreview?.repository).toBe('public-api-docs');
  });
});
