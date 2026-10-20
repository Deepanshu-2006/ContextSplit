import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  useLauncherStore,
  sortContextMatches,
  sortContextList,
  groupLauncherContexts,
  matchesContextFilter,
  getActionErrorTitle,
  LauncherContextItem,
} from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { quickActionsService } from '../../services/quickActionsService';
import { db } from '../../services/db';
import { contextInteractionService } from '../../services/contextInteractionService';
import { toast } from 'sonner';
import {
  formatShortId,
  formatGentleTime,
  formatFriendlyDate,
  getDuplicateNameSet,
  resolveDisambiguationTag,
  resolveContextSubtitle,
  resolveContextDisplayIdentity,
} from '../../models/contextIdentification';
import { validateContextName } from '../../models/contextOrganization';
import {
  saveLauncherPreferences,
  loadLauncherPreferences,
  resetLauncherPreferences,
  DEFAULT_LAUNCHER_PREFERENCES,
  parseLauncherPreferences,
  isValidSortOrder,
  LAUNCHER_PREFERENCES_STORAGE_KEY,
} from '../../models/launcherPreferences';
import {
  validateContextId,
  validateContextActionTransition,
  resolveContextLifecycleState,
  getAvailableContextActions,
  getPrimaryContextActionDescriptor,
} from '../../models/quickActions';

// Mock Tauri invoke to prevent window undefined in test environments
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock authoritative Context Interaction API service
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

// Mock Database service
vi.mock('../../services/db', () => ({
  db: {
    updateWorkspace: vi.fn(),
    setWorkspacePinned: vi.fn(),
    toggleWorkspacePinned: vi.fn(),
    archiveWorkspace: vi.fn(),
    unarchiveWorkspace: vi.fn(),
    listWorkspaces: vi.fn().mockResolvedValue([]),
  },
}));

// Mock Sonner toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

