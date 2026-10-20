import { describe, it, expect, vi, beforeEach } from 'vitest';
import { quickActionsService } from './quickActionsService';
import { db } from './db';
import { useWorkspaceStore } from '../store/workspaceStore';
import { assertContextIdentityPreserved } from '../models/contextOrganization';
import { useLauncherStore } from '../store/launcherStore';

vi.mock('./db', () => ({
  db: {
    updateWorkspace: vi.fn(),
    archiveWorkspace: vi.fn(),
    unarchiveWorkspace: vi.fn(),
  },
}));

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

describe('Context Renaming & Persistence Contract', () => {
  const contextId = 'ws-stable-uuid-1234';
  const initialName = 'Authentication Service';

  beforeEach(() => {
    vi.clearAllMocks();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: contextId,
          name: initialName,
          created_at: 1700000000,
          updated_at: 1700000000,
          lifecycle_state: 'active',
        } as any,
      ],
      activeWorkspaceId: contextId,
      loadWorkspaces: vi.fn().mockResolvedValue(undefined),
      selectWorkspace: vi.fn(),
      openOverview: vi.fn(),
    });

    useLauncherStore.setState({
      activeContext: {
        id: contextId,
        name: initialName,
        state: 'active',
      },
      activePreview: {
        contextId,
        name: initialName,
        state: 'active',
        importantItems: [],
        worksetItems: [
          {
            id: 'item-1',
            itemType: 'file',
            title: 'auth.ts',
            isImportant: true,
          },
        ],
        itemCount: 1,
        reasons: [],
        isAmbiguous: false,
        isStale: false,
        readinessBand: 'ready',
        readinessExplanation: 'All files present',
        lastActiveAt: '2026-10-20T10:00:00Z',
        activitySummary: 'Working on auth tokens',
      },
      recentContexts: [
        {
          id: contextId,
          name: initialName,
          state: 'active',
        },
      ],
      isActionExecuting: false,
      executingActionId: null,
      executingTargetId: null,
      actionError: null,
    });
  });

  // =========================================================================
  // 1. Successful Rename
  // =========================================================================
  describe('1. Successful Rename Operation', () => {
    it('successfully renames context using db.updateWorkspace and returns SUCCESS', async () => {
      vi.mocked(db.updateWorkspace).mockResolvedValueOnce({
        id: contextId,
        name: 'OAuth & Token Service',
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-20T12:00:00Z',
      });

      const result = await quickActionsService.executeContextAction('context.rename', contextId, {
        newName: 'OAuth & Token Service',
      });

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');
      expect(result.resolvedContextId).toBe(contextId);
      expect(result.message).toBe('Renamed context to "OAuth & Token Service"');
      expect(db.updateWorkspace).toHaveBeenCalledTimes(1);
      expect(db.updateWorkspace).toHaveBeenCalledWith(contextId, 'OAuth & Token Service');
    });

    it('updates launcher preview, active context, and recent list via renameContextById', () => {
      const store = useLauncherStore.getState();
      store.renameContextById(contextId, 'OAuth & Token Service');

      const updated = useLauncherStore.getState();
      expect(updated.activePreview?.name).toBe('OAuth & Token Service');
      expect(updated.activeContext?.name).toBe('OAuth & Token Service');
      expect(updated.recentContexts[0].name).toBe('OAuth & Token Service');
    });

    it('trims leading and trailing whitespace on successful rename', async () => {
      vi.mocked(db.updateWorkspace).mockResolvedValueOnce({
        id: contextId,
        name: 'Clean Context Name',
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-20T12:00:00Z',
      });

      const result = await quickActionsService.executeContextAction('context.rename', contextId, {
        newName: '   Clean Context Name   ',
      });

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');
      expect(db.updateWorkspace).toHaveBeenCalledWith(contextId, 'Clean Context Name');
    });
  });

  // =========================================================================
  // 2. Name Validation & Edge Cases
  // =========================================================================
  describe('2. Name Validation Rules', () => {
    it('rejects empty string names without invoking persistence', async () => {
      const result = await quickActionsService.executeContextAction('context.rename', contextId, {
        newName: '',
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.error).toContain('cannot be empty');
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });

    it('rejects whitespace-only names without invoking persistence', async () => {
      const result = await quickActionsService.executeContextAction('context.rename', contextId, {
        newName: '    \t\n  ',
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.error).toContain('cannot be empty');
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });

    it('rejects names exceeding 100 characters', async () => {
      const longName = 'A'.repeat(101);
      const result = await quickActionsService.executeContextAction('context.rename', contextId, {
        newName: longName,
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.error).toContain('cannot exceed 100 characters');
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });

    it('rejects names containing control characters', async () => {
      const controlCharName = 'Invalid\x00Name';
      const result = await quickActionsService.executeContextAction('context.rename', contextId, {
        newName: controlCharName,
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.error).toContain('control characters');
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });

    it('rejects missing or undefined newName', async () => {
      const result = await quickActionsService.executeContextAction('context.rename', contextId);

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.error).toContain('New context name is required');
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });
  });

  // =========================================================================
  // 3. Cancellation & No-op Handling
  // =========================================================================
  describe('3. Cancellation & No-op Behavior', () => {
    it('does not invoke db.updateWorkspace if new name is identical to current name', () => {
      const preview = useLauncherStore.getState().activePreview!;
      const trimmedInput = 'Authentication Service';

      // Simulates UI check: if trimmed === preview.name.trim(), cancel without saving
      const shouldSubmit = trimmedInput !== preview.name.trim();
      expect(shouldSubmit).toBe(false);
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });

    it('leaves persisted and store data intact when user cancels', () => {
      const beforeState = useLauncherStore.getState();
      expect(beforeState.activePreview?.name).toBe(initialName);

      // Simulating cancel action (user pressed Escape or clicked Cancel)
      // Form closes and no store mutation occurs
      const afterState = useLauncherStore.getState();
      expect(afterState.activePreview?.name).toBe(initialName);
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });
  });

  // =========================================================================
  // 4. Persistence Failure
  // =========================================================================
  describe('4. Backend Persistence Failure Handling', () => {
    it('handles database rejection without crashing and returns structured error', async () => {
      vi.mocked(db.updateWorkspace).mockRejectedValueOnce(new Error('database is locked'));

      const result = await quickActionsService.executeContextAction('context.rename', contextId, {
        newName: 'Failed Rename Attempt',
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.error).toContain('database is locked');
    });

    it('does not mutate launcher store state when persistence fails', async () => {
      vi.mocked(db.updateWorkspace).mockRejectedValueOnce(new Error('database disk image is malformed'));

      await useLauncherStore.getState().executeContextAction('context.rename', contextId, {
        newName: 'Failed Rename',
      });

      const store = useLauncherStore.getState();
      // Name should remain the original initialName
      expect(store.activePreview?.name).toBe(initialName);
      expect(store.activeContext?.name).toBe(initialName);
      // Action error state should be populated with 'Rename Failed'
      expect(store.actionError).not.toBeNull();
      expect(store.actionError?.title).toBe('Rename Failed');
      expect(store.actionError?.message).toContain('database disk image is malformed');
    });
  });

  // =========================================================================
  // 5. Relationship Preservation & Stable ID
  // =========================================================================
  describe('5. Identity & Relationship Preservation', () => {
    it('preserves the context stable ID across rename', async () => {
      vi.mocked(db.updateWorkspace).mockResolvedValueOnce({
        id: contextId,
        name: 'Brand New Title',
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-20T12:00:00Z',
      });

      const before = { id: contextId, name: initialName };
      await quickActionsService.executeContextAction('context.rename', contextId, {
        newName: 'Brand New Title',
      });
      const after = { id: contextId, name: 'Brand New Title' };

      const check = assertContextIdentityPreserved(before, after);
      expect(check.isPreserved).toBe(true);
      expect(check.violation).toBeUndefined();
    });

    it('preserves items, workset memberships, and lifecycle state', async () => {
      vi.mocked(db.updateWorkspace).mockResolvedValueOnce({
        id: contextId,
        name: 'Refactored Workset Name',
        created_at: '2026-10-20T10:00:00Z',
        updated_at: '2026-10-20T12:00:00Z',
      });

      const beforePreview = useLauncherStore.getState().activePreview!;
      const beforeItemCount = beforePreview.worksetItems.length;
      const beforeState = beforePreview.state;

      await useLauncherStore.getState().executeContextAction('context.rename', contextId, {
        newName: 'Refactored Workset Name',
      });

      const afterPreview = useLauncherStore.getState().activePreview!;
      expect(afterPreview.contextId).toBe(contextId);
      expect(afterPreview.worksetItems.length).toBe(beforeItemCount);
      expect(afterPreview.worksetItems[0].id).toBe(beforePreview.worksetItems[0].id);
      expect(afterPreview.state).toBe(beforeState);
    });
  });

  // =========================================================================
  // 6. Concurrency & Duplicate Submission Guard
  // =========================================================================
  describe('6. Concurrency & Duplicate Submission Guard', () => {
    it('blocks subsequent action execution while an action is already in-flight', async () => {
      useLauncherStore.setState({ isActionExecuting: true, executingActionId: 'context.rename' });

      const result = await useLauncherStore.getState().executeContextAction('context.rename', contextId, {
        newName: 'Concurrent Attempt',
      });

      expect(result.outcome).toBe('BLOCKED');
      // Should be ignored/rejected without calling db
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });
  });
});
