import { describe, it, expect, vi, beforeEach } from 'vitest';
import { quickActionsService } from './quickActionsService';
import {
  getAvailableContextActions,
  validateContextActionTransition,
} from '../models/quickActions';
import {
  sortContextMatches,
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

describe('Context Pinning Architecture & Integration', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ws-active-1',
          name: 'Active Project',
          is_pinned: false,
          lifecycle_state: 'active',
          created_at: '2026-10-20T10:00:00Z',
          updated_at: '2026-10-20T11:00:00Z',
        },
        {
          id: 'ws-recent-2',
          name: 'Recent Research',
          is_pinned: true,
          lifecycle_state: 'dormant',
          created_at: '2026-10-20T10:05:00Z',
          updated_at: '2026-10-20T11:05:00Z',
        },
      ],
      activeWorkspaceId: 'ws-active-1',
      loadWorkspaces: vi.fn().mockResolvedValue(undefined),
      selectWorkspace: vi.fn(),
      openOverview: vi.fn(),
    });
  });

  // =========================================================================
  // 1. Contract & Availability
  // =========================================================================
  describe('Pinning Action Contract & Availability', () => {
    it('allows context.pin and context.unpin across all operational lifecycle states', () => {
      const states = ['active', 'paused', 'dormant', 'archived'] as const;
      for (const st of states) {
        expect(validateContextActionTransition('context.pin', st).isValid).toBe(true);
        expect(validateContextActionTransition('context.unpin', st).isValid).toBe(true);
      }
    });

    it('offers context.pin when isPinned is false or undefined with shortcut ⌥P', () => {
      const actions = getAvailableContextActions({
        contextId: 'ws-active-1',
        lifecycleState: 'active',
        isPinned: false,
      });

      const pinAction = actions.find((a) => a.id === 'context.pin');
      expect(pinAction).toBeDefined();
      expect(pinAction?.label).toBe('Pin Context');
      expect(pinAction?.shortcut).toBe('⌥P');
      expect(pinAction?.isSupported).toBe(true);
      expect(actions.some((a) => a.id === 'context.unpin')).toBe(false);
    });

    it('offers context.unpin when isPinned is true with shortcut ⌥P', () => {
      const actions = getAvailableContextActions({
        contextId: 'ws-recent-2',
        lifecycleState: 'dormant',
        isPinned: true,
      });

      const unpinAction = actions.find((a) => a.id === 'context.unpin');
      expect(unpinAction).toBeDefined();
      expect(unpinAction?.label).toBe('Unpin Context');
      expect(unpinAction?.shortcut).toBe('⌥P');
      expect(unpinAction?.isSupported).toBe(true);
      expect(actions.some((a) => a.id === 'context.pin')).toBe(false);
    });
  });

  // =========================================================================
  // 2. Persistence & Backend Service Integration
  // =========================================================================
  describe('Persistence Execution via db.setWorkspacePinned', () => {
    it('successfully executes context.pin, calls db.setWorkspacePinned(id, true), and updates workspaceStore', async () => {
      vi.mocked(db.setWorkspacePinned).mockResolvedValueOnce({
        id: 'ws-active-1',
        name: 'Active Project',
        is_pinned: true,
        lifecycle_state: 'active',
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-20T12:00:00Z',
      });

      const result = await quickActionsService.executeContextAction('context.pin', 'ws-active-1');

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');
      expect(db.setWorkspacePinned).toHaveBeenCalledWith('ws-active-1', true);

      const storeWorkspaces = useWorkspaceStore.getState().workspaces;
      const updated = storeWorkspaces.find((w) => w.id === 'ws-active-1');
      expect(updated?.is_pinned).toBe(true);
      expect(updated?.updated_at).toBe('2026-10-20T12:00:00Z');
    });

    it('successfully executes context.unpin, calls db.setWorkspacePinned(id, false), and updates workspaceStore', async () => {
      vi.mocked(db.setWorkspacePinned).mockResolvedValueOnce({
        id: 'ws-recent-2',
        name: 'Recent Research',
        is_pinned: false,
        lifecycle_state: 'dormant',
        created_at: '2026-10-20T10:05:00Z',
        updated_at: '2026-10-20T12:30:00Z',
      });

      const result = await quickActionsService.executeContextAction('context.unpin', 'ws-recent-2');

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');
      expect(db.setWorkspacePinned).toHaveBeenCalledWith('ws-recent-2', false);

      const storeWorkspaces = useWorkspaceStore.getState().workspaces;
      const updated = storeWorkspaces.find((w) => w.id === 'ws-recent-2');
      expect(updated?.is_pinned).toBe(false);
      expect(updated?.updated_at).toBe('2026-10-20T12:30:00Z');
    });
  });

  // =========================================================================
  // 3. Failure Handling & No False State Mutation
  // =========================================================================
  describe('Failure Recovery and Integrity Protection', () => {
    it('handles database persistence failure without corrupting displayed store state', async () => {
      vi.mocked(db.setWorkspacePinned).mockRejectedValueOnce(
        new Error('database is locked')
      );

      const result = await quickActionsService.executeContextAction('context.pin', 'ws-active-1');

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.error).toContain('database is locked');

      // State remains unchanged: is_pinned must still be false
      const storeWorkspaces = useWorkspaceStore.getState().workspaces;
      const ws = storeWorkspaces.find((w) => w.id === 'ws-active-1');
      expect(ws?.is_pinned).toBe(false);
    });

    it('provides descriptive action error titles for Pin and Unpin failures', () => {
      expect(getActionErrorTitle('context.pin')).toBe('Pin Failed');
      expect(getActionErrorTitle('context.unpin')).toBe('Unpin Failed');
    });

    it('handles unavailable or deleted contexts gracefully without crashing', async () => {
      vi.mocked(db.setWorkspacePinned).mockRejectedValueOnce(
        new Error('Workspace ws-nonexistent not found')
      );

      const result = await quickActionsService.executeContextAction('context.pin', 'ws-nonexistent');

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.error).toContain('not found');
    });
  });

  // =========================================================================
  // 4. Stable Identity & Relationships Preservation
  // =========================================================================
  describe('Stable Context Identity & Relationship Preservation', () => {
    it('preserves context stable ID, name, and lifecycle state when pinning/unpinning', async () => {
      const original = useWorkspaceStore.getState().workspaces.find((w) => w.id === 'ws-active-1')!;

      vi.mocked(db.setWorkspacePinned).mockResolvedValueOnce({
        ...original,
        is_pinned: true,
        updated_at: '2026-10-20T13:00:00Z',
      });

      await quickActionsService.executeContextAction('context.pin', 'ws-active-1');

      const afterPin = useWorkspaceStore.getState().workspaces.find((w) => w.id === 'ws-active-1')!;
      expect(afterPin.id).toBe(original.id);
      expect(afterPin.name).toBe(original.name);
      expect(afterPin.lifecycle_state).toBe(original.lifecycle_state);
      expect(afterPin.is_pinned).toBe(true);
    });
  });

  // =========================================================================
  // 5. Search Ranking & Navigational Priority
  // =========================================================================
  describe('Navigational Elevation in sortContextMatches', () => {
    it('elevates pinned contexts above unpinned contexts within the same ranking tier', () => {
      const items: LauncherContextItem[] = [
        {
          id: 'ctx-unpinned-1',
          name: 'Context Engine Documentation',
          isPinned: false,
          lastActiveAt: '2026-10-20T10:00:00Z',
        },
        {
          id: 'ctx-pinned-2',
          name: 'Context Engine Specifications',
          isPinned: true,
          lastActiveAt: '2026-10-19T10:00:00Z', // older timestamp
        },
      ];

      // Both match "Context" in Tier 2 (starts with)
      const sorted = sortContextMatches(items, 'Context');

      // Pinned context must be first despite being older
      expect(sorted[0].id).toBe('ctx-pinned-2');
      expect(sorted[1].id).toBe('ctx-unpinned-1');
    });

    it('preserves higher tier match above lower tier even if lower tier is pinned', () => {
      const items: LauncherContextItem[] = [
        {
          id: 'ctx-exact',
          name: 'Auth',
          isPinned: false, // exact match (Tier 1)
        },
        {
          id: 'ctx-prefix-pinned',
          name: 'Authentication Module',
          isPinned: true, // prefix match (Tier 2)
        },
      ];

      const sorted = sortContextMatches(items, 'Auth');
      expect(sorted[0].id).toBe('ctx-exact');
      expect(sorted[1].id).toBe('ctx-prefix-pinned');
    });
  });
});
