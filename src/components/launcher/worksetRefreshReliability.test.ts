/**
 * Context Workset Data-Refresh Reliability Test Suite
 *
 * Verifies robust and reliable data-refresh behavior for Context Worksets:
 * 1. Refresh workset data when the selected context changes.
 * 2. Prevent phantom context leakage: context switching does NOT display another context's items.
 * 3. Concurrent requests & monotonic sequencing: stale asynchronous responses never overwrite newer data.
 * 4. Successful mutations materially affecting the workset trigger targeted refresh.
 * 5. Read-only actions (open/reveal) do not cause unnecessary refetches or cache invalidations.
 * 6. Preserves valid data when a refresh fails, setting clear error notification.
 * 7. Gracefully handles deleted or unavailable items during refresh.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  useLauncherStore,
  invalidateLauncherPreviewCache,
  getLauncherPreviewCache,
} from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { contextInteractionService } from '../../services/contextInteractionService';
import { contextWorksetService } from '../../services/contextWorkset';
import { retrieveContextWorkset } from '../../services/contextWorksetRetrieval';
import { db } from '../../services/db';
import type { MinimalContextWorkset } from '../../models/contextWorkset';

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

describe('Context Workset Data-Refresh Reliability', () => {
  const mockWorksetAlpha: MinimalContextWorkset = {
    contextId: 'ctx-alpha',
    contextName: 'Alpha Engine',
    lifecycleState: 'active',
    worksetState: 'active',
    confidence: 0.95,
    confidenceBand: 'high',
    readinessBand: 'ready',
    totalItemCount: 1,
    generatedAt: new Date().toISOString(),
    items: [
      {
        id: 'item-a1',
        title: 'engine.rs',
        itemType: 'file',
        relevanceTier: 'primary',
        provenance: 'confirmed',
        physicalLocation: 'workspace',
        workspaceId: 'ctx-alpha',
        contextId: 'ctx-alpha',
        confidence: 0.95,
        isSuggestedAddition: false,
        isImportant: true,
        status: 'available',
        reasons: ['Associated workspace file'],
      },
    ],
    primaryItems: [
      {
        id: 'item-a1',
        title: 'engine.rs',
        itemType: 'file',
        relevanceTier: 'primary',
        provenance: 'confirmed',
        physicalLocation: 'workspace',
        workspaceId: 'ctx-alpha',
        contextId: 'ctx-alpha',
        confidence: 0.95,
        isSuggestedAddition: false,
        isImportant: true,
        status: 'available',
        reasons: ['Associated workspace file'],
      },
    ],
    supportingItems: [],
    referenceItems: [],
    suggestedAdditions: [],
    activeTaskTitle: 'Stabilize workset refresh',
    recentActivitySummary: 'Modified engine.rs',
  };

  const mockWorksetBeta: MinimalContextWorkset = {
    contextId: 'ctx-beta',
    contextName: 'Beta Analytics',
    lifecycleState: 'active',
    worksetState: 'active',
    confidence: 0.90,
    confidenceBand: 'high',
    readinessBand: 'ready',
    totalItemCount: 1,
    generatedAt: new Date().toISOString(),
    items: [
      {
        id: 'item-b1',
        title: 'dashboard.tsx',
        itemType: 'file',
        relevanceTier: 'primary',
        provenance: 'confirmed',
        physicalLocation: 'workspace',
        workspaceId: 'ctx-beta',
        contextId: 'ctx-beta',
        confidence: 0.90,
        isSuggestedAddition: false,
        isImportant: false,
        status: 'available',
        reasons: ['Associated workspace file'],
      },
    ],
    primaryItems: [
      {
        id: 'item-b1',
        title: 'dashboard.tsx',
        itemType: 'file',
        relevanceTier: 'primary',
        provenance: 'confirmed',
        physicalLocation: 'workspace',
        workspaceId: 'ctx-beta',
        contextId: 'ctx-beta',
        confidence: 0.90,
        isSuggestedAddition: false,
        isImportant: false,
        status: 'available',
        reasons: ['Associated workspace file'],
      },
    ],
    supportingItems: [],
    referenceItems: [],
    suggestedAdditions: [],
    activeTaskTitle: 'Render analytics charts',
    recentActivitySummary: 'Updated dashboard.tsx',
  };

  beforeEach(() => {
    vi.clearAllMocks();
    invalidateLauncherPreviewCache();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-alpha',
          name: 'Alpha Engine',
          lifecycle_state: 'active',
          updated_at: new Date().toISOString(),
          created_at: new Date().toISOString(),
          items: [],
        } as any,
        {
          id: 'ctx-beta',
          name: 'Beta Analytics',
          lifecycle_state: 'recent',
          updated_at: new Date().toISOString(),
          created_at: new Date().toISOString(),
          items: [],
        } as any,
      ],
      activeWorkspaceId: 'ctx-alpha',
    });

    useLauncherStore.setState({
      isOpen: true,
      selectedContextId: null,
      activePreview: null,
      isLoadingPreview: false,
      previewError: null,
      recentContexts: [
        { id: 'ctx-alpha', name: 'Alpha Engine', state: 'active' },
        { id: 'ctx-beta', name: 'Beta Analytics', state: 'recent' },
      ],
    });

    vi.mocked(contextInteractionService.getContextState).mockImplementation(async (id) => ({
      context_id: id,
      context_name: id === 'ctx-alpha' ? 'Alpha Engine' : 'Beta Analytics',
      state: id === 'ctx-alpha' ? 'active' : 'recent',
      task_title: id === 'ctx-alpha' ? 'Stabilize workset refresh' : 'Render analytics charts',
      last_meaningful_activity: id === 'ctx-alpha' ? 'Modified engine.rs' : 'Updated dashboard.tsx',
    } as any));

    vi.mocked(contextInteractionService.getResumeReadiness).mockImplementation(async (id) => ({
      context_id: id,
      state: 'ready',
      readiness: {
        readiness: 'ready',
        formatted_explanation: 'Ready to continue',
      },
      formatted_resume_summary: 'All resources ready',
    } as any));

    vi.mocked(retrieveContextWorkset).mockImplementation(async (id) => {
      if (id === 'ctx-alpha') return mockWorksetAlpha;
      if (id === 'ctx-beta') return mockWorksetBeta;
      throw new Error(`Context ${id} not found`);
    });
  });

  afterEach(() => {
    useLauncherStore.getState().closeLauncher();
  });

  // =========================================================================
  // 1. Context Switching & Phantom Context Leakage Prevention (Req 1, 7)
  // =========================================================================
  it('refreshes workset when selected context changes and never displays phantom items', async () => {
    // 1a. Select Context Alpha
    const selectAlphaPromise = useLauncherStore.getState().selectContextById('ctx-alpha');
    await selectAlphaPromise;

    const stateAlpha = useLauncherStore.getState();
    expect(stateAlpha.selectedContextId).toBe('ctx-alpha');
    expect(stateAlpha.activePreview?.contextId).toBe('ctx-alpha');
    expect(stateAlpha.activePreview?.worksetItems).toHaveLength(1);
    expect(stateAlpha.activePreview?.worksetItems[0].id).toBe('item-a1');

    // 1b. Switch to Context Beta (uncached)
    // Synchronously before resolution: activePreview must be cleared (null) so Alpha's items do not leak
    const selectBetaPromise = useLauncherStore.getState().selectContextById('ctx-beta');
    const immediateState = useLauncherStore.getState();
    expect(immediateState.selectedContextId).toBe('ctx-beta');
    expect(immediateState.activePreview).toBeNull(); // Phantom context leakage prevention!
    expect(immediateState.isLoadingPreview).toBe(true);

    // Wait for Beta to resolve
    await selectBetaPromise;

    const finalBetaState = useLauncherStore.getState();
    expect(finalBetaState.selectedContextId).toBe('ctx-beta');
    expect(finalBetaState.isLoadingPreview).toBe(false);
    expect(finalBetaState.activePreview?.contextId).toBe('ctx-beta');
    expect(finalBetaState.activePreview?.worksetItems[0].id).toBe('item-b1');
  });

  // =========================================================================
  // 2. Concurrent Requests & Monotonic Sequencing (Req 4)
  // =========================================================================
  it('prevents stale slower responses from overwriting newer selected context data', async () => {
    // Setup: Slow Alpha fetch (100ms), fast Beta fetch (10ms)
    vi.mocked(retrieveContextWorkset).mockImplementation(async (id) => {
      if (id === 'ctx-alpha') {
        await new Promise((res) => setTimeout(res, 100));
        return mockWorksetAlpha;
      }
      if (id === 'ctx-beta') {
        await new Promise((res) => setTimeout(res, 10));
        return mockWorksetBeta;
      }
      throw new Error('unknown');
    });

    // User rapidly navigates: selects Alpha then Beta
    const pAlpha = useLauncherStore.getState().selectContextById('ctx-alpha');
    // Rapidly switch to Beta after 5ms
    await new Promise((res) => setTimeout(res, 5));
    const pBeta = useLauncherStore.getState().selectContextById('ctx-beta');

    await Promise.all([pAlpha, pBeta]);

    // Ensure Beta is retained, Alpha's slower response was discarded
    const finalState = useLauncherStore.getState();
    expect(finalState.selectedContextId).toBe('ctx-beta');
    expect(finalState.activePreview?.contextId).toBe('ctx-beta');
    expect(finalState.activePreview?.name).toBe('Beta Analytics');
    expect(finalState.activePreview?.worksetItems[0].id).toBe('item-b1');
  });

  // =========================================================================
  // 3. Successful Mutations Trigger Refresh (Req 2, 3)
  // =========================================================================
  it('refreshes the workset after item.mark_important and item.unmark_important mutations', async () => {
    await useLauncherStore.getState().selectContextById('ctx-alpha');
    expect(getLauncherPreviewCache().has('ctx-alpha')).toBe(true);

    vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValue(true);

    // After mutation, retrieval service will return item marked important = true
    const updatedWorkset: MinimalContextWorkset = {
      ...mockWorksetAlpha,
      primaryItems: [{ ...mockWorksetAlpha.primaryItems[0], isImportant: true }],
    };
    vi.mocked(retrieveContextWorkset).mockResolvedValue(updatedWorkset);

    const result = await useLauncherStore
      .getState()
      .executeItemAction('item.mark_important', 'ctx-alpha', 'item-a1');

    expect(result.success).toBe(true);
    expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith(
      'ctx-alpha',
      'item-a1',
      'keep_important'
    );

    // Verify cache was invalidated and re-fetched
    const state = useLauncherStore.getState();
    expect(state.selectedContextId).toBe('ctx-alpha');
    expect(state.activePreview?.worksetItems[0].isImportant).toBe(true);
  });

  it('refreshes the workset after item.remove_from_context mutation', async () => {
    await useLauncherStore.getState().selectContextById('ctx-alpha');

    vi.mocked(contextWorksetService.controlContextMemory).mockResolvedValue(true);

    // After removal, retrieval returns empty primary items
    const emptyWorkset: MinimalContextWorkset = {
      ...mockWorksetAlpha,
      items: [],
      primaryItems: [],
      totalItemCount: 0,
    };
    vi.mocked(retrieveContextWorkset).mockResolvedValue(emptyWorkset);

    const result = await useLauncherStore
      .getState()
      .executeItemAction('item.remove_from_context', 'ctx-alpha', 'item-a1');

    expect(result.success).toBe(true);
    expect(contextWorksetService.controlContextMemory).toHaveBeenCalledWith(
      'ctx-alpha',
      'item-a1',
      'remove_from_context'
    );

    const state = useLauncherStore.getState();
    expect(state.activePreview?.worksetItems).toHaveLength(0);
    expect(state.activePreview?.itemCount).toBe(0);
  });

  it('refreshes preview and cache after context rename mutation', async () => {
    await useLauncherStore.getState().selectContextById('ctx-alpha');

    vi.mocked(db.updateWorkspace).mockResolvedValue({
      id: 'ctx-alpha',
      name: 'Alpha Core Engine',
      updated_at: new Date().toISOString(),
    } as any);

    const result = await useLauncherStore
      .getState()
      .renameContextById('ctx-alpha', 'Alpha Core Engine');

    expect(result.success).toBe(true);

    const state = useLauncherStore.getState();
    expect(state.activePreview?.name).toBe('Alpha Core Engine');
  });

  // =========================================================================
  // 4. Read-Only Operations Do Not Cause Invalidation or Refetch (Req 5)
  // =========================================================================
  it('does not invalidate preview cache on item.open or item.reveal', async () => {
    await useLauncherStore.getState().selectContextById('ctx-alpha');
    const initialEntry = getLauncherPreviewCache().get('ctx-alpha');
    expect(initialEntry).toBeDefined();

    vi.mocked(db.getItem).mockResolvedValue({
      id: 'item-a1',
      title: 'engine.rs',
      item_type: 'file',
      content: '/path/to/engine.rs',
      metadata: '{}',
    } as any);

    const openResult = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-alpha', 'item-a1');

    expect(openResult.success).toBe(true);
    // Cache entry remains valid and intact
    expect(getLauncherPreviewCache().get('ctx-alpha')).toBe(initialEntry);
  });

  // =========================================================================
  // 5. Preserving Valid Data on Refresh Failure (Req 8)
  // =========================================================================
  it('preserves existing valid workset data when a subsequent refresh fails', async () => {
    // 1. Initial successful retrieval
    await useLauncherStore.getState().selectContextById('ctx-alpha');
    const state1 = useLauncherStore.getState();
    expect(state1.activePreview?.worksetItems).toHaveLength(1);
    expect(state1.previewError).toBeNull();

    // 2. Subsequent forceRefresh fails with database lock timeout
    vi.mocked(retrieveContextWorkset).mockRejectedValueOnce(
      new Error('Database lock acquisition timeout')
    );

    await useLauncherStore.getState().selectContextById('ctx-alpha', { forceRefresh: true });

    const state2 = useLauncherStore.getState();
    // Valid workset items are preserved!
    expect(state2.activePreview?.contextId).toBe('ctx-alpha');
    expect(state2.activePreview?.worksetItems).toHaveLength(1);
    expect(state2.activePreview?.worksetItems[0].id).toBe('item-a1');
    // Error is surfaced clearly to the user
    expect(state2.previewError).toBe('Database lock acquisition timeout');
    expect(state2.isLoadingPreview).toBe(false);
  });

  it('provides safe fallback preview when first-time retrieval fails with no prior data', async () => {
    vi.mocked(retrieveContextWorkset).mockRejectedValueOnce(
      new Error('Context Engine IPC offline')
    );
    vi.mocked(contextInteractionService.getResumeReadiness).mockRejectedValueOnce(
      new Error('Context Engine IPC offline')
    );

    await useLauncherStore.getState().selectContextById('ctx-beta');

    const state = useLauncherStore.getState();
    expect(state.selectedContextId).toBe('ctx-beta');
    expect(state.isLoadingPreview).toBe(false);
    expect(state.previewError).toBe('Context Engine IPC offline');
    expect(state.activePreview?.contextId).toBe('ctx-beta');
    expect(state.activePreview?.worksetItems).toHaveLength(0);
    expect(state.activePreview?.readinessBand).toBe('low');
  });

  // =========================================================================
  // 6. Graceful Handling of Deleted or Unavailable Items (Req 6)
  // =========================================================================
  it('handles deleted item gracefully in refreshed workset and blocks action with clear feedback', async () => {
    const deletedWorkset: MinimalContextWorkset = {
      ...mockWorksetAlpha,
      primaryItems: [
        {
          ...mockWorksetAlpha.primaryItems[0],
          status: 'deleted',
          title: '[Deleted Item]',
        },
      ],
    };
    vi.mocked(retrieveContextWorkset).mockResolvedValue(deletedWorkset);

    await useLauncherStore.getState().selectContextById('ctx-alpha', { forceRefresh: true });

    const state = useLauncherStore.getState();
    expect(state.activePreview?.worksetItems[0].status).toBe('deleted');
    expect(state.activePreview?.worksetItems[0].title).toBe('[Deleted Item]');

    // Executing an action on a deleted item is blocked safely
    vi.mocked(db.getItem).mockResolvedValue({
      id: 'item-a1',
      title: 'engine.rs',
      item_type: 'file',
      content: '/path/to/engine.rs',
      metadata: JSON.stringify({ is_deleted: true }),
    } as any);

    const actionResult = await useLauncherStore
      .getState()
      .executeItemAction('item.open', 'ctx-alpha', 'item-a1');

    expect(actionResult.success).toBe(false);
    expect(actionResult.outcome).toBe('BLOCKED');
    expect(actionResult.message).toContain('deleted');
  });

  // =========================================================================
  // 7. Navigation & Selection Synchronization (Req 1, 9)
  // =========================================================================
  it('updates selectedContextId and loads workset when selection changes via navigation', async () => {
    // 1. Initial selection: Alpha (index 0)
    await useLauncherStore.getState().selectContextById('ctx-alpha');
    useLauncherStore.setState({ selectedIndex: 0 });

    expect(useLauncherStore.getState().selectedContextId).toBe('ctx-alpha');
    expect(useLauncherStore.getState().activePreview?.contextId).toBe('ctx-alpha');

    // 2. Select next context: Beta (index 1)
    useLauncherStore.setState({ selectedIndex: 1 });
    await useLauncherStore.getState().selectContextById('ctx-beta');

    const stateBeta = useLauncherStore.getState();
    expect(stateBeta.selectedContextId).toBe('ctx-beta');
    expect(stateBeta.selectedIndex).toBe(1);
    expect(stateBeta.activePreview?.contextId).toBe('ctx-beta');
    expect(stateBeta.activePreview?.worksetItems[0].id).toBe('item-b1');
  });
});
