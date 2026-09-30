import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  useLauncherStore,
  matchContext,
  sortContextMatches,
  matchesContextFilter,
  getContextSearchTier,
  LauncherContextItem,
} from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { contextInteractionService } from '../../services/contextInteractionService';

// Mock Tauri invoke to prevent window undefined during workspace select
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock the authoritative Context Interaction API service
vi.mock('../../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getResumeReadiness: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    resumeContext: vi.fn(),
    pauseContext: vi.fn(),
  },
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

/**
 * Real ContextSplit Phase 1 Datasets
 * Represents realistic developer contexts created through normal daily development usage:
 * 1. PayX Development (active payments context)
 * 2. University Research (recent distributed systems thesis work)
 * 3. Payment Security Audit (paused PCI-DSS review)
 * 4. ContextSplit Engine (recent desktop launcher work)
 * 5. Auth0 SSO Integration (dormant identity migration)
 * 6. Legacy Payment Gateway (archived historical repository)
 */
const realWorkspaces = [
  {
    id: 'ctx-payx-dev',
    name: 'PayX Development',
    created_at: '2026-09-01T10:00:00Z',
    updated_at: '2026-10-01T02:00:00Z',
    lifecycle_state: 'active' as const,
    project_identifier: 'payx-core',
    repository: 'payx-billing-service',
    description: 'Core payment engine handling billing subscriptions and webhooks',
  },
  {
    id: 'ctx-univ-research',
    name: 'University Research',
    created_at: '2026-08-15T09:00:00Z',
    updated_at: '2026-09-30T18:30:00Z',
    lifecycle_state: 'active' as const,
    project_identifier: 'cs-thesis',
    repository: 'distributed-systems-research',
    description: 'Academic study evaluating Raft consensus latency and leader election',
  },
  {
    id: 'ctx-pay-security',
    name: 'Payment Security Audit',
    created_at: '2026-09-10T11:00:00Z',
    updated_at: '2026-09-29T14:00:00Z',
    lifecycle_state: 'active' as const,
    project_identifier: 'payx-security',
    repository: 'payx-billing-service',
    description: 'Audit token encryption in transit and rest across payment flows',
  },
  {
    id: 'ctx-contextsplit-engine',
    name: 'ContextSplit Engine',
    created_at: '2026-09-20T08:00:00Z',
    updated_at: '2026-09-30T22:15:00Z',
    lifecycle_state: 'active' as const,
    project_identifier: 'contextsplit',
    repository: 'contextsplit-desktop',
    description: 'Lightweight context switching and search interface for macOS and Windows',
  },
  {
    id: 'ctx-auth0-sso',
    name: 'Auth0 SSO Integration',
    created_at: '2026-08-01T14:00:00Z',
    updated_at: '2026-09-15T11:00:00Z',
    lifecycle_state: 'dormant' as const,
    project_identifier: 'auth-portal',
    repository: 'identity-service',
    description: 'Enterprise identity gateway with SAML and OAuth providers',
  },
  {
    id: 'ctx-legacy-pay',
    name: 'Legacy Payment Gateway',
    created_at: '2026-06-01T09:00:00Z',
    updated_at: '2026-08-01T09:00:00Z',
    lifecycle_state: 'archived' as const,
    project_identifier: 'legacy-pay',
    repository: 'v1-payment-api',
    description: 'Retired payment webhook receiver from legacy platform',
  },
];

