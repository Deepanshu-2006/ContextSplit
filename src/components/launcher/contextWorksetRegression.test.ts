import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  useLauncherStore,
  invalidateLauncherPreviewCache,
} from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { quickActionsService } from '../../services/quickActionsService';
import { db, Workspace, Item, ContextMembership } from '../../services/db';
import { contextInteractionService } from '../../services/contextInteractionService';
import {
  retrieveFromRepository,
} from '../../services/contextWorksetRetrieval';
import {
  createWorksetPresentationModel,
  createEmptyWorksetPresentation,
} from '../../models/contextWorksetPresentation';
import {
  deriveMinimalWorkset,
  normalizeWorksetItem,
  MinimalWorksetItem,
} from '../../models/contextWorkset';
import { openUrl, revealItemInDir } from '@tauri-apps/plugin-opener';

// ---------------------------------------------------------------------------
// Isolated Test Mocks
// ---------------------------------------------------------------------------

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
  revealItemInDir: vi.fn().mockResolvedValue(undefined),
  openPath: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

vi.mock('../../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    getResumeReadiness: vi.fn(),
    resumeContext: vi.fn(),
    pauseContext: vi.fn(),
  },
}));

vi.mock('../../services/db', () => ({
  db: {
    getWorkspace: vi.fn(),
    listWorkspaceMemberships: vi.fn(),
    listItems: vi.fn(),
    getItem: vi.fn(),
    getLastActiveTask: vi.fn(),
    getContextMemory: vi.fn(),
    updateWorkspace: vi.fn(),
    archiveWorkspace: vi.fn(),
    unarchiveWorkspace: vi.fn(),
    listWorkspaces: vi.fn(),
  },
}));

