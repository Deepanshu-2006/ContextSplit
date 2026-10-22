/**
 * Context Workset Integration Validation Suite
 *
 * Validates the Context Workset integration against realistic usage scenarios
 * per the ContextSplit implementation and specifications:
 *
 * 1. Open a context that has several explicitly associated items.
 * 2. Open a context that has no relevant items.
 * 3. Open a context with both confirmed and inferred associations.
 * 4. Switch quickly between two contexts (concurrent/stale response cancellation).
 * 5. Open an item from the workset (supported action, safe execution).
 * 6. Simulate an unavailable or deleted item using safe test data.
 * 7. Refresh the workset after a supported change.
 * 8. Restart or reinitialize components and verify persisted data integrity.
 *
 * Verification criteria checked for each scenario:
 * - Correct context and item identities.
 * - Correct item associations.
 * - No duplicate entries.
 * - No stale items from another selected context.
 * - Accurate loading, empty, and error states.
 * - No false success feedback.
 * - No unexpected changes to contextual membership or workspace placement.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  useLauncherStore,
  invalidateLauncherPreviewCache,
  getLauncherPreviewCache,
} from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { contextWorksetService } from '../../services/contextWorkset';
import { retrieveContextWorkset } from '../../services/contextWorksetRetrieval';
import { contextInteractionService } from '../../services/contextInteractionService';
import { db } from '../../services/db';
import type { MinimalContextWorkset } from '../../models/contextWorkset';

// Mock Tauri core invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock Tauri opener plugin
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
  revealItemInDir: vi.fn().mockResolvedValue(undefined),
  openPath: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getContextState: vi.fn(),
    getResumeReadiness: vi.fn(),
    resumeContext: vi.fn(),
    pauseContext: vi.fn(),
  },
}));

vi.mock('../../services/contextWorkset', () => ({
  contextWorksetService: {
    controlContextMemory: vi.fn(),
  },
}));

vi.mock('../../services/contextWorksetRetrieval', () => ({
  retrieveContextWorkset: vi.fn(),
}));

vi.mock('../../services/db', () => ({
  db: {
    getItem: vi.fn(),
    updateWorkspace: vi.fn(),
    archiveWorkspace: vi.fn(),
    unarchiveWorkspace: vi.fn(),
  },
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
    warning: vi.fn(),
  },
}));

describe('Context Workset Integration Validation (Realistic Scenarios)', () => {
  // Isolated test data fixtures
  const expItem1 = {
    id: 'item-exp-1',
    contextId: 'ctx-explicit-1',
    workspaceId: 'ctx-explicit-1',
    title: 'Architecture Blueprint',
    itemType: 'file',
    contentSnippet: '/docs/architecture.pdf',
    provenance: 'confirmed' as const,
    physicalLocation: 'workspace' as const,
    status: 'available' as const,
    relevanceTier: 'primary' as const,
    confidence: 0.95,
    isSuggestedAddition: false,
    isImportant: false,
    reasons: ['Directly assigned to workspace'],
    lastAccessedAt: '2026-10-22T21:00:00Z',
  };

  const expItem2 = {
    id: 'item-exp-2',
    contextId: 'ctx-explicit-1',
    workspaceId: 'ctx-explicit-1',
    title: 'Project Repository',
    itemType: 'url',
    contentSnippet: 'https://github.com/project/engine',
    provenance: 'confirmed' as const,
    physicalLocation: 'workspace' as const,
    status: 'available' as const,
    relevanceTier: 'primary' as const,
    confidence: 0.92,
    isSuggestedAddition: false,
    isImportant: false,
    reasons: ['Directly assigned to workspace'],
    lastAccessedAt: '2026-10-22T20:30:00Z',
  };

  const expItem3 = {
    id: 'item-exp-3',
    contextId: 'ctx-explicit-1',
    workspaceId: 'ctx-explicit-1',
    title: 'Engine Configuration',
    itemType: 'file',
    contentSnippet: '/etc/engine.conf',
    provenance: 'confirmed' as const,
    physicalLocation: 'workspace' as const,
    status: 'available' as const,
    relevanceTier: 'supporting' as const,
    confidence: 0.81,
    isSuggestedAddition: false,
    isImportant: false,
    reasons: ['Configuration file in context'],
    lastAccessedAt: '2026-10-22T19:00:00Z',
  };

  const expItem4 = {
    id: 'item-exp-4',
    contextId: 'ctx-explicit-1',
    workspaceId: 'ctx-explicit-1',
    title: 'API Reference Notes',
    itemType: 'note',
    contentSnippet: 'Key bindings and endpoints',
    provenance: 'confirmed' as const,
    physicalLocation: 'workspace' as const,
    status: 'available' as const,
    relevanceTier: 'reference' as const,
    confidence: 0.70,
    isSuggestedAddition: false,
    isImportant: false,
    reasons: ['Context documentation'],
    lastAccessedAt: '2026-10-22T18:00:00Z',
  };

  const mockWorksetExplicit: MinimalContextWorkset = {
    contextId: 'ctx-explicit-1',
    contextName: 'Project Engine',
    lifecycleState: 'active',
    worksetState: 'active',
    items: [expItem1, expItem2, expItem3, expItem4],
    primaryItems: [expItem1, expItem2],
    supportingItems: [expItem3],
    referenceItems: [expItem4],
    suggestedAdditions: [],
    totalItemCount: 4,
    recentActivitySummary: 'Active in Project Engine with 4 confirmed assets.',
    activeTaskTitle: 'Stabilizing core engine pipelines',
    confidence: 0.95,
    confidenceBand: 'high',
    readinessBand: 'ready',
    generatedAt: '2026-10-22T22:00:00Z',
  };

  const mockWorksetEmpty: MinimalContextWorkset = {
    contextId: 'ctx-empty-workspace',
    contextName: 'Fresh Sandbox',
    lifecycleState: 'active',
    worksetState: 'dormant',
    items: [],
    primaryItems: [],
    supportingItems: [],
    referenceItems: [],
    suggestedAdditions: [],
    totalItemCount: 0,
    recentActivitySummary: null,
    activeTaskTitle: null,
    confidence: 0.5,
    confidenceBand: 'low',
    readinessBand: 'low',
    generatedAt: '2026-10-22T22:00:00Z',
  };

  const hybConfirmedItem = {
    id: 'item-hyb-confirmed',
    contextId: 'ctx-hybrid-proj',
    workspaceId: 'ctx-hybrid-proj',
    title: 'Production Schema',
    itemType: 'file',
    contentSnippet: '/db/schema.sql',
    provenance: 'confirmed' as const,
    physicalLocation: 'workspace' as const,
    status: 'available' as const,
    relevanceTier: 'primary' as const,
    confidence: 0.94,
    isSuggestedAddition: false,
    isImportant: false,
    reasons: ['Explicit workspace file'],
    lastAccessedAt: '2026-10-22T21:00:00Z',
  };

  const hybInferredItem = {
    id: 'item-hyb-inferred',
    contextId: 'ctx-hybrid-proj',
    workspaceId: null,
    title: 'Related Research Doc',
    itemType: 'url',
    contentSnippet: 'https://research.internal/docs',
    provenance: 'inferred' as const,
    physicalLocation: 'inbox' as const,
    status: 'available' as const,
    relevanceTier: 'suggested' as const,
    confidence: 0.85,
    isSuggestedAddition: true,
    isImportant: false,
    reasons: ['Discovered via semantic affinity with schema tasks'],
    inferredReason: 'Discovered via semantic affinity with schema tasks',
    lastAccessedAt: '2026-10-22T20:00:00Z',
  };

  const mockWorksetHybrid: MinimalContextWorkset = {
    contextId: 'ctx-hybrid-proj',
    contextName: 'Hybrid Context',
    lifecycleState: 'active',
    worksetState: 'active',
    items: [hybConfirmedItem, hybInferredItem],
    primaryItems: [hybConfirmedItem],
    supportingItems: [],
    referenceItems: [],
    suggestedAdditions: [hybInferredItem],
    totalItemCount: 2,
    recentActivitySummary: 'Active with 1 confirmed asset and 1 suggestion.',
    activeTaskTitle: 'Schema migration',
    confidence: 0.88,
    confidenceBand: 'high',
    readinessBand: 'ready',
    generatedAt: '2026-10-22T22:00:00Z',
  };

  beforeEach(() => {
    vi.clearAllMocks();
    invalidateLauncherPreviewCache();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-explicit-1',
          name: 'Project Engine',
          lifecycle_state: 'active',
          created_at: '2026-10-01T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
        } as any,
        {
          id: 'ctx-empty-workspace',
          name: 'Fresh Sandbox',
          lifecycle_state: 'active',
          created_at: '2026-10-10T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
        } as any,
        {
          id: 'ctx-hybrid-proj',
          name: 'Hybrid Context',
          lifecycle_state: 'active',
          created_at: '2026-10-12T10:00:00Z',
          updated_at: '2026-10-22T10:00:00Z',
        } as any,
      ],
      activeWorkspaceId: 'ctx-explicit-1',
    });

    useLauncherStore.setState({
      isOpen: true,
      selectedContextId: null,
      activePreview: null,
      isLoadingPreview: false,
      isActionExecuting: false,
      executingActionId: null,
      executingTargetId: null,
      actionError: null,
      previewError: null,
      recentContexts: [
        { id: 'ctx-explicit-1', name: 'Project Engine', state: 'active' },
        { id: 'ctx-empty-workspace', name: 'Fresh Sandbox', state: 'active' },
        { id: 'ctx-hybrid-proj', name: 'Hybrid Context', state: 'active' },
      ],
    });

    vi.mocked(contextInteractionService.getContextState).mockImplementation(async (id) => ({
      context_id: id,
      context_name:
        id === 'ctx-explicit-1'
          ? 'Project Engine'
          : id === 'ctx-empty-workspace'
          ? 'Fresh Sandbox'
          : 'Hybrid Context',
      state: 'active',
      freshness: 'warm',
      task_title: id === 'ctx-explicit-1' ? 'Stabilizing core engine pipelines' : null,
      last_meaningful_activity:
        id === 'ctx-explicit-1' ? 'Active in Project Engine with 4 confirmed assets.' : null,
    } as any));

    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValue({
      ready: true,
      score: 0.95,
      reason: 'Context is active and ready',
    } as any);
  });

  afterEach(() => {
    useLauncherStore.getState().closeLauncher();
    vi.restoreAllMocks();
  });

  // =========================================================================
  // Scenario 1: Open a context that has several explicitly associated items
  // =========================================================================
  it('Scenario 1: correctly loads context with explicitly associated items and preserves identities without duplicates', async () => {
    vi.mocked(retrieveContextWorkset).mockResolvedValueOnce(mockWorksetExplicit);

    await useLauncherStore.getState().selectContextById('ctx-explicit-1');

    const preview = useLauncherStore.getState().activePreview;
    expect(preview).not.toBeNull();
    expect(preview?.contextId).toBe('ctx-explicit-1');
    expect(preview?.name).toBe('Project Engine');

    // Verify item count and correct item identities
    expect(preview?.worksetItems).toHaveLength(4);
    const itemIds = preview?.worksetItems.map((item) => item.id);
    expect(itemIds).toEqual(['item-exp-1', 'item-exp-2', 'item-exp-3', 'item-exp-4']);

    // Verify no duplicates
    const uniqueIds = new Set(itemIds);
    expect(uniqueIds.size).toBe(4);

    // Verify associations and provenances
    preview?.worksetItems.forEach((item) => {
      expect(item.provenance).toBe('confirmed');
      expect(item.isSuggestedAddition).toBe(false);
      expect(item.status).toBe('available');
    });

    // Verify active task and activity data
    expect(preview?.worksetPresentation?.activeTaskTitle).toBe('Stabilizing core engine pipelines');
    expect(preview?.worksetPresentation?.recentActivitySummary).toBe(
      'Active in Project Engine with 4 confirmed assets.'
    );

    // Verify no unexpected contextual membership or workspace placement changed
    expect(useWorkspaceStore.getState().workspaces).toHaveLength(3);
    expect(useWorkspaceStore.getState().workspaces[0].id).toBe('ctx-explicit-1');
  });

  // =========================================================================
  // Scenario 2: Open a context that has no relevant items
  // =========================================================================
  it('Scenario 2: correctly displays empty state without fabricating items or false success feedback', async () => {
    vi.mocked(retrieveContextWorkset).mockResolvedValueOnce(mockWorksetEmpty);

    await useLauncherStore.getState().selectContextById('ctx-empty-workspace');

    const preview = useLauncherStore.getState().activePreview;
    expect(preview).not.toBeNull();
    expect(preview?.contextId).toBe('ctx-empty-workspace');
    expect(preview?.name).toBe('Fresh Sandbox');

    // Verify no fabricated items
    expect(preview?.worksetItems).toHaveLength(0);
    expect(preview?.worksetPresentation?.hasItems).toBe(false);
    expect(preview?.worksetPresentation?.populationState).toBe('empty');
    expect(preview?.worksetPresentation?.totalItemCount).toBe(0);

    // Verify clean empty state messaging without errors
    expect(useLauncherStore.getState().previewError).toBeNull();
    expect(useLauncherStore.getState().isLoadingPreview).toBe(false);
  });

  // =========================================================================
  // Scenario 3: Open a context with both confirmed and inferred associations
  // =========================================================================
  it('Scenario 3: maintains strict epistemic distinction between confirmed and inferred items', async () => {
    vi.mocked(retrieveContextWorkset).mockResolvedValueOnce(mockWorksetHybrid);

    await useLauncherStore.getState().selectContextById('ctx-hybrid-proj');

    const preview = useLauncherStore.getState().activePreview;
    expect(preview).not.toBeNull();
    expect(preview?.worksetItems).toHaveLength(2);

    const confirmedItem = preview?.worksetItems.find((i) => i.id === 'item-hyb-confirmed');
    const inferredItem = preview?.worksetItems.find((i) => i.id === 'item-hyb-inferred');

    // Confirmed item assertions
    expect(confirmedItem).toBeDefined();
    expect(confirmedItem?.provenance).toBe('confirmed');
    expect(confirmedItem?.relevanceTier).toBe('primary');
    expect(confirmedItem?.isSuggestedAddition).toBe(false);

    // Inferred item assertions
    expect(inferredItem).toBeDefined();
    expect(inferredItem?.provenance).toBe('inferred');
    expect(inferredItem?.relevanceTier).toBe('suggested');
    expect(inferredItem?.isSuggestedAddition).toBe(true);

    // Presentation model separates them into distinct buckets
    expect(preview?.worksetPresentation?.primaryFocus).toHaveLength(1);
    expect(preview?.worksetPresentation?.suggestedAdditions).toHaveLength(1);
  });

  // =========================================================================
  // Scenario 4: Switch quickly between two contexts (concurrent / out-of-order)
  // =========================================================================
  it('Scenario 4: discards delayed out-of-order responses and prevents stale context item leakage', async () => {
    // Setup: Slow Explicit fetch (80ms), fast Empty fetch (10ms)
    vi.mocked(retrieveContextWorkset).mockImplementation(async (id) => {
      if (id === 'ctx-explicit-1') {
        await new Promise((res) => setTimeout(res, 80));
        return mockWorksetExplicit;
      }
      if (id === 'ctx-empty-workspace') {
        await new Promise((res) => setTimeout(res, 10));
        return mockWorksetEmpty;
      }
      throw new Error(`Unknown context ${id}`);
    });

    // Rapid switch: Explicit first, then Empty 5ms later
    const pFirst = useLauncherStore.getState().selectContextById('ctx-explicit-1');
    await new Promise((res) => setTimeout(res, 5));
    const pSecond = useLauncherStore.getState().selectContextById('ctx-empty-workspace');

    await Promise.all([pFirst, pSecond]);

    // Ensure Empty is retained and Explicit's slower response was discarded
    const finalState = useLauncherStore.getState();
    expect(finalState.selectedContextId).toBe('ctx-empty-workspace');
    expect(finalState.activePreview?.contextId).toBe('ctx-empty-workspace');
    expect(finalState.activePreview?.name).toBe('Fresh Sandbox');
    expect(finalState.activePreview?.worksetItems).toHaveLength(0);
  });

  // =========================================================================
  // Scenario 5: Open an item from the workset
  // =========================================================================
  it('Scenario 5: executes item.open safely via existing opener service without altering workspace memberships', async () => {
    vi.mocked(retrieveContextWorkset).mockResolvedValueOnce(mockWorksetExplicit);
    await useLauncherStore.getState().selectContextById('ctx-explicit-1');

    // Mock db.getItem for item-exp-2
    vi.mocked(db.getItem).mockResolvedValueOnce({
      id: 'item-exp-2',
      workspace_id: 'ctx-explicit-1',
      session_id: null,
      item_type: 'url',
      content: 'https://github.com/project/engine',
      title: 'Project Repository',
      metadata: '{}',
      created_at: '2026-10-22T10:00:00Z',
      updated_at: '2026-10-22T10:00:00Z',
    });

    const actionResult = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-explicit-1', 'item-exp-2');

    expect(actionResult.success).toBe(true);
    expect(actionResult.outcome).toBe('SUCCESS');
    expect(actionResult.targetItemId).toBe('item-exp-2');
    expect(actionResult.resolvedContextId).toBe('ctx-explicit-1');

    // Verify opener was invoked
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    expect(openUrl).toHaveBeenCalledWith('https://github.com/project/engine');

    // Workspace membership remains intact
    expect(useWorkspaceStore.getState().activeWorkspaceId).toBe('ctx-explicit-1');
  });

  // =========================================================================
  // Scenario 6: Simulate an unavailable or deleted item using safe test data
  // =========================================================================
  it('Scenario 6: safely handles unavailable or missing items with clear error and preserves context integrity', async () => {
    vi.mocked(retrieveContextWorkset).mockResolvedValueOnce(mockWorksetExplicit);
    await useLauncherStore.getState().selectContextById('ctx-explicit-1');

    // Mock db.getItem returning a deleted / null record
    vi.mocked(db.getItem).mockRejectedValueOnce(new Error('Record not found in database'));

    const actionResult = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-explicit-1', 'item-nonexistent-999');

    expect(actionResult.success).toBe(false);
    expect(actionResult.outcome).toBe('ERROR');
    expect(actionResult.message).toMatch(/not found|missing|invalid|unavailable|removed/i);

    // Verify store error state is surfaced accurately
    expect(useLauncherStore.getState().actionError).not.toBeNull();

    // Verify context and preview are preserved without crashing
    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-explicit-1');
    expect(useLauncherStore.getState().activePreview?.worksetItems).toHaveLength(4);
  });

  // =========================================================================
  // Scenario 7: Refresh the workset after a supported change
  // =========================================================================
  it('Scenario 7: invalidates cache and triggers reliable refresh after material item mutation', async () => {
    vi.mocked(retrieveContextWorkset).mockResolvedValueOnce(mockWorksetExplicit);
    await useLauncherStore.getState().selectContextById('ctx-explicit-1');

    // Verify cached initially
    expect(getLauncherPreviewCache().has('ctx-explicit-1')).toBe(true);

    vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValue(true);

    // After mutation, retrieval service will return item marked important = true
    const updatedWorkset: MinimalContextWorkset = {
      ...mockWorksetExplicit,
      primaryItems: [
        {
          ...mockWorksetExplicit.primaryItems[0],
          isImportant: true,
        },
        mockWorksetExplicit.primaryItems[1],
      ],
    };

    vi.mocked(retrieveContextWorkset).mockResolvedValue(updatedWorkset);

    // Execute material item mutation
    const result = await useLauncherStore
      .getState()
      .executeItemAction('item.mark_important', 'ctx-explicit-1', 'item-exp-1');

    expect(result.success).toBe(true);
    expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith(
      'ctx-explicit-1',
      'item-exp-1',
      'keep_important'
    );

    // Verify cache was invalidated and re-fetched
    const updatedPreview = useLauncherStore.getState().activePreview;
    expect(updatedPreview?.worksetItems[0].isImportant).toBe(true);
  });

  // =========================================================================
  // Scenario 8: Restart or reinitialize components and verify persisted data
  // =========================================================================
  it('Scenario 8: cleanly reinitializes state after simulated restart without data loss or corruption', async () => {
    // Phase A: Initial session loads and caches data
    vi.mocked(retrieveContextWorkset).mockResolvedValueOnce(mockWorksetExplicit);
    await useLauncherStore.getState().selectContextById('ctx-explicit-1');
    expect(useLauncherStore.getState().activePreview?.worksetItems).toHaveLength(4);

    // Phase B: Simulate component unmount / application restart
    // 1. Invalidate in-memory caches
    invalidateLauncherPreviewCache();
    expect(getLauncherPreviewCache().has('ctx-explicit-1')).toBe(false);

    // 2. Reset launcher store to initial cold state
    useLauncherStore.setState({
      isOpen: false,
      selectedContextId: null,
      activePreview: null,
      isLoadingPreview: false,
      previewError: null,
      actionError: null,
    });

    // 3. Re-open launcher and select context afresh
    useLauncherStore.setState({ isOpen: true });
    vi.mocked(retrieveContextWorkset).mockResolvedValueOnce(mockWorksetExplicit);
    await useLauncherStore.getState().selectContextById('ctx-explicit-1');

    // Phase C: Verify clean rehydration with exact identities and associations
    const rehydratedPreview = useLauncherStore.getState().activePreview;
    expect(rehydratedPreview).not.toBeNull();
    expect(rehydratedPreview?.contextId).toBe('ctx-explicit-1');
    expect(rehydratedPreview?.worksetItems).toHaveLength(4);

    const rehydratedIds = rehydratedPreview?.worksetItems.map((i) => i.id);
    expect(rehydratedIds).toEqual(['item-exp-1', 'item-exp-2', 'item-exp-3', 'item-exp-4']);

    // Workspace store remains uncorrupted
    expect(useWorkspaceStore.getState().workspaces).toHaveLength(3);
    expect(useWorkspaceStore.getState().workspaces[0].name).toBe('Project Engine');
  });
});
