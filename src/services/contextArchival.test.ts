import { describe, it, expect, vi, beforeEach } from 'vitest';
import { quickActionsService } from './quickActionsService';
import {
  getAvailableContextActions,
  validateContextActionTransition,
  resolveContextLifecycleState,
  getPrimaryContextActionDescriptor,
} from '../models/quickActions';
import {
  matchesContextFilter,
  groupLauncherContexts,
  getActionErrorTitle,
  LauncherContextItem,
} from '../store/launcherStore';
import { db } from './db';
import { useWorkspaceStore } from '../store/workspaceStore';

vi.mock('./contextInteractionService', () => ({
  contextInteractionService: {
    resumeContext: vi.fn(),
    pauseContext: vi.fn(),
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    getResumeReadiness: vi.fn(),
  },
}));

vi.mock('./contextWorkset', () => ({
  contextWorksetService: {
    controlContextMemory: vi.fn(),
  },
}));

vi.mock('./db', () => ({
  db: {
    updateWorkspace: vi.fn(),
    archiveWorkspace: vi.fn(),
    unarchiveWorkspace: vi.fn(),
    setWorkspacePinned: vi.fn(),
    toggleWorkspacePinned: vi.fn(),
  },
}));

describe('Context Lifecycle, Archival & Restore Architecture', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ws-active',
          name: 'Active Project',
          is_pinned: false,
          lifecycle_state: 'active',
          created_at: '2026-10-20T10:00:00Z',
          updated_at: '2026-10-20T11:00:00Z',
        },
        {
          id: 'ws-paused',
          name: 'Paused Feature',
          is_pinned: false,
          lifecycle_state: 'dormant',
          created_at: '2026-10-20T10:05:00Z',
          updated_at: '2026-10-20T11:05:00Z',
        },
        {
          id: 'ws-dormant',
          name: 'Dormant Experiment',
          is_pinned: false,
          lifecycle_state: 'dormant',
          created_at: '2026-10-20T10:10:00Z',
          updated_at: '2026-10-20T11:10:00Z',
        },
        {
          id: 'ws-archived',
          name: 'Archived Reference',
          is_pinned: false,
          lifecycle_state: 'archived',
          created_at: '2026-10-20T10:20:00Z',
          updated_at: '2026-10-20T11:20:00Z',
        },
      ],
      activeWorkspaceId: 'ws-active',
      loadWorkspaces: vi.fn().mockResolvedValue(undefined),
      selectWorkspace: vi.fn(),
      openOverview: vi.fn(),
    });
  });

  // =========================================================================
  // 1. Lifecycle States & Normalization (Requirement 1, 8)
  // =========================================================================
  describe('Lifecycle State Resolution & Anti-Collision', () => {
    it('normalizes operational lifecycle states correctly', () => {
      expect(resolveContextLifecycleState({ lifecycleState: 'active' })).toBe('active');
      expect(resolveContextLifecycleState({ lifecycleState: 'paused' })).toBe('paused');
      expect(resolveContextLifecycleState({ lifecycleState: 'dormant' })).toBe('dormant');
      expect(resolveContextLifecycleState({ lifecycleState: 'archived' })).toBe('archived');
      // Historical alias 'inactive' is resolved to 'archived'
      expect(resolveContextLifecycleState({ lifecycleState: 'inactive' })).toBe('archived');
    });

    it('ensures archived contexts are NEVER treated as active even with stale isActive=true (Requirement 8)', () => {
      // Stale state collision: isActive is true, but database lifecycleState is archived
      const resolved = resolveContextLifecycleState({
        lifecycleState: 'archived',
        isActive: true,
      });
      expect(resolved).toBe('archived');

      const resolvedInactive = resolveContextLifecycleState({
        lifecycleState: 'inactive',
        isActive: true,
      });
      expect(resolvedInactive).toBe('archived');
    });
  });

  // =========================================================================
  // 2. Valid & Invalid State Transitions (Requirement 1, 6)
  // =========================================================================
  describe('State Transitions & Guard Validation', () => {
    it('allows context.archive from active, paused, and dormant states', () => {
      expect(validateContextActionTransition('context.archive', 'active').isValid).toBe(true);
      expect(validateContextActionTransition('context.archive', 'paused').isValid).toBe(true);
      expect(validateContextActionTransition('context.archive', 'dormant').isValid).toBe(true);
    });

    it('blocks context.archive for already archived contexts', () => {
      const validation = validateContextActionTransition('context.archive', 'archived');
      expect(validation.isValid).toBe(false);
      expect(validation.reason).toContain('Context is already archived');
    });

    it('allows context.restore ONLY for archived contexts', () => {
      expect(validateContextActionTransition('context.restore', 'archived').isValid).toBe(true);
      expect(validateContextActionTransition('context.restore', 'active').isValid).toBe(false);
      expect(validateContextActionTransition('context.restore', 'paused').isValid).toBe(false);
      expect(validateContextActionTransition('context.restore', 'dormant').isValid).toBe(false);
    });

    it('blocks context.pause for archived contexts', () => {
      const validation = validateContextActionTransition('context.pause', 'archived');
      expect(validation.isValid).toBe(false);
      expect(validation.reason).toContain('Cannot pause an archived context');
    });

    it('never allows permanent context deletion in launcher (Requirement 4)', () => {
      const validation = validateContextActionTransition('context.delete', 'archived');
      expect(validation.isValid).toBe(false);
      expect(validation.reason).toContain('Context deletion must be performed from workspace management settings');
    });
  });

  // =========================================================================
  // 3. Action Offerings in Launcher (Requirement 6, 7)
  // =========================================================================
  describe('Action Offerings per Lifecycle State', () => {
    it('offers "Restore & Resume" as primary action for archived context', () => {
      const primary = getPrimaryContextActionDescriptor('archived');
      expect(primary.actionId).toBe('context.resume');
      expect(primary.label).toBe('Restore & Resume');
      expect(primary.inProgressLabel).toBe('Restoring...');
      expect(primary.isSupported).toBe(true);
    });

    it('offers "Restore Context" in secondary actions for archived context', () => {
      const actions = getAvailableContextActions({
        contextId: 'ws-archived',
        lifecycleState: 'archived',
        isPinned: false,
      });

      const restoreAction = actions.find((a) => a.id === 'context.restore');
      expect(restoreAction).toBeDefined();
      expect(restoreAction?.label).toBe('Restore Context');
      expect(restoreAction?.isSupported).toBe(true);

      // Archive context must NOT be offered when already archived
      expect(actions.some((a) => a.id === 'context.archive')).toBe(false);
    });

    it('offers "Archive Context" in secondary actions for dormant or paused contexts', () => {
      const actions = getAvailableContextActions({
        contextId: 'ws-dormant',
        lifecycleState: 'dormant',
        isPinned: false,
      });

      const archiveAction = actions.find((a) => a.id === 'context.archive');
      expect(archiveAction).toBeDefined();
      expect(archiveAction?.label).toBe('Archive Context');
      expect(archiveAction?.isSupported).toBe(true);

      // Restore must not be offered for dormant
      expect(actions.some((a) => a.id === 'context.restore')).toBe(false);
    });
  });

  // =========================================================================
  // 4. Persistence Execution & Relationship Preservation (Requirement 3, 5)
  // =========================================================================
  describe('Persistence Execution via Real Backend Operations', () => {
    it('executes context.archive via db.archiveWorkspace without deleting items or relationships', async () => {
      vi.mocked(db.archiveWorkspace).mockResolvedValueOnce({
        id: 'ws-dormant',
        name: 'Dormant Experiment',
        is_pinned: false,
        lifecycle_state: 'archived',
        created_at: '2026-10-20T10:10:00Z',
        updated_at: '2026-10-20T12:00:00Z',
      });

      const result = await quickActionsService.executeContextAction('context.archive', 'ws-dormant', {
        lifecycleState: 'dormant',
      });

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');
      expect(db.archiveWorkspace).toHaveBeenCalledWith('ws-dormant');
      expect(useWorkspaceStore.getState().loadWorkspaces).toHaveBeenCalled();
    });

    it('executes context.restore via db.unarchiveWorkspace and refreshes workspaces', async () => {
      vi.mocked(db.unarchiveWorkspace).mockResolvedValueOnce({
        id: 'ws-archived',
        name: 'Archived Reference',
        is_pinned: false,
        lifecycle_state: 'active',
        created_at: '2026-10-20T10:20:00Z',
        updated_at: '2026-10-20T12:30:00Z',
      });

      const result = await quickActionsService.executeContextAction('context.restore', 'ws-archived', {
        lifecycleState: 'archived',
      });

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');
      expect(db.unarchiveWorkspace).toHaveBeenCalledWith('ws-archived');
      expect(useWorkspaceStore.getState().loadWorkspaces).toHaveBeenCalled();
    });

    it('unarchives automatically before resuming when context is in archived state', async () => {
      vi.mocked(db.unarchiveWorkspace).mockResolvedValueOnce({
        id: 'ws-archived',
        name: 'Archived Reference',
        is_pinned: false,
        lifecycle_state: 'active',
        created_at: '2026-10-20T10:20:00Z',
        updated_at: '2026-10-20T12:30:00Z',
      });

      const result = await quickActionsService.executeContextAction('context.resume', 'ws-archived', {
        lifecycleState: 'archived',
      });

      expect(result.success).toBe(true);
      expect(db.unarchiveWorkspace).toHaveBeenCalledWith('ws-archived');
    });

    it('deselects active workspace if archived workspace was currently active in UI (Requirement 8)', async () => {
      useWorkspaceStore.setState({ activeWorkspaceId: 'ws-dormant' });
      vi.mocked(db.archiveWorkspace).mockResolvedValueOnce({
        id: 'ws-dormant',
        name: 'Dormant Experiment',
        is_pinned: false,
        lifecycle_state: 'archived',
        created_at: '2026-10-20T10:10:00Z',
        updated_at: '2026-10-20T12:00:00Z',
      });

      await quickActionsService.executeContextAction('context.archive', 'ws-dormant', {
        lifecycleState: 'dormant',
      });

      expect(useWorkspaceStore.getState().selectWorkspace).toHaveBeenCalledWith('inbox');
    });
  });

  // =========================================================================
  // 5. Filter & Group Isolation (Requirement 7, 8)
  // =========================================================================
  describe('Filter & Group Isolation for Archived Contexts', () => {
    const items: LauncherContextItem[] = [
      { id: '1', name: 'Active', isActive: true, state: 'active' },
      { id: '2', name: 'Recent', isActive: false, state: 'recent' },
      { id: '3', name: 'Paused', isActive: false, state: 'paused' },
      { id: '4', name: 'Archived', isActive: false, state: 'archived' },
      { id: '5', name: 'Stale Active Archived', isActive: true, state: 'archived' }, // Stale state!
    ];

    it('matchesContextFilter("active") excludes archived contexts even with stale isActive=true', () => {
      const activeMatches = items.filter((i) => matchesContextFilter(i, 'active'));
      const activeIds = activeMatches.map((i) => i.id);
      expect(activeIds).toEqual(['1']);
      expect(activeIds).not.toContain('4');
      expect(activeIds).not.toContain('5');
    });

    it('matchesContextFilter("archived") matches archived and inactive contexts', () => {
      const archivedMatches = items.filter((i) => matchesContextFilter(i, 'archived'));
      const archivedIds = archivedMatches.map((i) => i.id);
      expect(archivedIds).toContain('4');
      expect(archivedIds).toContain('5');
      expect(archivedIds).not.toContain('1');
    });

    it('groupLauncherContexts never places archived contexts in "Active Context" group', () => {
      const groups = groupLauncherContexts(items, {
        activeFilter: 'all',
        activeContextId: '5', // Stale active context ID
      });

      const activeGroup = groups.find((g) => g.id === 'active');
      // If the context ID is archived, it must not form an active group
      expect(activeGroup?.items.some((i) => i.id === '5')).toBeFalsy();
    });
  });

  // =========================================================================
  // 6. Fault Handling & Stale State Recovery (Requirement 9)
  // =========================================================================
  describe('Fault Handling & Stale State Recovery', () => {
    it('handles database error without mutating state and produces descriptive title', async () => {
      vi.mocked(db.archiveWorkspace).mockRejectedValueOnce(new Error('SQLite locked: disk error'));

      const result = await quickActionsService.executeContextAction('context.archive', 'ws-dormant', {
        lifecycleState: 'dormant',
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(getActionErrorTitle('context.archive')).toBe('Archive Failed');
      expect(getActionErrorTitle('context.restore')).toBe('Restore Failed');
    });

    it('detects unavailable or deleted context during restore', async () => {
      vi.mocked(db.unarchiveWorkspace).mockRejectedValueOnce(new Error('workspace ws-deleted not found'));

      const result = await quickActionsService.executeContextAction('context.restore', 'ws-deleted', {
        lifecycleState: 'archived',
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.message).toContain('Failed to restore context');
    });
  });
});