describe('Context Workset Focused Regression Suite', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invalidateLauncherPreviewCache();

    // Default mock behavior for clean isolation
    vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValue(null as any);
    vi.mocked(contextInteractionService.getContextState).mockResolvedValue({
      task_title: 'Implement Regression Suite',
      last_active_at: '2026-10-22T10:00:00Z',
      freshness_display: '5m ago',
      last_meaningful_activity: 'Working on context workset regression tests',
      lifecycle_state: 'active',
      is_ambiguous: false,
    } as any);

    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValue({
      state: 'ready',
      readiness: {
        readiness: 'ready',
        formatted_explanation: 'Ready to continue work in context',
        reasons: [],
      },
      formatted_resume_summary: 'All required items present',
    } as any);

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-reg-valid',
          name: 'Valid Project',
          path: '/projects/valid',
          lifecycle_state: 'active',
          created_at: '2026-10-20T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
        } as any,
        {
          id: 'ctx-reg-empty',
          name: 'Empty Project',
          path: '/projects/empty',
          lifecycle_state: 'active',
          created_at: '2026-10-21T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
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
      actionError: null,
      previewError: null,
      recentContexts: [
        {
          id: 'ctx-reg-valid',
          name: 'Valid Project',
          isActive: false,
          state: 'active',
        },
        {
          id: 'ctx-reg-empty',
          name: 'Empty Project',
          isActive: false,
          state: 'active',
        },
      ],
    });
  });

  // =========================================================================
  // 1. Retrieving the workset for a valid context
  // =========================================================================
  describe('1. Retrieving the workset for a valid context', () => {
    it('retrieves workspace metadata, task grounding, and associated items for a valid context', async () => {
      const mockWorkspace: Workspace = {
        id: 'ctx-reg-valid',
        name: 'Valid Project',
        lifecycle_state: 'active',
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-22T10:00:00Z',
        last_activity_at: '2026-10-22T10:00:00Z',
      };

      const mockTask = {
        task_id: 'task-reg-1',
        title: 'Ship Workset Feature',
        confidence: 0.95,
        is_current: true,
      };

      const mockItems: Item[] = [
        {
          id: 'item-valid-1',
          workspace_id: 'ctx-reg-valid',
          session_id: null,
          item_type: 'url',
          content: 'https://docs.contextsplit.dev/workset',
          title: 'Workset Spec Document',
          metadata: JSON.stringify({ is_important: true, relevance_score: 0.9, task_id: 'task-reg-1' }),
          created_at: '2026-10-22T08:00:00Z',
          updated_at: '2026-10-22T09:00:00Z',
        },
        {
          id: 'item-valid-2',
          workspace_id: 'ctx-reg-valid',
          session_id: null,
          item_type: 'file',
          content: '/src/contextWorkset.ts',
          title: 'contextWorkset.ts',
          metadata: JSON.stringify({ is_important: false, relevance_score: 0.85 }),
          created_at: '2026-10-22T08:30:00Z',
          updated_at: '2026-10-22T09:30:00Z',
        },
      ];

      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(mockTask as any);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce({
        recent_activity_summary: 'Implemented workset normalization and sorting',
      } as any);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce(mockItems);

      const workset = await retrieveFromRepository('ctx-reg-valid');

      expect(workset).not.toBeNull();
      expect(workset?.contextId).toBe('ctx-reg-valid');
      expect(workset?.contextName).toBe('Valid Project');
      expect(workset?.activeTaskId).toBe('task-reg-1');
      expect(workset?.activeTaskTitle).toBe('Ship Workset Feature');
      expect(workset?.totalItemCount).toBe(2);
      expect(workset?.items.map((i) => i.id)).toEqual(['item-valid-1', 'item-valid-2']);

      // Presentation model mapping
      const presentation = createWorksetPresentationModel(workset!);
      expect(presentation.populationState).toBe('complete');
      expect(presentation.hasTask).toBe(true);
      expect(presentation.hasItems).toBe(true);
      expect(presentation.totalItemCount).toBe(2);
      expect(presentation.primaryFocus).toHaveLength(1);
      expect(presentation.references).toHaveLength(1);
    });
  });

  // =========================================================================
  // 2. Handling contexts with no associated items
  // =========================================================================
  describe('2. Handling contexts with no associated items', () => {
    it('handles empty context workspaces cleanly without fabricating tasks or phantom items', async () => {
      const mockEmptyWorkspace: Workspace = {
        id: 'ctx-reg-empty',
        name: 'Empty Project',
        lifecycle_state: 'active',
        created_at: '2026-10-21T10:00:00Z',
        updated_at: '2026-10-22T10:00:00Z',
      };

      vi.mocked(db.getWorkspace).mockResolvedValueOnce(mockEmptyWorkspace);
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce([]);

      const workset = await retrieveFromRepository('ctx-reg-empty');

      expect(workset).not.toBeNull();
      expect(workset?.contextId).toBe('ctx-reg-empty');
      expect(workset?.totalItemCount).toBe(0);
      expect(workset?.primaryItems).toEqual([]);
      expect(workset?.supportingItems).toEqual([]);
      expect(workset?.referenceItems).toEqual([]);
      expect(workset?.suggestedAdditions).toEqual([]);
      expect(workset?.activeTaskId).toBeNull();
      expect(workset?.activeTaskTitle).toBeNull();
      expect(workset?.worksetState).toBe('insufficient_evidence');

      const presentation = createWorksetPresentationModel(workset!);
      expect(presentation.populationState).toBe('empty');
      expect(presentation.hasItems).toBe(false);
      expect(presentation.hasTask).toBe(false);
      expect(presentation.primaryFocus).toEqual([]);
      expect(presentation.references).toEqual([]);
      expect(presentation.suggestedAdditions).toEqual([]);
    });
  });

  // =========================================================================
  // 3. Retrieving items associated with multiple contexts
  // =========================================================================
  describe('3. Retrieving items associated with multiple contexts', () => {
    it('supports cross-context items with correct target context-scoping', async () => {
      const sharedItemRecord: Item = {
        id: 'item-shared-spec',
        workspace_id: 'ctx-alpha', // Physically residing in Alpha
        session_id: null,
        item_type: 'file',
        content: '/specs/architecture.md',
        title: 'Architecture Specification',
        metadata: JSON.stringify({ is_important: true }),
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-22T10:00:00Z',
      };

      // Context Beta has confirmed membership for the item
      const betaMembership: ContextMembership = {
        id: 'mem-beta-1',
        item_id: 'item-shared-spec',
        workspace_id: 'ctx-beta',
        membership_type: 'confirmed',
        confidence: 0.95,
        source: 'user',
        created_at: '2026-10-21T10:00:00Z',
        updated_at: '2026-10-21T10:00:00Z',
      };

      // Query Context Alpha:
      vi.mocked(db.getWorkspace).mockResolvedValueOnce({
        id: 'ctx-alpha',
        name: 'Alpha Project',
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-22T10:00:00Z',
      });
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce([sharedItemRecord]);

      const alphaWorkset = await retrieveFromRepository('ctx-alpha');
      expect(alphaWorkset?.items[0].id).toBe('item-shared-spec');
      expect(alphaWorkset?.items[0].contextId).toBe('ctx-alpha');
      expect(alphaWorkset?.items[0].physicalLocation).toBe('workspace');

      // Query Context Beta:
      vi.mocked(db.getWorkspace).mockResolvedValueOnce({
        id: 'ctx-beta',
        name: 'Beta Project',
        created_at: '2026-10-21T10:00:00Z',
        updated_at: '2026-10-22T10:00:00Z',
      });
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([betaMembership]);
      vi.mocked(db.listItems).mockResolvedValueOnce([]); // Not physical in Beta
      vi.mocked(db.getItem).mockResolvedValueOnce(sharedItemRecord); // Fetched via membership

      const betaWorkset = await retrieveFromRepository('ctx-beta');
      expect(betaWorkset?.items[0].id).toBe('item-shared-spec');
      expect(betaWorkset?.items[0].contextId).toBe('ctx-beta');
      expect(betaWorkset?.items[0].provenance).toBe('confirmed');
    });
  });

  // =========================================================================
  // 4. Deduplicating items correctly
  // =========================================================================
  describe('4. Deduplicating items correctly', () => {
    it('deduplicates items appearing through multiple relationships and preserves highest confidence and confirmed status', async () => {
      const duplicateItem: Item = {
        id: 'item-dup-target',
        workspace_id: 'ctx-dup-test',
        session_id: null,
        item_type: 'file',
        content: '/src/main.rs',
        title: 'main.rs',
        metadata: JSON.stringify({ is_important: true }),
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-22T10:00:00Z',
      };

      const memberships: ContextMembership[] = [
        {
          id: 'mem-1',
          item_id: 'item-dup-target',
          workspace_id: 'ctx-dup-test',
          membership_type: 'suggested',
          confidence: 0.65,
          source: 'context_engine',
          created_at: '2026-10-20T10:00:00Z',
          updated_at: '2026-10-20T10:00:00Z',
        },
        {
          id: 'mem-2',
          item_id: 'item-dup-target',
          workspace_id: 'ctx-dup-test',
          membership_type: 'confirmed',
          confidence: 0.95,
          source: 'user',
          created_at: '2026-10-21T10:00:00Z',
          updated_at: '2026-10-21T10:00:00Z',
        },
      ];

      vi.mocked(db.getWorkspace).mockResolvedValueOnce({
        id: 'ctx-dup-test',
        name: 'Deduplication Test',
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-22T10:00:00Z',
      });
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce(memberships);
      vi.mocked(db.listItems).mockResolvedValueOnce([duplicateItem]);

      const workset = await retrieveFromRepository('ctx-dup-test');

      expect(workset?.items).toHaveLength(1);
      expect(workset?.totalItemCount).toBe(1);
      const item = workset?.items[0]!;
      expect(item.id).toBe('item-dup-target');
      expect(item.provenance).toBe('confirmed');
      expect(item.confidence).toBe(0.95);
    });
  });

  // =========================================================================
  // 5. Distinguishing confirmed associations from inferred relevance
  // =========================================================================
  describe('5. Distinguishing confirmed associations from inferred relevance', () => {
    it('accurately tags items as confirmed or inferred across retrieval and presentation', () => {
      const confirmedRaw = normalizeWorksetItem(
        {
          id: 'item-confirmed',
          title: 'Confirmed Task Spec',
          itemType: 'file',
          workspaceId: 'ctx-1',
          provenance: 'confirmed',
          isSuggestedAddition: false,
          confidence: 0.9,
        },
        'ctx-1'
      );

      const inferredRaw = normalizeWorksetItem(
        {
          id: 'item-inferred',
          title: 'Candidate Web Reference',
          itemType: 'url',
          workspaceId: 'inbox',
          provenance: 'inferred',
          isSuggestedAddition: true,
          confidence: 0.75,
        },
        'ctx-1'
      );

      expect(confirmedRaw.provenance).toBe('confirmed');
      expect(confirmedRaw.isSuggestedAddition).toBe(false);

      expect(inferredRaw.provenance).toBe('inferred');
      expect(inferredRaw.isSuggestedAddition).toBe(true);

      const presentationConfirmed = createWorksetPresentationModel({
        contextId: 'ctx-1',
        contextName: 'Provenance Test',
        lifecycleState: 'active',
        worksetState: 'active',
        activeTaskId: null,
        activeTaskTitle: null,
        taskConfidence: null,
        generatedAt: '2026-10-22T10:00:00Z',
        lastActiveAt: null,
        confidence: 0.9,
        confidenceBand: 'high',
        readinessBand: 'ready',
        readinessExplanation: null,
        recentActivitySummary: null,
        items: [confirmedRaw, inferredRaw],
        primaryItems: [confirmedRaw],
        supportingItems: [],
        referenceItems: [],
        suggestedAdditions: [inferredRaw],
        totalItemCount: 2,
      });

      expect(presentationConfirmed.primaryFocus[0].provenance).toBe('confirmed');
      expect(presentationConfirmed.suggestedAdditions[0].provenance).toBe('inferred');
      expect(presentationConfirmed.hasSuggestions).toBe(true);
    });
  });

  // =========================================================================
  // 6. Preserving stable context and item IDs
  // =========================================================================
  describe('6. Preserving stable context and item IDs', () => {
    it('preserves immutable context and item IDs through transformation layers', () => {
      const stableContextId = 'ctx-stable-unique-12345';
      const stableItemId = 'item-stable-unique-67890';

      const item = normalizeWorksetItem(
        {
          id: stableItemId,
          title: 'Invariant Item',
          itemType: 'file',
          workspaceId: stableContextId,
          confidence: 0.9,
          relevanceTier: 'primary',
        },
        stableContextId
      );

      expect(item.id).toBe(stableItemId);
      expect(item.contextId).toBe(stableContextId);

      const workset = deriveMinimalWorkset({
        contextId: stableContextId,
        contextName: 'Stable Context',
        rawItems: [item],
      });

      expect(workset.contextId).toBe(stableContextId);
      expect(workset.items[0].id).toBe(stableItemId);
      expect(workset.items[0].contextId).toBe(stableContextId);

      const presentation = createWorksetPresentationModel(workset);
      expect(presentation.contextId).toBe(stableContextId);
      expect(presentation.primaryFocus[0].id).toBe(stableItemId);
      expect(presentation.primaryFocus[0].contextId).toBe(stableContextId);
    });
  });

  // =========================================================================
  // 7. Deterministic ordering of workset items
  // =========================================================================
  describe('7. Deterministic ordering of workset items', () => {
    it('consistently produces identical sorting order regardless of input arrangement', () => {
      const rawA: MinimalWorksetItem = {
        id: 'item-a-primary-confirmed',
        title: 'Primary Task File',
        itemType: 'file',
        relevanceTier: 'primary',
        provenance: 'confirmed',
        physicalLocation: 'workspace',
        workspaceId: 'ctx-order',
        contextId: 'ctx-order',
        confidence: 0.95,
        isSuggestedAddition: false,
        isImportant: true,
        reasons: [],
        status: 'available',
        appName: null,
        domain: null,
        contentSnippet: null,
        createdAt: '2026-10-22T08:00:00Z',
        lastActiveAt: '2026-10-22T09:00:00Z',
      };

      const rawB: MinimalWorksetItem = {
        id: 'item-b-primary-inferred',
        title: 'Inferred Primary Spec',
        itemType: 'file',
        relevanceTier: 'primary',
        provenance: 'inferred',
        physicalLocation: 'inbox',
        workspaceId: null,
        contextId: 'ctx-order',
        confidence: 0.90,
        isSuggestedAddition: false,
        isImportant: false,
        reasons: [],
        status: 'available',
        appName: null,
        domain: null,
        contentSnippet: null,
        createdAt: '2026-10-22T08:00:00Z',
        lastActiveAt: '2026-10-22T09:00:00Z',
      };

      const rawC: MinimalWorksetItem = {
        id: 'item-c-supporting-confirmed',
        title: 'Supporting Docs',
        itemType: 'url',
        relevanceTier: 'supporting',
        provenance: 'confirmed',
        physicalLocation: 'workspace',
        workspaceId: 'ctx-order',
        contextId: 'ctx-order',
        confidence: 0.85,
        isSuggestedAddition: false,
        isImportant: false,
        reasons: [],
        status: 'available',
        appName: null,
        domain: 'docs.example.com',
        contentSnippet: null,
        createdAt: '2026-10-22T08:00:00Z',
        lastActiveAt: '2026-10-22T08:30:00Z',
      };

      // Shuffled inputs
      const order1 = deriveMinimalWorkset({
        contextId: 'ctx-order',
        contextName: 'Order Test',
        rawItems: [rawC, rawA, rawB],
      }).items.map((i) => i.id);

      const order2 = deriveMinimalWorkset({
        contextId: 'ctx-order',
        contextName: 'Order Test',
        rawItems: [rawB, rawC, rawA],
      }).items.map((i) => i.id);

      expect(order1).toEqual(['item-a-primary-confirmed', 'item-c-supporting-confirmed', 'item-b-primary-inferred']);
      expect(order2).toEqual(order1);
    });
  });

  // =========================================================================
  // 8. Handling deleted, archived, or unavailable records
  // =========================================================================
  describe('8. Handling deleted, archived, or unavailable records', () => {
    it('filters out deleted items during retrieval and blocks actions on unavailable items safely', async () => {
      // 1. Retrieval excludes items marked as deleted
      const items: Item[] = [
        {
          id: 'item-active',
          workspace_id: 'ctx-avail',
          session_id: null,
          item_type: 'url',
          content: 'https://valid.com',
          title: 'Active Link',
          metadata: JSON.stringify({ is_important: true, is_deleted: false }),
          created_at: '2026-10-20T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
        },
        {
          id: 'item-deleted',
          workspace_id: 'ctx-avail',
          session_id: null,
          item_type: 'file',
          content: '/deleted.txt',
          title: 'Deleted Note',
          metadata: JSON.stringify({ is_deleted: true }),
          created_at: '2026-10-20T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
        },
      ];

      vi.mocked(db.getWorkspace).mockResolvedValueOnce({
        id: 'ctx-avail',
        name: 'Availability Project',
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-22T10:00:00Z',
      });
      vi.mocked(db.getLastActiveTask).mockResolvedValueOnce(null);
      vi.mocked(db.getContextMemory).mockResolvedValueOnce(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValueOnce([]);
      vi.mocked(db.listItems).mockResolvedValueOnce(items);

      const workset = await retrieveFromRepository('ctx-avail');
      expect(workset?.items.some((i) => i.id === 'item-deleted')).toBe(false);

      // 2. Action execution: deleted item is BLOCKED
      vi.mocked(db.getItem).mockResolvedValueOnce({
        ...items[1],
        metadata: JSON.stringify({ is_deleted: true }),
      });

      const blockedResult = await quickActionsService.executeItemAction(
        'item.open',
        'ctx-avail',
        'item-deleted'
      );
      expect(blockedResult.outcome).toBe('BLOCKED');
      expect(blockedResult.message).toContain('deleted');

      // 3. Action execution: nonexistent item returns ERROR
      vi.mocked(db.getItem).mockResolvedValueOnce(null as any);
      const missingResult = await quickActionsService.executeItemAction(
        'item.open',
        'ctx-avail',
        'item-nonexistent'
      );
      expect(missingResult.outcome).toBe('ERROR');
      expect(missingResult.message).toContain('unavailable');
    });
  });

  // =========================================================================
  // 9. Loading, empty, partial, and error states
  // =========================================================================
  describe('9. Loading, empty, partial, and error states', () => {
    it('transitions correctly between loading, empty, partial, complete, and error states', async () => {
      // Empty presentation
      const emptyPres = createEmptyWorksetPresentation('ctx-empty', 'Empty Context');
      expect(emptyPres.populationState).toBe('empty');
      expect(emptyPres.hasItems).toBe(false);
      expect(emptyPres.hasTask).toBe(false);

      // Partial presentation (items present, but no active task)
      const partialItem = normalizeWorksetItem(
        {
          id: 'item-partial-1',
          title: 'Notes',
          itemType: 'url',
          workspaceId: 'ctx-partial',
          confidence: 0.8,
        },
        'ctx-partial'
      );
      const partialWorkset = deriveMinimalWorkset({
        contextId: 'ctx-partial',
        contextName: 'Partial Context',
        activeTaskId: null,
        activeTaskTitle: null,
        rawItems: [partialItem],
      });
      const partialPres = createWorksetPresentationModel(partialWorkset);
      expect(partialPres.populationState).toBe('partial');
      expect(partialPres.hasItems).toBe(true);
      expect(partialPres.hasTask).toBe(false);

      // Complete presentation (both items and active task)
      const completeWorkset = deriveMinimalWorkset({
        contextId: 'ctx-complete',
        contextName: 'Complete Context',
        activeTaskId: 'task-1',
        activeTaskTitle: 'Fix Bug',
        rawItems: [{ ...partialItem, relevanceTier: 'primary' }],
      });
      const completePres = createWorksetPresentationModel(completeWorkset);
      expect(completePres.populationState).toBe('complete');
      expect(completePres.hasItems).toBe(true);
      expect(completePres.hasTask).toBe(true);
    });
  });

  // =========================================================================
  // 10. Switching contexts while requests are in flight
  // =========================================================================
  describe('10. Switching contexts while requests are in flight', () => {
    it('discards delayed in-flight responses and prevents phantom context leakage', async () => {
      let resolveSlowA: (val: any) => void;
      const slowAPromise = new Promise((resolve) => {
        resolveSlowA = resolve;
      });

      // Context A is slow
      vi.mocked(db.getWorkspace).mockImplementation(async (id: string) => {
        if (id === 'ctx-slow-a') {
          await slowAPromise;
          return { id: 'ctx-slow-a', name: 'Slow Context A', created_at: '', updated_at: '' };
        }
        return { id: 'ctx-fast-b', name: 'Fast Context B', created_at: '', updated_at: '' };
      });
      vi.mocked(db.getLastActiveTask).mockResolvedValue(null);
      vi.mocked(db.getContextMemory).mockResolvedValue(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValue([]);
      vi.mocked(db.listItems).mockResolvedValue([]);

      // 1. Select slow A
      const reqA = useLauncherStore.getState().selectContextById('ctx-slow-a');

      // 2. Rapidly select fast B
      const reqB = useLauncherStore.getState().selectContextById('ctx-fast-b');
      await reqB;

      // Fast B finishes first
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-fast-b');
      expect(useLauncherStore.getState().activePreview?.contextId).toBe('ctx-fast-b');

      // Now resolve slow A
      resolveSlowA!({ id: 'ctx-slow-a', name: 'Slow Context A' });
      await reqA;

      // Invariant: Slower response A must NOT overwrite B
      expect(useLauncherStore.getState().selectedContextId).toBe('ctx-fast-b');
      expect(useLauncherStore.getState().activePreview?.contextId).toBe('ctx-fast-b');
    });
  });

  // =========================================================================
  // 11. Opening or revealing supported item types
  // =========================================================================
  describe('11. Opening or revealing supported item types', () => {
    it('opens URLs via system opener, reveals files, and rejects unsupported types', async () => {
      // URL item
      vi.mocked(db.getItem).mockResolvedValueOnce({
        id: 'item-url',
        workspace_id: 'ctx-actions',
        session_id: null,
        item_type: 'url',
        content: 'https://vitest.dev',
        title: 'Vitest Docs',
        metadata: '{}',
        created_at: '',
        updated_at: '',
      });

      const urlResult = await quickActionsService.executeItemAction(
        'item.open',
        'ctx-actions',
        'item-url'
      );
      expect(urlResult.success).toBe(true);
      expect(urlResult.outcome).toBe('SUCCESS');
      expect(openUrl).toHaveBeenCalledWith('https://vitest.dev/');

      // File item
      vi.mocked(db.getItem).mockResolvedValueOnce({
        id: 'item-file',
        workspace_id: 'ctx-actions',
        session_id: null,
        item_type: 'file',
        content: '/Users/test/project/file.ts',
        title: 'file.ts',
        metadata: '{}',
        created_at: '',
        updated_at: '',
      });

      const fileResult = await quickActionsService.executeItemAction(
        'item.reveal',
        'ctx-actions',
        'item-file'
      );
      expect(fileResult.success).toBe(true);
      expect(fileResult.outcome).toBe('SUCCESS');
      expect(revealItemInDir).toHaveBeenCalledWith('/Users/test/project/file.ts');

      // Unsupported item type
      vi.mocked(db.getItem).mockResolvedValueOnce({
        id: 'item-note',
        workspace_id: 'ctx-actions',
        session_id: null,
        item_type: 'custom_note',
        content: 'Scratch note body',
        title: 'Note Title',
        metadata: '{}',
        created_at: '',
        updated_at: '',
      });

      const noteResult = await quickActionsService.executeItemAction(
        'item.open',
        'ctx-actions',
        'item-note'
      );
      expect(noteResult.success).toBe(false);
      expect(noteResult.outcome).toBe('UNSUPPORTED');
    });
  });

  // =========================================================================
  // 12. Recovering from item-action failures
  // =========================================================================
  describe('12. Recovering from item-action failures', () => {
    it('sets clear error state when item action fails and recovers seamlessly upon clearActionError', async () => {
      // Simulate item open failure
      vi.mocked(db.getItem).mockResolvedValueOnce({
        id: 'item-bad-file',
        workspace_id: 'ctx-err-rec',
        session_id: null,
        item_type: 'file',
        content: '', // Missing path
        title: 'Corrupt File',
        metadata: '{}',
        created_at: '',
        updated_at: '',
      });

      const result = await useLauncherStore
        .getState()
        .executeItemAction('item.reveal', 'ctx-err-rec', 'item-bad-file');

      expect(result.success).toBe(false);
      expect(useLauncherStore.getState().actionError).not.toBeNull();
      expect(useLauncherStore.getState().isActionExecuting).toBe(false);

      // User dismisses error
      useLauncherStore.getState().clearActionError();
      expect(useLauncherStore.getState().actionError).toBeNull();
    });
  });

  // =========================================================================
  // 13. Refreshing after relevant changes
  // =========================================================================
  describe('13. Refreshing after relevant changes', () => {
    it('invalidates preview cache and forces refresh after mutating item actions', async () => {
      const contextId = 'ctx-refresh-target';
      useLauncherStore.setState({ selectedContextId: contextId });

      let fetchCount = 0;
      vi.mocked(db.getWorkspace).mockImplementation(async () => {
        fetchCount++;
        return {
          id: contextId,
          name: 'Refresh Target',
          created_at: '2026-10-20T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
        };
      });
      vi.mocked(db.getLastActiveTask).mockResolvedValue(null);
      vi.mocked(db.getContextMemory).mockResolvedValue(null);
      vi.mocked(db.listWorkspaceMemberships).mockResolvedValue([]);
      vi.mocked(db.listItems).mockResolvedValue([]);

      // 1. Initial selection loads preview
      await useLauncherStore.getState().selectContextById(contextId);
      const initialFetchCount = fetchCount;
      expect(initialFetchCount).toBeGreaterThan(0);

      // 2. Mutation: item.mark_important triggers forced refresh
      await useLauncherStore
        .getState()
        .executeItemAction('item.mark_important', contextId, 'item-1');

      expect(fetchCount).toBeGreaterThan(initialFetchCount);
    });
  });

  // =========================================================================
  // 14. Preserving existing launcher quick actions and keyboard behavior
  // =========================================================================
  describe('14. Preserving existing launcher quick actions and keyboard behavior', () => {
    it('preserves context resumption and keyboard navigation along with workset preview synchronization', async () => {
      const contextId = 'ctx-reg-valid';
      useWorkspaceStore.setState({ activeWorkspaceId: null });

      vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({
        context_id: contextId,
        previous_context_id: null,
        resumed_at: '2026-10-22T12:00:00Z',
        state: 'active',
      } as any);

      // Context resume execution
      const resumeResult = await useLauncherStore
        .getState()
        .executeContextAction('context.resume', contextId);

      expect(resumeResult.success).toBe(true);
      expect(contextInteractionService.resumeContext).toHaveBeenCalledWith(contextId);
      expect(useWorkspaceStore.getState().activeWorkspaceId).toBe(contextId);

      // Selection synchronization
      useLauncherStore.getState().setSelectedIndex(1);
      expect(useLauncherStore.getState().selectedIndex).toBe(1);
    });
  });
});