describe('Real-World Context Search & Discovery Tests (Phase 1 Contexts)', () => {
  beforeEach(async () => {
    vi.clearAllMocks();

    // Seed Phase 1 workspace store
    useWorkspaceStore.setState({
      workspaces: realWorkspaces as any,
      activeWorkspaceId: 'ctx-payx-dev',
    });

    // Reset launcher store
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

    // Setup Phase 1 Authoritative Engine Mocks
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValue({
      context_id: 'ctx-payx-dev',
      context: 'PayX Development',
      state: 'active',
      task: 'Implement stripe webhook signature verification',
      last_meaningful_activity: 'Verified Stripe webhook HMAC-SHA256 signature in test environment',
      last_active_at: '2026-10-01T02:00:00Z',
      recent_items: [{ id: 'item-stripe-webhook' }, { id: 'item-billing-routes' }],
      context_confidence: 'high',
      memory_statement: 'Working on Stripe webhook signature verification for PayX billing',
    });

    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValue([
      {
        context_id: 'ctx-contextsplit-engine',
        context_name: 'ContextSplit Engine',
        state: 'recent',
        last_active_at: '2026-09-30T22:15:00Z',
        task_title: 'Launcher search ranking and discovery optimization',
        duration_estimate: 'Active 4h ago',
        item_count: 5,
      },
      {
        context_id: 'ctx-univ-research',
        context_name: 'University Research',
        state: 'recent',
        last_active_at: '2026-09-30T18:30:00Z',
        task_title: 'Evaluate Raft consensus latency under network partition',
        duration_estimate: 'Active 8h ago',
        item_count: 8,
      },
      {
        context_id: 'ctx-pay-security',
        context_name: 'Payment Security Audit',
        state: 'paused',
        last_active_at: '2026-09-29T14:00:00Z',
        task_title: 'PCI-DSS compliance review for token storage',
        duration_estimate: 'Paused yesterday',
        item_count: 4,
      },
      {
        context_id: 'ctx-auth0-sso',
        context_name: 'Auth0 SSO Integration',
        state: 'dormant',
        last_active_at: '2026-09-15T11:00:00Z',
        task_title: 'Migrate legacy LDAP authentication to Auth0 SAML 2.0',
        duration_estimate: 'Dormant 16d ago',
        item_count: 2,
      },
      {
        context_id: 'ctx-legacy-pay',
        context_name: 'Legacy Payment Gateway',
        state: 'archived',
        last_active_at: '2026-08-01T09:00:00Z',
        task_title: 'Decommission deprecated PayPal v1 IPN endpoints',
        duration_estimate: 'Archived 2mo ago',
        item_count: 1,
      },
    ]);

    vi.mocked(contextInteractionService.getResumeReadiness).mockImplementation(async (id: string) => {
      if (id === 'ctx-payx-dev') {
        return {
          context_id: id,
          readiness: { readiness: 'ready', formatted_explanation: 'Active webhook workset in progress' },
          formatted_resume_summary: 'Continue Stripe webhook signature verification',
        } as any;
      }
      if (id === 'ctx-auth0-sso') {
        return {
          context_id: id,
          readiness: {
            readiness: 'partial',
            formatted_explanation: 'Context is dormant. IdP certificates may need re-validation.',
            reasons: ['Inactive for >14 days'],
          },
          formatted_resume_summary: 'Review SAML configuration and renew credentials',
        } as any;
      }
      if (id === 'ctx-legacy-pay') {
        return {
          context_id: id,
          readiness: {
            readiness: 'needs_review',
            formatted_explanation: 'Context is archived. Resume requires un-archiving.',
            reasons: ['Archived repository'],
          },
          formatted_resume_summary: 'Un-archive to review PayPal v1 historical routes',
        } as any;
      }
      return {
        context_id: id,
        readiness: { readiness: 'ready', formatted_explanation: 'Ready to resume' },
        formatted_resume_summary: 'Continue from previous working set',
      } as any;
    });

    vi.mocked(contextInteractionService.getContextState).mockImplementation(async (id: string) => {
      const match = realWorkspaces.find((w) => w.id === id);
      return {
        context_id: id,
        context_name: match?.name || 'Context',
        state: (match?.lifecycle_state || 'recent') as any,
        task_title: `Task for ${match?.name}`,
        last_meaningful_activity: `Recent activity in ${match?.name}`,
        freshness_display: 'Recently active',
      } as any;
    });

    vi.mocked(contextInteractionService.getContextWorkset).mockImplementation(async (id: string) => {
      const match = realWorkspaces.find((w) => w.id === id);
      return {
        context_id: id,
        context_name: match?.name || 'Context',
        items: [
          {
            id: `item-${id}-primary`,
            title: `${match?.repository || 'source'}/main.ts`,
            item_type: 'code',
            app_name: 'VS Code',
            relevance: 'primary',
            content_snippet: `// Implementation for ${match?.name}`,
          },
          {
            id: `item-${id}-secondary`,
            title: 'task_notes.md',
            item_type: 'note',
            app_name: 'Obsidian',
            relevance: 'secondary',
            content_snippet: `Notes for ${match?.description}`,
          },
        ],
      } as any;
    });

    // Populate launcher store from engine
    await useLauncherStore.getState().fetchLauncherData();
  });

  // 1. Initial Launcher Load: Active + Recent Contexts
  it('1. populates active and recent contexts accurately on initial open with empty search', () => {
    const state = useLauncherStore.getState();

    // Verify Active Context
    expect(state.activeContext).not.toBeNull();
    expect(state.activeContext?.id).toBe('ctx-payx-dev');
    expect(state.activeContext?.name).toBe('PayX Development');
    expect(state.activeContext?.isActive).toBe(true);
    expect(state.activeContext?.projectIdentifier).toBe('payx-core');
    expect(state.activeContext?.repository).toBe('payx-billing-service');

    // Verify Recent Contexts in Authoritative Engine Order
    expect(state.recentContexts).toHaveLength(5);
    expect(state.recentContexts.map((c) => c.name)).toEqual([
      'ContextSplit Engine',
      'University Research',
      'Payment Security Audit',
      'Auth0 SSO Integration',
      'Legacy Payment Gateway',
    ]);
  });

  // 2. Partial Name Search
  it('2. finds and ranks contexts correctly with partial name queries', () => {
    const allContexts = [
      useLauncherStore.getState().activeContext!,
      ...useLauncherStore.getState().recentContexts,
    ];

    // Query: "pay" -> Should match PayX Development, Payment Security Audit, Legacy Payment Gateway
    const payMatches = allContexts.filter((c) => matchContext(c, 'pay'));
    const sortedPay = sortContextMatches(payMatches, 'pay');

    expect(sortedPay.map((c) => c.name)).toEqual([
      'PayX Development', // Prefix match with shorter token ("PayX")
      'Payment Security Audit', // Prefix match ("Payment")
      'Legacy Payment Gateway', // Substring match ("Payment")
    ]);

    // Query: "univ" -> Matches University Research
    const univMatches = allContexts.filter((c) => matchContext(c, 'univ'));
    const sortedUniv = sortContextMatches(univMatches, 'univ');
    expect(sortedUniv).toHaveLength(1);
    expect(sortedUniv[0].name).toBe('University Research');

    // Query: "context" -> Matches ContextSplit Engine
    const contextMatches = allContexts.filter((c) => matchContext(c, 'context'));
    const sortedContext = sortContextMatches(contextMatches, 'context');
    expect(sortedContext[0].name).toBe('ContextSplit Engine');
  });

  // 3. Project Name Search
  it('3. resolves contexts by project names and repositories without needing name matches', () => {
    const allContexts = [
      useLauncherStore.getState().activeContext!,
      ...useLauncherStore.getState().recentContexts,
    ];

    // Search by project identifier "cs-thesis"
    const thesisMatches = allContexts.filter((c) => matchContext(c, 'cs-thesis'));
    expect(thesisMatches).toHaveLength(1);
    expect(thesisMatches[0].name).toBe('University Research');
    expect(thesisMatches[0].projectIdentifier).toBe('cs-thesis');

    // Search by repository "identity-service"
    const identityMatches = allContexts.filter((c) => matchContext(c, 'identity-service'));
    expect(identityMatches).toHaveLength(1);
    expect(identityMatches[0].name).toBe('Auth0 SSO Integration');
    expect(identityMatches[0].repository).toBe('identity-service');

    // Search by project identifier "payx-core"
    const payxCoreMatches = allContexts.filter((c) => matchContext(c, 'payx-core'));
    expect(payxCoreMatches).toHaveLength(1);
    expect(payxCoreMatches[0].name).toBe('PayX Development');
  });

  // 4. Common Keyword Search
  it('4. matches common domain keywords across task titles, descriptions, and metadata', () => {
    const allContexts = [
      useLauncherStore.getState().activeContext!,
      ...useLauncherStore.getState().recentContexts,
    ];

    // Search "webhook" -> Matches PayX Development (task & desc) and Legacy Payment Gateway (desc)
    const webhookMatches = allContexts.filter((c) => matchContext(c, 'webhook'));
    const sortedWebhook = sortContextMatches(webhookMatches, 'webhook');
    expect(sortedWebhook.map((c) => c.name)).toEqual([
      'PayX Development', // Active, most recent
      'Legacy Payment Gateway',
    ]);

    // Search "consensus" -> Matches University Research ("Evaluate Raft consensus...")
    const consensusMatches = allContexts.filter((c) => matchContext(c, 'consensus'));
    expect(consensusMatches).toHaveLength(1);
    expect(consensusMatches[0].name).toBe('University Research');

    // Search "saml" -> Matches Auth0 SSO Integration
    const samlMatches = allContexts.filter((c) => matchContext(c, 'saml'));
    expect(samlMatches).toHaveLength(1);
    expect(samlMatches[0].name).toBe('Auth0 SSO Integration');

    // Search "pci" -> Matches Payment Security Audit
    const pciMatches = allContexts.filter((c) => matchContext(c, 'pci'));
    expect(pciMatches).toHaveLength(1);
    expect(pciMatches[0].name).toBe('Payment Security Audit');
  });

  // 5. Ranking Understandability
  it('5. adheres to deterministic ranking tiers without arbitrary numerical scores or AI', () => {
    const allContexts = [
      useLauncherStore.getState().activeContext!,
      ...useLauncherStore.getState().recentContexts,
    ];

    // Query: "payment"
    // PayX Development -> Tier 4 (metadata "Core payment engine")
    // Payment Security Audit -> Tier 2 (name starts with "Payment")
    // Legacy Payment Gateway -> Tier 3 (name contains "Payment")
    const tierPaySecurity = getContextSearchTier(
      allContexts.find((c) => c.id === 'ctx-pay-security')!,
      'payment'
    );
    const tierLegacyPay = getContextSearchTier(
      allContexts.find((c) => c.id === 'ctx-legacy-pay')!,
      'payment'
    );
    const tierPayxDev = getContextSearchTier(
      allContexts.find((c) => c.id === 'ctx-payx-dev')!,
      'payment'
    );

    expect(tierPaySecurity).toBe(2); // Starts with "Payment"
    expect(tierLegacyPay).toBe(3); // Contains "Payment"
    expect(tierPayxDev).toBe(4); // Description match

    const sorted = sortContextMatches(
      [
        allContexts.find((c) => c.id === 'ctx-legacy-pay')!,
        allContexts.find((c) => c.id === 'ctx-payx-dev')!,
        allContexts.find((c) => c.id === 'ctx-pay-security')!,
      ],
      'payment'
    );

    // Strict predictable tier ordering: Tier 2 before Tier 3 before Tier 4
    expect(sorted[0].id).toBe('ctx-pay-security');
    expect(sorted[1].id).toBe('ctx-legacy-pay');
    expect(sorted[2].id).toBe('ctx-payx-dev');
  });

  // 6. Recently Used Contexts & Recency Tie-Breaking
  it('6. uses recency as a deterministic tie-breaker for contexts in the same ranking tier', () => {
    // Both ContextSplit Engine and University Research have state 'recent'
    // ContextSplit Engine was active at 22:15, University Research was active at 18:30
    const ctxEngine = useLauncherStore.getState().recentContexts.find((c) => c.id === 'ctx-contextsplit-engine')!;
    const ctxUniv = useLauncherStore.getState().recentContexts.find((c) => c.id === 'ctx-univ-research')!;

    // Both match query "e" at Tier 3 (both names contain "e")
    const tierEngine = getContextSearchTier(ctxEngine, 'e');
    const tierUniv = getContextSearchTier(ctxUniv, 'e');
    expect(tierEngine).toBe(3);
    expect(tierUniv).toBe(3);

    // Recency tie-breaker should place ContextSplit Engine (22:15) before University Research (18:30)
    const sorted = sortContextMatches([ctxUniv, ctxEngine], 'e');
    expect(sorted[0].id).toBe('ctx-contextsplit-engine');
    expect(sorted[1].id).toBe('ctx-univ-research');
  });

  // 7. Inactive and Archived Contexts
  it('7. surfaces dormant and archived contexts correctly with appropriate lifecycle state badges', () => {
    const allContexts = [
      useLauncherStore.getState().activeContext!,
      ...useLauncherStore.getState().recentContexts,
    ];

    // Filter by archived
    const archivedOnly = allContexts.filter((c) => matchesContextFilter(c, 'archived'));
    expect(archivedOnly).toHaveLength(1);
    expect(archivedOnly[0].name).toBe('Legacy Payment Gateway');
    expect(archivedOnly[0].state).toBe('archived');

    // Filter by paused (includes paused and dormant contexts per engine contract)
    const pausedOnly = allContexts.filter((c) => matchesContextFilter(c, 'paused'));
    expect(pausedOnly).toHaveLength(2);
    expect(pausedOnly.map((c) => c.name)).toEqual([
      'Payment Security Audit',
      'Auth0 SSO Integration',
    ]);
    expect(pausedOnly.find((c) => c.name === 'Payment Security Audit')?.state).toBe('paused');
    expect(pausedOnly.find((c) => c.name === 'Auth0 SSO Integration')?.state).toBe('dormant');

    // Search query within archived contexts
    const searchArchived = allContexts
      .filter((c) => matchesContextFilter(c, 'archived'))
      .filter((c) => matchContext(c, 'paypal'));
    expect(searchArchived).toHaveLength(1);
    expect(searchArchived[0].id).toBe('ctx-legacy-pay');
  });

  // 8. Context Selection & Preview Loading
  it('8. loads comprehensive context preview when a real context is selected', async () => {
    // Select University Research
    useLauncherStore.getState().selectContextById('ctx-univ-research');

    // Allow debounce timer to settle
    await new Promise((resolve) => setTimeout(resolve, 60));

    const preview = useLauncherStore.getState().activePreview;
    expect(preview).not.toBeNull();
    expect(preview?.contextId).toBe('ctx-univ-research');
    expect(preview?.name).toBe('University Research');
    expect(preview?.projectIdentifier).toBe('cs-thesis');
    expect(preview?.repository).toBe('distributed-systems-research');
    expect(preview?.readinessBand).toBe('ready');
    expect(preview?.worksetItems).toHaveLength(2);
    expect(preview?.importantItems).toHaveLength(1);
    expect(preview?.importantItems[0].title).toBe('distributed-systems-research/main.ts');
  });

  // 9. Preview for Inactive/Dormant Contexts Shows Freshness & Readiness
  it('9. preview for dormant contexts surfaces readiness warnings accurately', async () => {
    useLauncherStore.getState().selectContextById('ctx-auth0-sso');

    await new Promise((resolve) => setTimeout(resolve, 60));

    const preview = useLauncherStore.getState().activePreview;
    expect(preview).not.toBeNull();
    expect(preview?.contextId).toBe('ctx-auth0-sso');
    expect(preview?.readinessBand).toBe('partial');
    expect(preview?.isStale).toBe(true);
    expect(preview?.readinessExplanation).toContain('IdP certificates');
  });

  // 10. Resume Action Execution
  it('10. resumes selected context, invokes backend signal, and updates active workspace', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ success: true });

    // Setup active preview for University Research
    useLauncherStore.setState({
      isOpen: true,
      selectedContextId: 'ctx-univ-research',
      activePreview: {
        contextId: 'ctx-univ-research',
        name: 'University Research',
        state: 'recent',
        readinessBand: 'ready',
        reasons: [],
        importantItems: [],
        worksetItems: [],
        itemCount: 2,
        isAmbiguous: false,
        isStale: false,
      },
    });

    const success = await useLauncherStore.getState().resumeContextById('ctx-univ-research');

    expect(success).toBe(true);
    expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-univ-research');
    expect(useWorkspaceStore.getState().activeWorkspaceId).toBe('ctx-univ-research');
    expect(useLauncherStore.getState().isOpen).toBe(false);
  });

  // 11. Clearing Search Restores Normal Launcher State
  it('11. restores active and recent contexts when search query is cleared', () => {
    const store = useLauncherStore.getState();

    // 1. Enter query "pay"
    store.setSearchQuery('pay');
    expect(useLauncherStore.getState().searchQuery).toBe('pay');
    expect(useLauncherStore.getState().selectedIndex).toBe(0);

    // 2. Select second result
    store.setSelectedIndex(1);
    expect(useLauncherStore.getState().selectedIndex).toBe(1);

    // 3. Clear query
    store.setSearchQuery('');
    const restoredState = useLauncherStore.getState();

    expect(restoredState.searchQuery).toBe('');
    expect(restoredState.selectedIndex).toBe(0);
    expect(restoredState.activeContext?.id).toBe('ctx-payx-dev');
    expect(restoredState.recentContexts).toHaveLength(5);
    expect(restoredState.recentContexts[0].id).toBe('ctx-contextsplit-engine');
  });

  // 12. Generalization: No Hardcoded Special-Case Rules
  it('12. ensures all real-world contexts follow identical generalized rules without special-casing', () => {
    // Check that each context item can be queried by arbitrary substrings without special logic
    for (const ws of realWorkspaces) {
      const item: LauncherContextItem = {
        id: ws.id,
        name: ws.name,
        description: ws.description,
        projectIdentifier: ws.project_identifier,
        repository: ws.repository,
        state: ws.lifecycle_state,
        isActive: ws.id === 'ctx-payx-dev',
      };

      // Query by first word of name
      const firstWord = ws.name.split(/\s+/)[0];
      expect(matchContext(item, firstWord)).toBe(true);

      // Query by project ID
      expect(matchContext(item, ws.project_identifier)).toBe(true);

      // Tier check is always within valid range 1..4 for matching terms
      const tier = getContextSearchTier(item, firstWord);
      expect([1, 2, 3, 4]).toContain(tier);
    }
  });
});
