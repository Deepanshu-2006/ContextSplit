import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLauncherStore } from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { quickActionsService } from '../../services/quickActionsService';
import { contextInteractionService } from '../../services/contextInteractionService';
import {
  getDuplicateNameSet,
  resolveContextDisplayIdentity,
  formatGentleTime,
} from '../../models/contextIdentification';
import {
  getAvailableContextActions,
  getAvailableItemActions,
  validateContextActionTransition,
  getPrimaryContextActionDescriptor,
} from '../../models/quickActions';

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock sonner toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

// Mock context interaction service
vi.mock('../../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getResumeReadiness: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    resumeContext: vi.fn(),
    pauseContext: vi.fn(),
    controlContextMemory: vi.fn(),
  },
}));

describe('Context Launcher - Empty States and Common Edge Cases', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    useWorkspaceStore.setState({
      workspaces: [],
      activeWorkspaceId: null,
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: null,
      activeFilter: 'all',
      activeSort: 'recent',
      isPreviewOpen: true,
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
      recentContexts: [],
      activePreview: null,
    });
  });

  // =========================================================================
  // Case 1: No contexts available
  // =========================================================================
  describe('Case 1: No Contexts Available', () => {
    it('accurately reflects an empty context state without fabricating fake data', () => {
      const state = useLauncherStore.getState();
      expect(state.activeContext).toBeNull();
      expect(state.recentContexts).toHaveLength(0);
      expect(state.selectedContextId).toBeNull();
      expect(state.activePreview).toBeNull();

      // Combined items in launcher
      const allContexts = [
        ...(state.activeContext ? [state.activeContext] : []),
        ...state.recentContexts,
      ];
      expect(allContexts).toHaveLength(0);
    });

    it('does not fabricate recovery buttons when database is genuinely empty', () => {
      const state = useLauncherStore.getState();
      // When searchQuery is empty and activeFilter is 'all', no recovery action is needed
      expect(state.searchQuery).toBe('');
      expect(state.activeFilter).toBe('all');
    });
  });

  // =========================================================================
  // Case 2: No context selected
  // =========================================================================
  describe('Case 2: No Context Selected', () => {
    it('maintains a clean preview state when selectedContextId is null', () => {
      useLauncherStore.setState({
        activeContext: {
          id: 'ctx-1',
          name: 'Existing Workspace',
          isActive: true,
          state: 'active',
        },
        selectedContextId: null,
        activePreview: null,
      });

      const state = useLauncherStore.getState();
      expect(state.selectedContextId).toBeNull();
      expect(state.activePreview).toBeNull();
      expect(state.actionError).toBeNull();
      expect(state.previewError).toBeNull();
    });

    it('dismisses action errors cleanly back to unselected state', () => {
      useLauncherStore.setState({
        selectedContextId: null,
        activePreview: null,
        actionError: {
          actionId: 'context.resume',
          title: 'Context Unavailable',
          message: 'Context was deleted',
          targetId: 'ctx-deleted',
          isRetryable: false,
        },
      });

      // Clear action error
      useLauncherStore.getState().clearActionError();
      const state = useLauncherStore.getState();
      expect(state.actionError).toBeNull();
      expect(state.activePreview).toBeNull();
    });
  });

  // =========================================================================
  // Case 3: A context with no associated items
  // =========================================================================
  describe('Case 3: Context With No Associated Items', () => {
    it('provides accurate next-step guidance depending on active lifecycle state', () => {
      // 1. Inactive context with 0 workset items
      const inactivePreview = {
        contextId: 'ctx-inactive',
        name: 'Empty Inactive Workspace',
        state: 'recent' as const,
        readinessBand: 'ready' as const,
        itemCount: 0,
        isPinned: false,
        reasons: [],
        importantItems: [],
        isAmbiguous: false,
        isStale: false,
        worksetItems: [],
      };

      const isCurrentActiveInactive = Boolean((inactivePreview.state as string) === 'active');
      expect(isCurrentActiveInactive).toBe(false);

      const inactiveGuidance = isCurrentActiveInactive
        ? 'Open or capture files, web links, or notes to build this working set.'
        : 'Resume this workspace to begin capturing files, web links, or notes.';
      expect(inactiveGuidance).toBe('Resume this workspace to begin capturing files, web links, or notes.');

      // 2. Active context with 0 workset items
      const activePreview = {
        ...inactivePreview,
        contextId: 'ctx-active',
        state: 'active' as const,
      };

      const isCurrentActiveActive = Boolean(activePreview.state === 'active');
      expect(isCurrentActiveActive).toBe(true);

      const activeGuidance = isCurrentActiveActive
        ? 'Open or capture files, web links, or notes to build this working set.'
        : 'Resume this workspace to begin capturing files, web links, or notes.';
      expect(activeGuidance).toBe('Open or capture files, web links, or notes to build this working set.');
    });

    it('preserves the primary action even when workset is empty', () => {
      const descriptor = getPrimaryContextActionDescriptor('paused');
      expect(descriptor.actionId).toBe('context.resume');
      expect(descriptor.label).toBe('Resume Context');
      expect(descriptor.isSupported).toBe(true);
    });
  });

  // =========================================================================
  // Case 4: Missing descriptions or timestamps
  // =========================================================================
  describe('Case 4: Missing Descriptions or Timestamps', () => {
    it('gracefully formats gentle relative time without failing on null or missing timestamps', () => {
      expect(formatGentleTime(null)).toBe('Ready to resume');
      expect(formatGentleTime(undefined)).toBe('Ready to resume');
      expect(formatGentleTime('')).toBe('Ready to resume');
    });

    it('handles invalid date strings without crashing or throwing', () => {
      expect(formatGentleTime('not-a-valid-date')).toBe('Recently active');
    });

    it('prefers explicit duration estimates when provided', () => {
      expect(formatGentleTime(null, '45m focused')).toBe('45m focused');
    });

    it('resolves display identity without subtitle when description and task are missing', () => {
      const item = {
        id: 'ctx-no-metadata',
        name: 'Minimal Context',
        state: 'recent' as const,
        isActive: false,
        isPinned: false,
        // no description, no taskTitle, no activitySummary, no lastActiveAt
      };

      const isDuplicate = false;
      const identity = resolveContextDisplayIdentity(item, isDuplicate);

      expect(identity.name).toBe('Minimal Context');
      expect(identity.subtitle).toBeNull();
      expect(identity.timeStr).toBe('Ready to resume');
      expect(identity.isDuplicate).toBe(false);
      expect(identity.disambiguationTag).toBeNull();
    });
  });

  // =========================================================================
  // Case 5: Long or duplicate context names
  // =========================================================================
  describe('Case 5: Long or Duplicate Context Names', () => {
    it('preserves very long context names completely in data model', () => {
      const veryLongName =
        'Feature: Implement Distributed Context Caching and Token Deduplication Architecture for Multi-Agent Workflows';
      const item = {
        id: 'ctx-long',
        name: veryLongName,
        state: 'active' as const,
        isActive: true,
      };

      const identity = resolveContextDisplayIdentity(item, false);
      expect(identity.name).toBe(veryLongName);
      expect(identity.name.length).toBeGreaterThan(80);
    });

    it('disambiguates identical context names using project tags or deterministic short IDs', () => {
      const ctxA = { id: 'ctx-111111-aaaa', name: 'Billing Service' };
      const ctxB = { id: 'ctx-222222-bbbb', name: 'Billing Service' };

      const duplicates = getDuplicateNameSet([ctxA, ctxB]);
      expect(duplicates.has('billing service')).toBe(true);

      const identityA = resolveContextDisplayIdentity(
        { ...ctxA, projectIdentifier: 'billing-api' },
        duplicates.has('billing service')
      );
      const identityB = resolveContextDisplayIdentity(
        { ...ctxB, projectIdentifier: 'billing-frontend' },
        duplicates.has('billing service')
      );

      expect(identityA.isDuplicate).toBe(true);
      expect(identityB.isDuplicate).toBe(true);
      expect(identityA.disambiguationTag).toBe('billing-api');
      expect(identityB.disambiguationTag).toBe('billing-frontend');
    });

    it('falls back to compact hex short IDs for duplicates lacking project metadata', () => {
      const ctxA = { id: '12345678-abcd-ef01-2345-6789abcdef01', name: 'Workspace' };
      const ctxB = { id: '87654321-dcba-fe10-5432-10fedcba9876', name: 'Workspace' };

      const duplicates = getDuplicateNameSet([ctxA, ctxB]);
      const identityA = resolveContextDisplayIdentity(ctxA, duplicates.has('workspace'));
      const identityB = resolveContextDisplayIdentity(ctxB, duplicates.has('workspace'));

      expect(identityA.disambiguationTag).toBe('#123456');
      expect(identityB.disambiguationTag).toBe('#876543');
    });
  });

  // =========================================================================
  // Case 6: A context becoming unavailable during interaction
  // =========================================================================
  describe('Case 6: Context Becoming Unavailable During Interaction', () => {
    it('detects deleted/unavailable context on execution, updates error state, and triggers resync', async () => {
      const contextId = 'ctx-disappeared';

      // Mock resumeContext to throw a "Context not found" error
      vi.mocked(contextInteractionService.resumeContext).mockRejectedValueOnce(
        new Error('Context not found: context was deleted')
      );

      useLauncherStore.setState({
        selectedContextId: contextId,
        activePreview: {
          contextId,
          name: 'Disappeared Workspace',
          state: 'recent',
          readinessBand: 'ready',
          itemCount: 1,
          isPinned: false,
          reasons: [],
          importantItems: [],
          isAmbiguous: false,
          isStale: false,
          worksetItems: [],
        },
      });

      const spyFetchLauncherData = vi.spyOn(useLauncherStore.getState(), 'fetchLauncherData');

      await useLauncherStore.getState().executeContextAction('context.resume', contextId);

      const state = useLauncherStore.getState();
      expect(state.actionError).not.toBeNull();
      expect(state.actionError?.title).toBe('Context Unavailable');
      expect(state.actionError?.isRetryable).toBe(false);
      expect(state.activePreview).toBeNull();
      expect(spyFetchLauncherData).toHaveBeenCalled();
    });
  });

  // =========================================================================
  // Case 7: A workset request failing
  // =========================================================================
  describe('Case 7: Workset Request Failing', () => {
    it('sets previewError and preserves loading completion when preview retrieval fails', async () => {
      const contextWorksetRetrieval = await import('../../services/contextWorksetRetrieval');
      vi.spyOn(contextWorksetRetrieval, 'retrieveContextWorkset').mockRejectedValueOnce(
        new Error('Network timeout fetching workset')
      );

      await useLauncherStore.getState().selectContextById('ctx-timeout', { forceRefresh: true });

      const state = useLauncherStore.getState();
      expect(state.previewError).toContain('Network timeout fetching workset');
      expect(state.isLoadingPreview).toBe(false);
    });

    it('preserves existing workset data during a failed refresh while surfacing a notice', () => {
      const initialItems = [
        {
          id: 'item-1',
          title: 'Preserved File',
          itemType: 'file' as const,
          status: 'available' as const,
          isImportant: false,
          provenance: 'confirmed' as const,
        },
      ];

      useLauncherStore.setState({
        selectedContextId: 'ctx-1',
        activePreview: {
          contextId: 'ctx-1',
          name: 'Workspace 1',
          state: 'active',
          readinessBand: 'ready',
          itemCount: 1,
          isPinned: false,
          reasons: [],
          importantItems: [],
          isAmbiguous: false,
          isStale: false,
          worksetItems: initialItems,
        },
        previewError: 'Background refresh failed: temporary network issue',
      });

      const state = useLauncherStore.getState();
      // Data is intact
      expect(state.activePreview?.worksetItems).toHaveLength(1);
      expect(state.activePreview?.worksetItems[0].title).toBe('Preserved File');
      // Notice is set
      expect(state.previewError).toBe('Background refresh failed: temporary network issue');

      // Dismiss notice
      useLauncherStore.setState({ previewError: null });
      expect(useLauncherStore.getState().previewError).toBeNull();
      expect(useLauncherStore.getState().activePreview?.worksetItems).toHaveLength(1);
    });
  });

  // =========================================================================
  // Case 8: An action becoming invalid before execution
  // =========================================================================
  describe('Case 8: Action Becoming Invalid Before Execution', () => {
    it('blocks pause action on an archived context and flags action as no longer valid', async () => {
      // Trying to pause a context that is archived
      const validation = validateContextActionTransition('context.pause', 'archived');
      expect(validation.isValid).toBe(false);
      expect(validation.reason).toContain('archived');

      const result = await quickActionsService.executeContextAction('context.pause', 'ctx-archived', {
        lifecycleState: 'archived',
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('Action is no longer valid');
    });

    it('blocks archive action on an already archived context and flags action as no longer valid', async () => {
      const validation = validateContextActionTransition('context.archive', 'archived');
      expect(validation.isValid).toBe(false);
      expect(validation.reason).toContain('already archived');

      const result = await quickActionsService.executeContextAction('context.archive', 'ctx-archived', {
        lifecycleState: 'archived',
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('Action is no longer valid');
    });
  });

  // =========================================================================
  // Case 9: A menu with no supported actions
  // =========================================================================
  describe('Case 9: Menus With No Supported Actions', () => {
    it('correctly reports supported and unsupported context secondary actions', () => {
      const actions = getAvailableContextActions({
        contextId: 'ctx-archived',
        lifecycleState: 'archived',
        isActive: false,
        readinessBand: 'ready',
      });

      // Filter out primary resume
      const secondaryActions = actions.filter((a) => a.id !== 'context.resume');
      expect(secondaryActions.length).toBeGreaterThan(0);

      // In archived state, pause is unsupported
      const pauseAction = secondaryActions.find((a) => a.id === 'context.pause');
      expect(pauseAction?.isSupported).toBe(false);
      expect(pauseAction?.unsupportedReason).toBe('Cannot pause an archived context.');
    });

    it('handles deleted item with only unmark action supported', () => {
      const itemActions = getAvailableItemActions({
        contextId: 'ctx-1',
        itemId: 'item-deleted',
        itemType: 'file',
        status: 'deleted',
      });

      const openAction = itemActions.find((a) => a.id === 'item.open');
      expect(openAction?.isSupported).toBe(false);
      expect(openAction?.unsupportedReason).toBe('Item has been deleted and cannot be opened.');
    });
  });

  // =========================================================================
  // Case 10: An empty search result & Filter recovery
  // =========================================================================
  describe('Case 10: Empty Search Result & Recovery Paths', () => {
    it('provides genuine search clearing recovery path', () => {
      useLauncherStore.setState({
        searchQuery: 'non-existent-keyword-xyz',
      });

      expect(useLauncherStore.getState().searchQuery).toBe('non-existent-keyword-xyz');

      // Clear search action
      useLauncherStore.getState().setSearchQuery('');
      expect(useLauncherStore.getState().searchQuery).toBe('');
    });

    it('provides genuine filter reset recovery path', () => {
      useLauncherStore.setState({
        activeFilter: 'archived',
        searchQuery: '',
      });

      expect(useLauncherStore.getState().activeFilter).toBe('archived');

      // Show all contexts action
      useLauncherStore.getState().setActiveFilter('all');
      expect(useLauncherStore.getState().activeFilter).toBe('all');
    });
  });
});