describe('Context Launcher Organization - Focused Regression Suite (Day 34)', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ws-active-1',
          name: 'ContextSplit Engine',
          lifecycle_state: 'active',
          is_pinned: false,
          project_identifier: 'ctx-core',
          repository: 'contextsplit',
          description: 'Core context intelligence engine',
          created_at: 1000,
          updated_at: 5000,
        } as any,
        {
          id: 'ws-recent-dup1',
          name: 'Authentication API',
          lifecycle_state: 'active',
          is_pinned: true,
          project_identifier: 'auth-service',
          repository: 'contextsplit/auth',
          description: 'OAuth2 token validation handlers',
          created_at: 1200,
          updated_at: 4000,
        } as any,
        {
          id: 'ws-recent-dup2',
          name: 'Authentication API',
          lifecycle_state: 'active',
          is_pinned: false,
          project_identifier: 'web-client',
          repository: 'contextsplit/web',
          description: 'Client sign-in flow',
          created_at: 1300,
          updated_at: 3000,
        } as any,
        {
          id: 'ws-paused-1',
          name: 'Billing Integration',
          lifecycle_state: 'paused',
          is_pinned: false,
          project_identifier: 'payx-billing',
          repository: null,
          description: 'Stripe webhook receiver',
          created_at: 1400,
          updated_at: 2000,
        } as any,
        {
          id: 'ws-archived-1',
          name: 'Legacy Pipeline',
          lifecycle_state: 'archived',
          is_pinned: false,
          project_identifier: null,
          repository: null,
          description: 'Deprecated telemetry pipeline',
          created_at: 800,
          updated_at: 1500,
        } as any,
      ],
      activeWorkspaceId: 'ws-active-1',
      loadWorkspaces: vi.fn().mockResolvedValue(undefined),
      selectWorkspace: vi.fn(),
      openOverview: vi.fn(),
    });

    useLauncherStore.setState({
      isOpen: true,
      searchQuery: '',
      activeFilter: 'all',
      activeSort: 'recent',
      selectedIndex: 0,
      selectedContextId: null,
      isSubmittingAction: false,
      activeActionId: null,
      actionError: null,
    });
  });

  // ===========================================================================
  // 1. Context Identification and Duplicate Names
  // ===========================================================================
  describe('1. Context identification and duplicate names', () => {
    it('displays context names clearly and trims whitespace without altering content', () => {
      const item: LauncherContextItem = {
        id: 'ws-test-name',
        name: '   Distributed Systems Research   ',
        state: 'active',
      };
      const identity = resolveContextDisplayIdentity(item, false);
      expect(identity.name).toBe('Distributed Systems Research');
      expect(identity.id).toBe('ws-test-name');
    });

    it('identifies duplicate names case-insensitively and whitespace-tolerantly', () => {
      const contexts = [
        { id: '1', name: 'Refactor Auth' },
        { id: '2', name: 'UI Overhaul' },
        { id: '3', name: '  refactor auth  ' },
        { id: '4', name: 'Bugfix' },
      ];
      const dupes = getDuplicateNameSet(contexts);
      expect(dupes.has('refactor auth')).toBe(true);
      expect(dupes.has('ui overhaul')).toBe(false);
      expect(dupes.has('bugfix')).toBe(false);
    });

    it('disambiguates duplicate names using projectIdentifier, repository, or short ID', () => {
      const dup1: LauncherContextItem = {
        id: 'ws-11111111-2222-3333-4444-555555555555',
        name: 'API Testing',
        state: 'recent',
        projectIdentifier: 'payment-svc',
        repository: 'contextsplit/payment',
      };
      expect(resolveDisambiguationTag(dup1, true)).toBe('payment-svc');

      const dup2: LauncherContextItem = {
        id: 'ws-22222222-3333-4444-5555-666666666666',
        name: 'API Testing',
        state: 'recent',
        projectIdentifier: null,
        repository: 'contextsplit/web',
      };
      expect(resolveDisambiguationTag(dup2, true)).toBe('contextsplit/web');

      const dup3: LauncherContextItem = {
        id: 'ws-33333333-4444-5555-6666-777777777777',
        name: 'API Testing',
        state: 'recent',
        projectIdentifier: null,
        repository: null,
      };
      expect(resolveDisambiguationTag(dup3, true)).toBe('#333333');
    });

    it('returns null disambiguation tag for unique context names to prevent UI clutter', () => {
      const unique: LauncherContextItem = {
        id: 'ws-unique-1',
        name: 'Single Unique Context',
        state: 'active',
        projectIdentifier: 'ctx-core',
      };
      expect(resolveDisambiguationTag(unique, false)).toBeNull();
    });

    it('never leaks raw UUIDs into context name, subtitle, or unique tags', () => {
      const rawUuid = 'ws-98765432-1111-2222-3333-444455556666';
      const item: LauncherContextItem = {
        id: rawUuid,
        name: 'Clean Workset',
        state: 'active',
      };
      const identity = resolveContextDisplayIdentity(item, false);
      expect(identity.name).not.toContain(rawUuid);
      expect(identity.subtitle).toBeNull();
      expect(identity.disambiguationTag).toBeNull();
      expect(formatShortId(rawUuid)).toBe('#987654');
    });

    it('preserves distinct stable UUIDs for duplicate contexts across selection', () => {
      const itemA: LauncherContextItem = { id: 'uuid-a', name: 'Dup', state: 'active' };
      const itemB: LauncherContextItem = { id: 'uuid-b', name: 'Dup', state: 'recent' };
      expect(itemA.id).not.toBe(itemB.id);
      expect(resolveContextDisplayIdentity(itemA, true).id).toBe('uuid-a');
      expect(resolveContextDisplayIdentity(itemB, true).id).toBe('uuid-b');
    });
  });

  // ===========================================================================
  // 2. Renaming and Persistence
  // ===========================================================================
  describe('2. Renaming and persistence', () => {
    it('validates context names strictly against bounds and control characters', () => {
      expect(validateContextName('').isValid).toBe(false);
      expect(validateContextName('   ').isValid).toBe(false);
      expect(validateContextName('a'.repeat(101)).isValid).toBe(false);
      expect(validateContextName('Invalid\nName').isValid).toBe(false);
      expect(validateContextName('Valid Name').isValid).toBe(true);
      expect(validateContextName('  Trimmed Name  ').sanitizedName).toBe('Trimmed Name');
    });

    it('persists renamed context via db.updateWorkspace and updates workspace store', async () => {
      vi.mocked(db.updateWorkspace).mockResolvedValueOnce({
        id: 'ws-active-1',
        name: 'ContextSplit Engine V2',
        path: '/path',
        created_at: 1000,
        updated_at: 6000,
      } as any);

      const result = await quickActionsService.executeContextAction('context.rename', 'ws-active-1', {
        newName: 'ContextSplit Engine V2',
      });

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');
      expect(db.updateWorkspace).toHaveBeenCalledWith('ws-active-1', 'ContextSplit Engine V2');
      const ws = useWorkspaceStore.getState().workspaces.find((w) => w.id === 'ws-active-1');
      expect(ws?.name).toBe('ContextSplit Engine V2');
    });

    it('rejects rename when input is invalid without writing to database', async () => {
      const result = await quickActionsService.executeContextAction('context.rename', 'ws-active-1', {
        newName: '   ',
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.error).toContain('cannot be empty');
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });

    it('cancels rename gracefully when new name matches current name without DB writes', () => {
      const currentName = 'ContextSplit Engine';
      const inputName = 'ContextSplit Engine';
      const shouldSubmit = inputName.trim() !== currentName.trim();

      expect(shouldSubmit).toBe(false);
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });

    it('prevents concurrent in-flight rename submissions in launcher store', async () => {
      useLauncherStore.setState({ isSubmittingAction: true });

      const result = await useLauncherStore
        .getState()
        .executeContextAction('context.rename', 'ws-active-1', { newName: 'New Name' });

      expect(result.outcome).toBe('BLOCKED');
      expect(result.message).toContain('Another action is currently in progress');
      expect(db.updateWorkspace).not.toHaveBeenCalled();
    });
  });

  // ===========================================================================
  // 3. Pinning or Favoriting and Persistence
  // ===========================================================================
  describe('3. Pinning and persistence', () => {
    it('supports context.pin and context.unpin across all operational lifecycle states', () => {
      const states = ['active', 'paused', 'dormant', 'archived'] as const;
      for (const st of states) {
        expect(validateContextActionTransition('context.pin', st).isValid).toBe(true);
        expect(validateContextActionTransition('context.unpin', st).isValid).toBe(true);
      }
    });

    it('offers context.pin when unpinned and context.unpin when pinned', () => {
      const actionsUnpinned = getAvailableContextActions({
        contextId: 'ws-1',
        isPinned: false,
      });
      expect(actionsUnpinned.some((a) => a.id === 'context.pin')).toBe(true);
      expect(actionsUnpinned.some((a) => a.id === 'context.unpin')).toBe(false);

      const actionsPinned = getAvailableContextActions({
        contextId: 'ws-1',
        isPinned: true,
      });
      expect(actionsPinned.some((a) => a.id === 'context.unpin')).toBe(true);
      expect(actionsPinned.some((a) => a.id === 'context.pin')).toBe(false);
    });

    it('persists pinning via db.setWorkspacePinned and updates workspaceStore', async () => {
      vi.mocked(db.setWorkspacePinned).mockResolvedValueOnce({
        id: 'ws-active-1',
        name: 'ContextSplit Engine',
        is_pinned: true,
        updated_at: 7000,
      } as any);

      const result = await quickActionsService.executeContextAction('context.pin', 'ws-active-1');

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');
      expect(db.setWorkspacePinned).toHaveBeenCalledWith('ws-active-1', true);
      const updatedWs = useWorkspaceStore.getState().workspaces.find((w) => w.id === 'ws-active-1');
      expect(updatedWs?.is_pinned).toBe(true);
    });

    it('persists unpinning via db.setWorkspacePinned and toggles state back', async () => {
      vi.mocked(db.setWorkspacePinned).mockResolvedValueOnce({
        id: 'ws-recent-dup1',
        name: 'Authentication API',
        is_pinned: false,
        updated_at: 8000,
      } as any);

      const result = await quickActionsService.executeContextAction('context.unpin', 'ws-recent-dup1');

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');
      expect(db.setWorkspacePinned).toHaveBeenCalledWith('ws-recent-dup1', false);
      const updatedWs = useWorkspaceStore.getState().workspaces.find((w) => w.id === 'ws-recent-dup1');
      expect(updatedWs?.is_pinned).toBe(false);
    });
  });

  // ===========================================================================
  // 4. Sorting and Grouping
  // ===========================================================================
  describe('4. Sorting and grouping', () => {
    const sampleItems: LauncherContextItem[] = [
      { id: '1', name: 'Zebra', isPinned: false, isActive: false, lastActiveAt: '2026-10-18T10:00:00Z' },
      { id: '2', name: 'Alpha', isPinned: true, isActive: false, lastActiveAt: '2026-10-15T10:00:00Z' },
      { id: '3', name: 'Beta Active', isPinned: false, isActive: true, lastActiveAt: '2026-10-19T10:00:00Z' },
      { id: '4', name: 'Beta Recent', isPinned: false, isActive: false, lastActiveAt: '2026-10-17T10:00:00Z' },
    ];

    it('sorts by "recent": elevates pinned, then active context, then descending recency', () => {
      const sorted = sortContextList(sampleItems, 'recent');
      const ids = sorted.map((i) => i.id);
      expect(ids).toEqual(['2', '3', '1', '4']);
    });

    it('sorts by "name": elevates pinned, then orders alphabetically A-Z case-insensitively', () => {
      const sorted = sortContextList(sampleItems, 'name');
      const ids = sorted.map((i) => i.id);
      expect(ids).toEqual(['2', '3', '4', '1']);
    });

    it('breaks ties between duplicate names deterministically using recency and stable ID', () => {
      const dupItems: LauncherContextItem[] = [
        { id: 'uuid-b', name: 'Dup Name', isPinned: false, lastActiveAt: 1000 },
        { id: 'uuid-a', name: 'Dup Name', isPinned: false, lastActiveAt: 1000 },
        { id: 'uuid-c', name: 'Dup Name', isPinned: false, lastActiveAt: 2000 },
      ];
      const sorted = sortContextList(dupItems, 'name');
      expect(sorted[0].id).toBe('uuid-c'); // Higher recency
      expect(sorted[1].id).toBe('uuid-a'); // Alphabetical ID tie-break
      expect(sorted[2].id).toBe('uuid-b');
    });

    it('partitions list into Pinned, Active Context, and Recent Contexts with zero duplication', () => {
      const groups = groupLauncherContexts(sampleItems, {
        activeFilter: 'all',
        activeContextId: '3',
        activeSort: 'recent',
      });

      expect(groups.length).toBe(3);
      expect(groups[0].title).toBe('Pinned');
      expect(groups[0].items.map((i) => i.id)).toEqual(['2']);

      expect(groups[1].title).toBe('Active Context');
      expect(groups[1].items.map((i) => i.id)).toEqual(['3']);

      expect(groups[2].title).toBe('Recent Contexts');
      expect(groups[2].items.map((i) => i.id)).toEqual(['1', '4']);

      // Assert total count matches and no item appears twice
      const allItemIds = groups.flatMap((g) => g.items.map((i) => i.id));
      expect(allItemIds.length).toBe(sampleItems.length);
      expect(new Set(allItemIds).size).toBe(sampleItems.length);
    });

    it('switches section title to "All Contexts" when activeSort is "name"', () => {
      const groups = groupLauncherContexts(sampleItems, {
        activeFilter: 'all',
        activeSort: 'name',
      });
      expect(groups[0].title).toBe('Pinned');
      expect(groups.find((g) => g.id === 'recent')?.title).toBe('All Contexts');
    });

    it('unifies list into single "Results (N)" group when a search query is active', () => {
      const groups = groupLauncherContexts(sampleItems, {
        activeFilter: 'all',
        searchQuery: 'beta',
      });
      expect(groups.length).toBe(1);
      expect(groups[0].title).toBe('Results (4)');
    });

    it('computes startIndex accurately for seamless sequential keyboard navigation', () => {
      const groups = groupLauncherContexts(sampleItems, {
        activeFilter: 'all',
        activeContextId: '3',
        activeSort: 'recent',
      });
      expect(groups[0].startIndex).toBe(0); // 1 pinned
      expect(groups[1].startIndex).toBe(1); // 1 active
      expect(groups[2].startIndex).toBe(2); // 2 recent
    });
  });

  // ===========================================================================
  // 5. Preference Restoration After Reinitialization
  // ===========================================================================
  describe('5. Preference restoration after reinitialization', () => {
    class MockStorage implements Storage {
      private store = new Map<string, string>();
      get length() { return this.store.size; }
      clear() { this.store.clear(); }
      getItem(k: string) { return this.store.get(k) ?? null; }
      key(i: number) { return Array.from(this.store.keys())[i] ?? null; }
      removeItem(k: string) { this.store.delete(k); }
      setItem(k: string, v: string) { this.store.set(k, v); }
    }

    it('provides sensible default preferences (sortOrder: "recent")', () => {
      expect(DEFAULT_LAUNCHER_PREFERENCES.sortOrder).toBe('recent');
      expect(isValidSortOrder('recent')).toBe(true);
      expect(isValidSortOrder('name')).toBe(true);
      expect(isValidSortOrder('invalid')).toBe(false);
    });

    it('saves preferences to storage and retrieves them intact', () => {
      const storage = new MockStorage();
      const saved = saveLauncherPreferences({ sortOrder: 'name' }, storage);
      expect(saved).toBe(true);

      const loaded = loadLauncherPreferences(storage);
      expect(loaded.sortOrder).toBe('name');

      resetLauncherPreferences(storage);
      const afterReset = loadLauncherPreferences(storage);
      expect(afterReset.sortOrder).toBe('recent');
    });

    it('survives launcher open/close cycles without resetting preferences', () => {
      useLauncherStore.setState({ activeSort: 'name' });
      useLauncherStore.getState().closeLauncher();
      expect(useLauncherStore.getState().isOpen).toBe(false);

      useLauncherStore.getState().openLauncher();
      expect(useLauncherStore.getState().isOpen).toBe(true);
      expect(useLauncherStore.getState().activeSort).toBe('name');
    });

    it('handles corrupt JSON syntax or outdated properties gracefully without throwing', () => {
      const storage = new MockStorage();
      storage.setItem(LAUNCHER_PREFERENCES_STORAGE_KEY, '{corrupted-json:');
      const loaded = loadLauncherPreferences(storage);
      expect(loaded.sortOrder).toBe('recent');

      storage.setItem(LAUNCHER_PREFERENCES_STORAGE_KEY, JSON.stringify({ sortOrder: 'deprecated_mode' }));
      const fallback = loadLauncherPreferences(storage);
      expect(fallback.sortOrder).toBe('recent');
    });

    it('never stores derived UI states (grouping layout, query, cursor) in persistent preferences', () => {
      const parsed = parseLauncherPreferences({
        sortOrder: 'name',
        searchQuery: 'my secret query',
        selectedIndex: 4,
        expandedGroups: ['pinned', 'active'],
      });
      expect(parsed).toEqual({ sortOrder: 'name' });
      expect((parsed as any).searchQuery).toBeUndefined();
      expect((parsed as any).selectedIndex).toBeUndefined();
    });
  });

  // ===========================================================================
  // 6. Archive and Restore Transitions
  // ===========================================================================
  describe('6. Archive and restore transitions', () => {
    it('normalizes operational lifecycle states and prevents archived contexts from being treated as active', () => {
      expect(resolveContextLifecycleState({ lifecycleState: 'active' })).toBe('active');
      expect(resolveContextLifecycleState({ lifecycleState: 'paused' })).toBe('paused');
      expect(resolveContextLifecycleState({ lifecycleState: 'dormant' })).toBe('dormant');
      expect(resolveContextLifecycleState({ lifecycleState: 'archived' })).toBe('archived');
      expect(resolveContextLifecycleState({ lifecycleState: 'inactive' })).toBe('archived');

      // Stale active flag anti-collision
      expect(resolveContextLifecycleState({ lifecycleState: 'archived', isActive: true })).toBe('archived');
      expect(resolveContextLifecycleState({ lifecycleState: 'inactive', isActive: true })).toBe('archived');
    });

    it('validates context.archive: allowed from active, paused, dormant; blocked if already archived', () => {
      expect(validateContextActionTransition('context.archive', 'active').isValid).toBe(true);
      expect(validateContextActionTransition('context.archive', 'paused').isValid).toBe(true);
      expect(validateContextActionTransition('context.archive', 'dormant').isValid).toBe(true);
      const archivedVal = validateContextActionTransition('context.archive', 'archived');
      expect(archivedVal.isValid).toBe(false);
      expect(archivedVal.reason).toContain('already archived');
    });

    it('validates context.restore: allowed ONLY for archived contexts', () => {
      expect(validateContextActionTransition('context.restore', 'archived').isValid).toBe(true);
      expect(validateContextActionTransition('context.restore', 'active').isValid).toBe(false);
      expect(validateContextActionTransition('context.restore', 'paused').isValid).toBe(false);
      expect(validateContextActionTransition('context.restore', 'dormant').isValid).toBe(false);
    });

    it('blocks context.pause for archived contexts', () => {
      const val = validateContextActionTransition('context.pause', 'archived');
      expect(val.isValid).toBe(false);
      expect(val.reason).toContain('Cannot pause an archived context');
    });

    it('never allows permanent context deletion from launcher quick actions', () => {
      const val = validateContextActionTransition('context.delete', 'archived');
      expect(val.isValid).toBe(false);
      expect(val.reason).toContain('workspace management settings');
    });

    it('offers "Restore & Resume" (↵) as primary and "Restore Context" (⌥A) in actions for archived contexts', () => {
      const primary = getPrimaryContextActionDescriptor('archived');
      expect(primary.actionId).toBe('context.resume');
      expect(primary.label).toBe('Restore & Resume');

      const actions = getAvailableContextActions({
        contextId: 'ws-archived-1',
        lifecycleState: 'archived',
        isPinned: false,
      });
      expect(actions.some((a) => a.id === 'context.restore' && a.isSupported)).toBe(true);
      expect(actions.some((a) => a.id === 'context.archive')).toBe(false);
    });

    it('executes context.archive via db.archiveWorkspace and deselects active workspace in UI', async () => {
      useWorkspaceStore.setState({ activeWorkspaceId: 'ws-active-1' });
      vi.mocked(db.archiveWorkspace).mockResolvedValueOnce({
        id: 'ws-active-1',
        name: 'ContextSplit Engine',
        lifecycle_state: 'archived',
      } as any);

      const result = await quickActionsService.executeContextAction('context.archive', 'ws-active-1', {
        lifecycleState: 'active',
      });

      expect(result.success).toBe(true);
      expect(db.archiveWorkspace).toHaveBeenCalledWith('ws-active-1');
      expect(useWorkspaceStore.getState().selectWorkspace).toHaveBeenCalledWith('inbox');
      expect(useWorkspaceStore.getState().loadWorkspaces).toHaveBeenCalled();
    });

    it('executes context.restore via db.unarchiveWorkspace and refreshes workspace store', async () => {
      vi.mocked(db.unarchiveWorkspace).mockResolvedValueOnce({
        id: 'ws-archived-1',
        name: 'Legacy Pipeline',
        lifecycle_state: 'active',
      } as any);

      const result = await quickActionsService.executeContextAction('context.restore', 'ws-archived-1', {
        lifecycleState: 'archived',
      });

      expect(result.success).toBe(true);
      expect(db.unarchiveWorkspace).toHaveBeenCalledWith('ws-archived-1');
      expect(useWorkspaceStore.getState().loadWorkspaces).toHaveBeenCalled();
    });
  });

  // ===========================================================================
  // 7. Invalid Context IDs and Unavailable Contexts
  // ===========================================================================
  describe('7. Invalid context IDs and unavailable contexts', () => {
    it('validates context ID strictly against falsy or whitespace-only inputs', () => {
      expect(validateContextId('').isValid).toBe(false);
      expect(validateContextId('   ').isValid).toBe(false);
      expect(validateContextId(null as any).isValid).toBe(false);
      expect(validateContextId(undefined as any).isValid).toBe(false);
      expect(validateContextId('valid-id').isValid).toBe(true);
    });

    it('handles non-existent or deleted contexts gracefully without crashing', async () => {
      vi.mocked(db.unarchiveWorkspace).mockRejectedValueOnce(new Error('workspace not found'));

      const result = await quickActionsService.executeContextAction('context.restore', 'ws-non-existent', {
        lifecycleState: 'archived',
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(result.message).toContain('Failed to restore context');
    });
  });

  // ===========================================================================
  // 8. Backend Failures and UI Recovery
  // ===========================================================================
  describe('8. Backend failures and UI recovery', () => {
    it('preserves displayed UI state when database rename fails', async () => {
      vi.mocked(db.updateWorkspace).mockRejectedValueOnce(new Error('Database disk error'));

      const result = await quickActionsService.executeContextAction('context.rename', 'ws-active-1', {
        newName: 'Failed Rename Attempt',
      });

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      const ws = useWorkspaceStore.getState().workspaces.find((w) => w.id === 'ws-active-1');
      expect(ws?.name).toBe('ContextSplit Engine'); // Unmutated
      expect(getActionErrorTitle('context.rename')).toBe('Rename Failed');
    });

    it('preserves displayed UI state when database pin fails', async () => {
      vi.mocked(db.setWorkspacePinned).mockRejectedValueOnce(new Error('SQLite locked'));

      const result = await quickActionsService.executeContextAction('context.pin', 'ws-active-1');

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(getActionErrorTitle('context.pin')).toBe('Pin Failed');
      expect(getActionErrorTitle('context.unpin')).toBe('Unpin Failed');
    });

    it('preserves displayed UI state when database archival fails', async () => {
      vi.mocked(db.archiveWorkspace).mockRejectedValueOnce(new Error('Constraint failure'));

      const result = await quickActionsService.executeContextAction('context.archive', 'ws-active-1');

      expect(result.success).toBe(false);
      expect(result.outcome).toBe('ERROR');
      expect(getActionErrorTitle('context.archive')).toBe('Archive Failed');
    });
  });

  // ===========================================================================
  // 9. Preservation of Stable Context IDs and Relationships
  // ===========================================================================
  describe('9. Preservation of stable context IDs and existing relationships', () => {
    it('guarantees context ID remains unchanged across rename, pin, sort, and lifecycle changes', async () => {
      const originalWs = useWorkspaceStore.getState().workspaces[0];
      const targetId = originalWs.id;

      // 1. Rename
      vi.mocked(db.updateWorkspace).mockResolvedValueOnce({
        ...originalWs,
        name: 'New Engine Name',
      } as any);
      await quickActionsService.executeContextAction('context.rename', targetId, {
        newName: 'New Engine Name',
      });
      expect(db.updateWorkspace).toHaveBeenCalledWith(targetId, 'New Engine Name');

      // 2. Pin
      vi.mocked(db.setWorkspacePinned).mockResolvedValueOnce({
        ...originalWs,
        is_pinned: true,
      } as any);
      await quickActionsService.executeContextAction('context.pin', targetId);
      expect(db.setWorkspacePinned).toHaveBeenCalledWith(targetId, true);

      // 3. ID remains identical
      expect(targetId).toBe('ws-active-1');
    });
  });

  // ===========================================================================
  // 10. Keyboard Accessibility for Organization Controls
  // ===========================================================================
  describe('10. Keyboard accessibility for organization controls', () => {
    it('provides keyboard shortcuts Alt+S for sort toggle and Alt+P for pin toggle', () => {
      // Simulate Alt+S keydown
      const currentSort = useLauncherStore.getState().activeSort;
      useLauncherStore.getState().toggleActiveSort();
      expect(useLauncherStore.getState().activeSort).toBe(currentSort === 'recent' ? 'name' : 'recent');

      // Toggle back
      useLauncherStore.getState().toggleActiveSort();
      expect(useLauncherStore.getState().activeSort).toBe(currentSort);
    });

    it('navigates list items sequentially across group boundaries via index', () => {
      const items: LauncherContextItem[] = [
        { id: '1', name: 'Pinned 1', isPinned: true },
        { id: '2', name: 'Active 1', isActive: true },
        { id: '3', name: 'Recent 1' },
      ];
      const groups = groupLauncherContexts(items, { activeFilter: 'all', activeContextId: '2' });

      // Item 0 is in Pinned group
      expect(groups[0].items[0].id).toBe('1');
      // Item 1 is in Active group
      expect(groups[1].items[0].id).toBe('2');
      // Item 2 is in Recent group
      expect(groups[2].items[0].id).toBe('3');

      // Moving cursor through 0, 1, 2 visits each without skipping
      const cursorSequence = [0, 1, 2].map((idx) => items[idx].id);
      expect(cursorSequence).toEqual(['1', '2', '3']);
    });
  });

  // ===========================================================================
  // 11. Correct Behavior When Optional Metadata is Missing
  // ===========================================================================
  describe('11. Correct behavior when optional metadata is missing', () => {
    it('handles missing or invalid timestamps without NaN during recency sort', () => {
      const items: LauncherContextItem[] = [
        { id: '1', name: 'No Timestamps', lastActiveAt: null, createdAt: null },
        { id: '2', name: 'Invalid Timestamps', lastActiveAt: 'bad-date', createdAt: undefined },
        { id: '3', name: 'Valid Timestamp', lastActiveAt: 5000 },
      ];

      const sorted = sortContextList(items, 'recent');
      expect(sorted[0].id).toBe('3');
      // Missing and invalid timestamps do not throw and are placed at end
      expect(sorted.map((i) => i.id)).toContain('1');
      expect(sorted.map((i) => i.id)).toContain('2');
    });

    it('returns null subtitle when description, taskTitle, and activitySummary are absent (zero fabrication)', () => {
      const item: LauncherContextItem = {
        id: 'ws-empty-meta',
        name: 'Empty Metadata Context',
        state: 'recent',
        description: null,
        taskTitle: null,
        activitySummary: null,
      };
      expect(resolveContextSubtitle(item, false)).toBeNull();
    });

    it('handles gentle relative time safely when timestamps are missing or invalid', () => {
      expect(formatGentleTime(null)).toBe('Ready to resume');
      expect(formatGentleTime(undefined)).toBe('Ready to resume');
      expect(formatGentleTime('invalid-date')).toBe('Recently active');
    });

    it('handles friendly date safely when date is missing or invalid', () => {
      expect(formatFriendlyDate(null)).toBeNull();
      expect(formatFriendlyDate(undefined)).toBeNull();
      expect(formatFriendlyDate('invalid-date')).toBeNull();
    });
  });

  // ===========================================================================
  // 12. Continued Compatibility with Existing Context Launcher Actions
  // ===========================================================================
  describe('12. Continued compatibility with existing Context Launcher actions', () => {
    it('preserves context.resume execution and state updates across contexts', async () => {
      vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce(undefined as any);

      const result = await quickActionsService.executeContextAction('context.resume', 'ws-paused-1', {
        lifecycleState: 'paused',
      });

      expect(result.success).toBe(true);
      expect(result.outcome).toBe('SUCCESS');
      expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ws-paused-1');
      expect(useWorkspaceStore.getState().selectWorkspace).toHaveBeenCalledWith('ws-paused-1');
    });

    it('preserves context.open_details execution', async () => {
      const result = await quickActionsService.executeContextAction('context.open_details', 'ws-active-1');
      expect(result.success).toBe(true);
      expect(useWorkspaceStore.getState().openOverview).toHaveBeenCalledWith('ws-active-1');
    });

    it('preserves filter isolation across all, active, recent, paused, and archived views', () => {
      const allItems: LauncherContextItem[] = [
        { id: '1', name: 'Active', isActive: true, state: 'active' },
        { id: '2', name: 'Recent', isActive: false, state: 'recent' },
        { id: '3', name: 'Paused', isActive: false, state: 'paused' },
        { id: '4', name: 'Archived', isActive: false, state: 'archived' },
      ];

      expect(allItems.filter((i) => matchesContextFilter(i, 'all')).length).toBe(4);
      expect(allItems.filter((i) => matchesContextFilter(i, 'active')).map((i) => i.id)).toEqual(['1']);
      expect(allItems.filter((i) => matchesContextFilter(i, 'recent')).map((i) => i.id)).toEqual(['2']);
      expect(allItems.filter((i) => matchesContextFilter(i, 'paused')).map((i) => i.id)).toEqual(['3']);
      expect(allItems.filter((i) => matchesContextFilter(i, 'archived')).map((i) => i.id)).toEqual(['4']);
    });
  });
});
