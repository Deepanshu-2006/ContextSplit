import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  contextWorksetRetrievalService,
  retrieveContextWorkset,
  retrieveFromRepository,
  retrieveFromEngine,
  WorksetRetrievalLogMeta,
} from './contextWorksetRetrieval';
import { db, Workspace, Item, ContextMembership } from './db';
import { contextWorksetService, ContextWorkset } from './contextWorkset';

// Mock dependencies
vi.mock('./db', () => ({
  db: {
    getWorkspace: vi.fn(),
    listWorkspaceMemberships: vi.fn(),
    listItems: vi.fn(),
    getItem: vi.fn(),
    getLastActiveTask: vi.fn(),
    getContextMemory: vi.fn(),
    // Spy to ensure AI APIs are never invoked
    analyzeItemContextAI: vi.fn(),
    suggestCandidateNameAI: vi.fn(),
  },
}));

vi.mock('./contextWorkset', () => ({
  contextWorksetService: {
    getContextWorkset: vi.fn(),
  },
}));

describe('Context Workset Retrieval Service', () => {
  const mockContextId = 'ctx-workspace-1';

  const mockActiveWorkspace: Workspace = {
    id: mockContextId,
    name: 'Context Engineering',
    lifecycle_state: 'active',
    created_at: '2026-10-20T10:00:00Z',
    updated_at: '2026-10-22T08:00:00Z',
    last_activity_at: '2026-10-22T08:00:00Z',
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  // =========================================================================
  // 1. Empty Contexts
  // =========================================================================
  describe('1. Empty Contexts', () => {
    it('handles an empty context workspace without phantom items or fabricated tasks', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce([]);

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.contextId).toBe(mockContextId);
      expect(workset?.contextName).toBe('Context Engineering');
      expect(workset?.totalItemCount).toBe(0);
      expect(workset?.primaryItems).toEqual([]);
      expect(workset?.supportingItems).toEqual([]);
      expect(workset?.referenceItems).toEqual([]);
      expect(workset?.suggestedAdditions).toEqual([]);
      expect(workset?.worksetState).toBe('insufficient_evidence');
      expect(workset?.readinessBand).toBe('low');
      expect(workset?.readinessExplanation).toContain('Context workspace is empty');
      expect(workset?.activeTaskId).toBeNull();
      expect(workset?.activeTaskTitle).toBeNull();
    });
  });

  // =========================================================================
  // 2. Multiple Memberships & Multi-Context Items
  // =========================================================================
  describe('2. Multiple Memberships & Multi-Context Items', () => {
    it('supports items physically placed in another workspace while contextually confirmed in target context', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);

      // Context B has a confirmed membership for item-cross physically residing in ctx-other
      const crossMembership: ContextMembership = {
        id: 'mem-cross-1',
        item_id: 'item-cross',
        workspace_id: mockContextId,
        membership_type: 'confirmed',
        confidence: 0.95,
        source: 'user',
        created_at: '2026-10-21T12:00:00Z',
        updated_at: '2026-10-21T12:00:00Z',
      };

      const crossItemRecord: Item = {
        id: 'item-cross',
        workspace_id: 'ctx-other', // physically in ctx-other
        session_id: 'session-1',
        item_type: 'file',
        content: '/docs/architecture.pdf',
        title: 'System Architecture Specification',
        metadata: JSON.stringify({ isImportant: true }),
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([crossMembership]);
      vi.mocked(db.listItems).mockResolvedValueOnce([]); // not physically in target workspace
      vi.mocked(db.getItem).mockResolvedValueOnce(crossItemRecord);

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.totalItemCount).toBe(1);

      const item = workset?.items[0];
      expect(item?.id).toBe('item-cross');
      expect(item?.contextId).toBe(mockContextId);
      expect(item?.workspaceId).toBe('ctx-other');
      expect(item?.physicalLocation).toBe('workspace');
      expect(item?.provenance).toBe('confirmed');
      expect(item?.isSuggestedAddition).toBe(false);
      expect(item?.isImportant).toBe(true);
    });
  });

  // =========================================================================
  // 3. Duplicates Handling
  // =========================================================================
  describe('3. Duplicates Handling', () => {
    it('deduplicates items when both physical item and membership point to the same record', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);

      const physicalItem: Item = {
        id: 'item-dup-1',
        workspace_id: mockContextId,
        session_id: 'session-1',
        item_type: 'file',
        content: 'src/main.rs',
        title: 'Rust Core Engine',
        metadata: JSON.stringify({ isImportant: true }),
        created_at: '2026-10-21T10:00:00Z',
        updated_at: '2026-10-22T09:00:00Z',
      };

      const duplicateMembership: ContextMembership = {
        id: 'mem-dup-1',
        item_id: 'item-dup-1',
        workspace_id: mockContextId,
        membership_type: 'confirmed',
        confidence: 0.99,
        source: 'user',
        created_at: '2026-10-21T10:00:00Z',
        updated_at: '2026-10-22T09:00:00Z',
      };

      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([duplicateMembership]);
      vi.mocked(db.listItems).mockResolvedValueOnce([physicalItem]);

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.totalItemCount).toBe(1);
      expect(workset?.items.length).toBe(1);
      expect(workset?.items[0].id).toBe('item-dup-1');
      expect(workset?.items[0].confidence).toBe(0.99);
    });

    it('upgrades inferred suggested item to confirmed if user confirmation exists', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);

      // Physical item present in workspace (confirmed)
      const physicalItem: Item = {
        id: 'item-conflict',
        workspace_id: mockContextId,
        session_id: 'session-1',
        item_type: 'url',
        content: 'https://github.com/org/repo',
        title: 'Project Repository',
        metadata: JSON.stringify({ isImportant: true }),
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      // Stale suggested membership also exists in memberships table
      const staleSuggestedMembership: ContextMembership = {
        id: 'mem-suggested-conflict',
        item_id: 'item-conflict',
        workspace_id: mockContextId,
        membership_type: 'suggested',
        confidence: 0.70,
        source: 'context_engine',
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-20T10:00:00Z',
      };

      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([staleSuggestedMembership]);
      vi.mocked(db.listItems).mockResolvedValueOnce([physicalItem]);

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.totalItemCount).toBe(1);
      expect(workset?.items[0].provenance).toBe('confirmed');
      expect(workset?.items[0].isSuggestedAddition).toBe(false);
      expect(workset?.suggestedAdditions).toEqual([]);
    });
  });

  // =========================================================================
  // 4. Unavailable, Missing, Deleted, Archived Records
  // =========================================================================
  describe('4. Unavailable, Missing, Deleted, and Archived Records', () => {
    it('returns null safely when workspace does not exist', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(null as any);

      const workset = await retrieveFromRepository('non-existent-ctx');
      expect(workset).toBeNull();
    });

    it('returns null safely when contextId is invalid', async () => {
      const workset = await retrieveFromRepository('   ');
      expect(workset).toBeNull();
      expect(db.getWorkspace).not.toHaveBeenCalled();
    });

    it('predictably handles archived contexts by returning a dormant blocked workset', async () => {
      const archivedWorkspace: Workspace = {
        id: 'ctx-archived',
        name: 'Legacy Migration',
        lifecycle_state: 'archived',
        created_at: '2026-08-01T10:00:00Z',
        updated_at: '2026-09-01T10:00:00Z',
        archived_at: '2026-09-01T10:00:00Z',
      };

      vi.mocked(db.getWorkspace).mockResolvedValueOnce(archivedWorkspace);

      const workset = await retrieveFromRepository('ctx-archived');

      expect(workset).not.toBeNull();
      expect(workset?.contextId).toBe('ctx-archived');
      expect(workset?.lifecycleState).toBe('archived');
      expect(workset?.worksetState).toBe('dormant');
      expect(workset?.readinessBand).toBe('blocked');
      expect(workset?.readinessExplanation).toContain('archived and must be unarchived');
      expect(workset?.totalItemCount).toBe(0);
      // DB items should not even be queried for archived contexts
      expect(db.listItems).not.toHaveBeenCalled();
    });

    it('handles missing item records referenced in context_memberships safely without throwing', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);

      const danglingMembership: ContextMembership = {
        id: 'mem-dangling',
        item_id: 'item-dangling-ghost',
        workspace_id: mockContextId,
        membership_type: 'confirmed',
        confidence: 0.90,
        source: 'user',
        created_at: '2026-10-21T10:00:00Z',
        updated_at: '2026-10-21T10:00:00Z',
      };

      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([danglingMembership]);
      vi.mocked(db.listItems).mockResolvedValueOnce([]);
      // db.getItem fails or throws for ghost item
      vi.mocked(db.getItem).mockRejectedValueOnce(new Error('Item not found'));

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.totalItemCount).toBe(1);
      const ghost = workset?.items[0];
      expect(ghost?.id).toBe('item-dangling-ghost');
      expect(ghost?.status).toBe('missing');
      expect(ghost?.title).toBe('[Unavailable Item]');
      // Missing items must not occupy primary focus
      expect(workset?.primaryItems).toEqual([]);
    });

    it('handles soft-deleted items properly and excludes them from primary items', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);

      const deletedItem: Item = {
        id: 'item-deleted',
        workspace_id: mockContextId,
        session_id: 'session-1',
        item_type: 'text',
        content: 'Old scratchpad note',
        title: 'Old Notes',
        metadata: JSON.stringify({ is_deleted: true, isImportant: true }),
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-21T10:00:00Z',
      };

      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce([deletedItem]);

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      const item = workset?.items[0];
      expect(item?.status).toBe('deleted');
      expect(item?.title).toBe('[Deleted Item]');
      expect(workset?.primaryItems).toEqual([]);
    });
  });

  // =========================================================================
  // 5. Ordering & Deterministic Tie-Breaking
  // =========================================================================
  describe('5. Ordering & Deterministic Tie-Breaking', () => {
    it('applies strict deterministic tie-breaking: confirmed > inferred, tier, confidence, recency, id ASC', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);

      // Two items with identical score, confidence, tier, and timestamp, differing only by ID
      const itemAlpha: Item = {
        id: 'item-alpha',
        workspace_id: mockContextId,
        session_id: 'session-1',
        item_type: 'text',
        content: 'Alpha spec',
        title: 'Alpha Spec',
        metadata: JSON.stringify({ isImportant: true }),
        created_at: '2026-10-22T08:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      const itemBeta: Item = {
        id: 'item-beta',
        workspace_id: mockContextId,
        session_id: 'session-1',
        item_type: 'text',
        content: 'Beta spec',
        title: 'Beta Spec',
        metadata: JSON.stringify({ isImportant: true }),
        created_at: '2026-10-22T08:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      // Provided in reverse order [itemBeta, itemAlpha]
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce([itemBeta, itemAlpha]);

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.items.length).toBe(2);
      // 'item-alpha' must come before 'item-beta' via id localeCompare tie-breaker
      expect(workset?.items[0].id).toBe('item-alpha');
      expect(workset?.items[1].id).toBe('item-beta');
    });

    it('ensures confirmed context items strictly outrank inferred suggestions even if score is equal', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);

      const confirmedItem: Item = {
        id: 'item-confirmed',
        workspace_id: mockContextId,
        session_id: 'session-1',
        item_type: 'text',
        content: 'Confirmed spec',
        title: 'Confirmed Spec',
        metadata: JSON.stringify({ isImportant: true }),
        created_at: '2026-10-22T08:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      const suggestedMembership: ContextMembership = {
        id: 'mem-suggested',
        item_id: 'item-suggested',
        workspace_id: mockContextId,
        membership_type: 'suggested',
        confidence: 0.95,
        source: 'context_engine',
        created_at: '2026-10-22T08:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      const suggestedItem: Item = {
        id: 'item-suggested',
        workspace_id: null, // in inbox
        session_id: 'session-1',
        item_type: 'url',
        content: 'https://example.com',
        title: 'Suggested Link',
        metadata: JSON.stringify({}),
        created_at: '2026-10-22T08:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([suggestedMembership]);
      vi.mocked(db.listItems).mockResolvedValueOnce([confirmedItem]);
      vi.mocked(db.getItem).mockResolvedValueOnce(suggestedItem);

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.items[0].id).toBe('item-confirmed');
      expect(workset?.items[0].provenance).toBe('confirmed');
      expect(workset?.items[1].id).toBe('item-suggested');
      expect(workset?.items[1].provenance).toBe('inferred');
    });
  });

  // =========================================================================
  // 6. Mixed Confidence & Inferred Suggestion Filtering
  // =========================================================================
  describe('6. Mixed Confidence & Inferred Suggestion Filtering', () => {
    it('admits high-confidence suggestions (>=0.60) into suggestedAdditions and rejects low-confidence noise', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);

      const highConfMembership: ContextMembership = {
        id: 'mem-high',
        item_id: 'item-high',
        workspace_id: mockContextId,
        membership_type: 'suggested',
        confidence: 0.82,
        source: 'context_engine',
        created_at: '2026-10-22T08:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      const lowConfMembership: ContextMembership = {
        id: 'mem-low',
        item_id: 'item-low',
        workspace_id: mockContextId,
        membership_type: 'suggested',
        confidence: 0.42, // below default 0.60 threshold
        source: 'context_engine',
        created_at: '2026-10-22T08:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      const highItem: Item = {
        id: 'item-high',
        workspace_id: null,
        session_id: 'session-1',
        item_type: 'url',
        content: 'https://developer.mozilla.org',
        title: 'MDN Web Docs',
        metadata: '{}',
        created_at: '2026-10-22T08:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([highConfMembership, lowConfMembership]);
      vi.mocked(db.listItems).mockResolvedValueOnce([]);
      vi.mocked(db.getItem).mockResolvedValueOnce(highItem);

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.totalItemCount).toBe(1);
      expect(workset?.suggestedAdditions.length).toBe(1);
      expect(workset?.suggestedAdditions[0].id).toBe('item-high');
      expect(workset?.suggestedAdditions[0].confidence).toBe(0.82);
      // Low confidence item should never have been queried via getItem
      expect(db.getItem).not.toHaveBeenCalledWith('item-low');
    });
  });

  // =========================================================================
  // 7. Workspace Clutter Exclusion (Anti-Accumulation)
  // =========================================================================
  describe('7. Workspace Clutter Exclusion', () => {
    it('excludes stale unrelated items simply because they share the workspace directory', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);

      const pinnedItem: Item = {
        id: 'item-pinned',
        workspace_id: mockContextId,
        session_id: 'session-1',
        item_type: 'text',
        content: 'Critical reference notes',
        title: 'Pinned Note',
        metadata: JSON.stringify({ isImportant: true }),
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-22T08:00:00Z',
      };

      const staleClutterItem: Item = {
        id: 'item-clutter',
        workspace_id: mockContextId,
        session_id: null,
        item_type: 'text',
        content: 'Random grocery list saved long ago',
        title: 'Groceries',
        metadata: JSON.stringify({ isImportant: false }),
        created_at: '2026-05-01T10:00:00Z',
        updated_at: '2026-05-01T10:00:00Z',
      };

      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce([pinnedItem, staleClutterItem]);

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.totalItemCount).toBe(1);
      expect(workset?.items[0].id).toBe('item-pinned');
      expect(workset?.items.some((i) => i.id === 'item-clutter')).toBe(false);
    });
  });

  // =========================================================================
  // 8. Zero AI Invocations & Optional AI Enrichment Isolation
  // =========================================================================
  describe('8. Zero AI Invocations', () => {
    it('generates the workset without invoking any AI services and keeps aiEnrichment null', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce([]);

      const workset = await retrieveFromRepository(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.aiEnrichment).toBeNull();
      expect(db.analyzeItemContextAI).not.toHaveBeenCalled();
      expect(db.suggestCandidateNameAI).not.toHaveBeenCalled();
    });
  });

  // =========================================================================
  // 9. Privacy-Safe Telemetry (Requirement 13)
  // =========================================================================
  describe('9. Privacy-Safe Telemetry', () => {
    it('invokes custom logger with structured counts and IDs without leaking item content or titles', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce([]);

      let loggedMeta: WorksetRetrievalLogMeta | null = null;
      const customLogger = (meta: WorksetRetrievalLogMeta) => {
        loggedMeta = meta;
      };

      await retrieveFromRepository(mockContextId, { logger: customLogger });

      expect(loggedMeta).not.toBeNull();
      const meta = loggedMeta as unknown as WorksetRetrievalLogMeta;
      expect(meta.contextId).toBe(mockContextId);
      expect(meta.source).toBe('repository');
      expect(meta.status).toBe('success');
      expect(typeof meta.durationMs).toBe('number');
      expect(meta.totalItems).toBe(0);

      // Verify no content or title keys exist on the metadata object
      const metaKeys = Object.keys(meta || {});
      expect(metaKeys).not.toContain('title');
      expect(metaKeys).not.toContain('content');
      expect(metaKeys).not.toContain('snippet');
    });
  });

  // =========================================================================
  // 10. Engine Delegation & Graceful Fallback
  // =========================================================================
  describe('10. Engine Delegation & Fallback', () => {
    it('retrieves from engine when engine succeeds', async () => {
      const mockEngineWorkset: ContextWorkset = {
        context_id: mockContextId,
        context_name: 'Context Engineering',
        task_id: 'task-1',
        task_title: 'Implement Workset',
        generated_at: '2026-10-22T08:00:00Z',
        last_active_at: '2026-10-22T08:00:00Z',
        workset_state: 'active',
        items: [
          {
            id: 'engine-item-1',
            title: 'Core Engine Spec',
            item_type: 'file',
            relevance: 'primary',
            relevance_score: 0.95,
            reasons: ['Direct task focus'],
            provenance: 'confirmed',
            created_at: '2026-10-22T08:00:00Z',
            last_active_at: '2026-10-22T08:00:00Z',
            app_name: 'VS Code',
            domain: null,
            content_snippet: 'Specification content',
            is_suggested_addition: false,
          },
        ],
        recent_activity: 'Active session',
        relevant_items: [],
        important_references: [],
        related_tasks: [],
        confidence: 0.95,
        confidence_band: 'high',
        provenance: 'confirmed',
        explanation: {
          summary: 'Active task focus',
          why_reasons: ['Direct task focus'],
          supporting_evidence: [],
          confidence_band: 'high',
          provenance: 'confirmed',
        },
      };

      vi.mocked(contextWorksetService.getContextWorkset).mockResolvedValueOnce(mockEngineWorkset);

      const workset = await retrieveContextWorkset(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.contextId).toBe(mockContextId);
      expect(workset?.primaryItems.length).toBe(1);
      expect(workset?.primaryItems[0].id).toBe('engine-item-1');
      expect(workset?.primaryItems[0].title).toBe('Core Engine Spec');
      expect(contextWorksetService.getContextWorkset).toHaveBeenCalledWith(mockContextId);
    });

    it('gracefully falls back to repository retrieval when engine returns null', async () => {
      vi.mocked(contextWorksetService.getContextWorkset).mockResolvedValueOnce(null);

      // Repository fallback
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce([]);

      const workset = await retrieveContextWorkset(mockContextId);

      expect(workset).not.toBeNull();
      expect(workset?.contextId).toBe(mockContextId);
      expect(db.getWorkspace).toHaveBeenCalledWith(mockContextId);
    });

    it('honors preferRepository option by querying repository directly', async () => {
      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockActiveWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce([]);

      const workset = await retrieveContextWorkset(mockContextId, { preferRepository: true });

      expect(workset).not.toBeNull();
      expect(contextWorksetService.getContextWorkset).not.toHaveBeenCalled();
      expect(db.getWorkspace).toHaveBeenCalledWith(mockContextId);
    });

    it('exposes retrieveFromEngine and contextWorksetRetrievalService facade methods', async () => {
      vi.mocked(contextWorksetService.getContextWorkset).mockResolvedValueOnce(null);
      const res = await retrieveFromEngine('invalid-ctx-not-found');
      expect(res).toBeNull();

      expect(contextWorksetRetrievalService.retrieveContextWorkset).toBe(retrieveContextWorkset);
      expect(contextWorksetRetrievalService.retrieveFromRepository).toBe(retrieveFromRepository);
      expect(contextWorksetRetrievalService.retrieveFromEngine).toBe(retrieveFromEngine);
    });
  });
});
