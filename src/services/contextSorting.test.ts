import { describe, it, expect, beforeEach } from 'vitest';
import {
  sortContextList,
  sortContextMatches,
  groupLauncherContexts,
  getContextRecencyTimestamp,
  getContextSearchTier,
  LauncherContextItem,
  useLauncherStore,
} from '../store/launcherStore';
import { getDuplicateNameSet, resolveContextDisplayIdentity } from '../models/contextIdentification';

describe('Context Sorting, Grouping & Organization', () => {
  const sampleContexts: LauncherContextItem[] = [
    {
      id: 'ctx-1',
      name: 'Alpha Project',
      isActive: false,
      isPinned: false,
      lastActiveAt: '2026-10-18T10:00:00Z',
      createdAt: '2026-10-01T10:00:00Z',
      state: 'dormant',
    },
    {
      id: 'ctx-2',
      name: 'Beta Project',
      isActive: true,
      isPinned: false,
      lastActiveAt: '2026-10-20T12:00:00Z',
      createdAt: '2026-10-05T10:00:00Z',
      state: 'active',
    },
    {
      id: 'ctx-3',
      name: 'Gamma Project',
      isActive: false,
      isPinned: true,
      lastActiveAt: '2026-10-15T10:00:00Z',
      createdAt: '2026-10-10T10:00:00Z',
      state: 'paused',
    },
    {
      id: 'ctx-4',
      name: 'Delta Project',
      isActive: false,
      isPinned: false,
      lastActiveAt: '2026-10-19T10:00:00Z',
      createdAt: '2026-10-12T10:00:00Z',
      state: 'dormant',
    },
  ];

  beforeEach(() => {
    useLauncherStore.setState({
      activeSort: 'recent',
      activeFilter: 'all',
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: null,
    });
  });

  // =========================================================================
  // 1. Ordering Logic (Recent vs Name)
  // =========================================================================
  describe('Context Ordering Logic', () => {
    it('sorts by recency by default: pinned first, then active, then descending timestamps', () => {
      const sorted = sortContextList(sampleContexts, 'recent');
      const sortedIds = sorted.map((c) => c.id);

      // ctx-3 is pinned -> must be first
      expect(sortedIds[0]).toBe('ctx-3');
      // ctx-2 is active -> must be first among unpinned
      expect(sortedIds[1]).toBe('ctx-2');
      // ctx-4 (Oct 19) is more recent than ctx-1 (Oct 18)
      expect(sortedIds[2]).toBe('ctx-4');
      expect(sortedIds[3]).toBe('ctx-1');
    });

    it('sorts by name A-Z: pinned first (alphabetically), then unpinned (alphabetically)', () => {
      const sorted = sortContextList(sampleContexts, 'name');
      const sortedIds = sorted.map((c) => c.id);

      // Pinned: ctx-3 (Gamma)
      expect(sortedIds[0]).toBe('ctx-3');

      // Unpinned: Alpha (ctx-1), Beta (ctx-2), Delta (ctx-4)
      expect(sortedIds[1]).toBe('ctx-1');
      expect(sortedIds[2]).toBe('ctx-2');
      expect(sortedIds[3]).toBe('ctx-4');
    });

    it('handles case-insensitivity and leading/trailing whitespace in name sorting', () => {
      const items: LauncherContextItem[] = [
        { id: '1', name: '  zebra  ', lastActiveAt: '2026-10-01' },
        { id: '2', name: 'apple', lastActiveAt: '2026-10-02' },
        { id: '3', name: 'Banana', lastActiveAt: '2026-10-03' },
      ];

      const sorted = sortContextList(items, 'name');
      expect(sorted.map((c) => c.id)).toEqual(['2', '3', '1']);
    });

    it('preserves order or returns early for single-item or empty lists', () => {
      expect(sortContextList([])).toEqual([]);
      const single = [sampleContexts[0]];
      expect(sortContextList(single, 'recent')).toEqual(single);
      expect(sortContextList(single, 'name')).toEqual(single);
    });
  });

  // =========================================================================
  // 2. Missing Metadata & Deterministic Timestamp Fallbacks
  // =========================================================================
  describe('Missing Metadata & Deterministic Timestamps', () => {
    it('falls back to createdAt when lastActiveAt is absent', () => {
      const item: LauncherContextItem = {
        id: 'no-active',
        name: 'Context Without LastActive',
        createdAt: '2026-10-15T00:00:00Z',
      };
      const ts = getContextRecencyTimestamp(item);
      expect(ts).toBe(new Date('2026-10-15T00:00:00Z').getTime());
    });

    it('returns 0 when both lastActiveAt and createdAt are absent or invalid', () => {
      const itemNull: LauncherContextItem = {
        id: 'null-ts',
        name: 'Null Timestamps',
        lastActiveAt: null,
        createdAt: null,
      };
      expect(getContextRecencyTimestamp(itemNull)).toBe(0);

      const itemInvalid: LauncherContextItem = {
        id: 'invalid-ts',
        name: 'Invalid Date String',
        lastActiveAt: 'not-a-valid-date',
        createdAt: undefined,
      };
      expect(getContextRecencyTimestamp(itemInvalid)).toBe(0);
    });

    it('places items with missing timestamps at the end during recency sort without throwing NaN', () => {
      const items: LauncherContextItem[] = [
        { id: 'missing-1', name: 'Zeta (Missing TS)', lastActiveAt: null, createdAt: null },
        { id: 'valid-1', name: 'Recent Item', lastActiveAt: '2026-10-19T10:00:00Z' },
        { id: 'missing-2', name: 'Alpha (Missing TS)', lastActiveAt: undefined, createdAt: undefined },
      ];

      const sorted = sortContextList(items, 'recent');
      // valid-1 is first
      expect(sorted[0].id).toBe('valid-1');
      // Between missing-1 (Zeta) and missing-2 (Alpha), name tie-breaker places Alpha first
      expect(sorted[1].id).toBe('missing-2');
      expect(sorted[2].id).toBe('missing-1');
    });

    it('sorts items with completely missing optional fields (description, project, repo) without errors', () => {
      const items: LauncherContextItem[] = [
        { id: 'b', name: 'Item B' },
        { id: 'a', name: 'Item A' },
      ];
      const sorted = sortContextList(items, 'name');
      expect(sorted.map((c) => c.id)).toEqual(['a', 'b']);
    });
  });

  // =========================================================================
  // 3. Duplicate Names & Disambiguation
  // =========================================================================
  describe('Duplicate Names Handling', () => {
    it('deterministically breaks ties for identical names using recency then stable ID', () => {
      const duplicates: LauncherContextItem[] = [
        { id: 'ctx-b', name: 'Frontend Refactor', lastActiveAt: '2026-10-15T00:00:00Z' },
        { id: 'ctx-a', name: 'Frontend Refactor', lastActiveAt: '2026-10-18T00:00:00Z' },
        { id: 'ctx-c', name: 'Frontend Refactor', lastActiveAt: '2026-10-15T00:00:00Z' },
      ];

      const sortedByName = sortContextList(duplicates, 'name');
      // ctx-a has more recent timestamp (Oct 18 vs Oct 15)
      expect(sortedByName[0].id).toBe('ctx-a');
      // ctx-b and ctx-c have equal timestamps, so tie is broken by ID ('ctx-b' < 'ctx-c')
      expect(sortedByName[1].id).toBe('ctx-b');
      expect(sortedByName[2].id).toBe('ctx-c');
    });

    it('ranks pinned duplicate above unpinned duplicate of the same name', () => {
      const duplicates: LauncherContextItem[] = [
        { id: 'unpinned-dup', name: 'Shared Task', isPinned: false, lastActiveAt: '2026-10-20' },
        { id: 'pinned-dup', name: 'Shared Task', isPinned: true, lastActiveAt: '2026-10-10' },
      ];

      const sorted = sortContextList(duplicates, 'name');
      expect(sorted[0].id).toBe('pinned-dup');
      expect(sorted[1].id).toBe('unpinned-dup');
    });

    it('correctly populates duplicate name set and disambiguation tags', () => {
      const all = [
        { id: '1', name: 'Backend Sync', projectIdentifier: 'proj-a' },
        { id: '2', name: 'backend sync', projectIdentifier: 'proj-b' },
        { id: '3', name: 'Unique Service' },
      ];

      const dupSet = getDuplicateNameSet(all);
      expect(dupSet.has('backend sync')).toBe(true);
      expect(dupSet.has('unique service')).toBe(false);

      const identity1 = resolveContextDisplayIdentity(all[0], dupSet);
      const identity3 = resolveContextDisplayIdentity(all[2], dupSet);

      expect(identity1.disambiguationTag).toBe('proj-a');
      expect(identity3.disambiguationTag).toBeUndefined();
    });
  });

  // =========================================================================
  // 4. Data-Grounded Grouping & Zero Duplication
  // =========================================================================
  describe('Context Grouping Logic', () => {
    it('groups contexts into Pinned, Active Context, and Recent Contexts with zero duplication', () => {
      const sorted = sortContextList(sampleContexts, 'recent');
      const groups = groupLauncherContexts(sorted, {
        activeFilter: 'all',
        activeSort: 'recent',
        activeContextId: 'ctx-2',
      });

      expect(groups.length).toBe(3);

      // Group 1: Pinned
      expect(groups[0].id).toBe('pinned');
      expect(groups[0].title).toBe('Pinned');
      expect(groups[0].items.map((i) => i.id)).toEqual(['ctx-3']);
      expect(groups[0].startIndex).toBe(0);

      // Group 2: Active Context
      expect(groups[1].id).toBe('active');
      expect(groups[1].title).toBe('Active Context');
      expect(groups[1].items.map((i) => i.id)).toEqual(['ctx-2']);
      expect(groups[1].startIndex).toBe(1);

      // Group 3: Recent Contexts
      expect(groups[2].id).toBe('recent');
      expect(groups[2].title).toBe('Recent Contexts');
      expect(groups[2].items.map((i) => i.id)).toEqual(['ctx-4', 'ctx-1']);
      expect(groups[2].startIndex).toBe(2);

      // Verify no item appears more than once across all groups
      const allGroupedIds = groups.flatMap((g) => g.items.map((i) => i.id));
      expect(new Set(allGroupedIds).size).toBe(allGroupedIds.length);
      expect(allGroupedIds.length).toBe(sampleContexts.length);
    });

    it('changes group title to "All Contexts" when activeSort is "name"', () => {
      const sorted = sortContextList(sampleContexts, 'name');
      const groups = groupLauncherContexts(sorted, {
        activeFilter: 'all',
        activeSort: 'name',
        activeContextId: 'ctx-2',
      });

      const recentOrAllGroup = groups.find((g) => g.id === 'recent');
      expect(recentOrAllGroup?.title).toBe('All Contexts');
    });

    it('groups into a single unified "Results (N)" group when searching', () => {
      const groups = groupLauncherContexts(sampleContexts, {
        searchQuery: 'Project',
        activeFilter: 'all',
        activeSort: 'recent',
      });

      expect(groups.length).toBe(1);
      expect(groups[0].id).toBe('results');
      expect(groups[0].title).toBe('Results (4)');
      expect(groups[0].startIndex).toBe(0);
    });

    it('guarantees startIndex sequential mapping for keyboard navigation', () => {
      const sorted = sortContextList(sampleContexts, 'recent');
      const groups = groupLauncherContexts(sorted, {
        activeFilter: 'all',
        activeSort: 'recent',
        activeContextId: 'ctx-2',
      });

      for (const group of groups) {
        group.items.forEach((item, relIdx) => {
          const globalIdx = group.startIndex + relIdx;
          expect(sorted[globalIdx].id).toBe(item.id);
        });
      }
    });
  });

  // =========================================================================
  // 5. Stable Selection & Immutability of Context Identity
  // =========================================================================
  describe('Stable Selection & Identity Preservation', () => {
    it('preserves context identity, lifecycle, and properties through sorting', () => {
      const sorted = sortContextList(sampleContexts, 'name');

      for (const item of sorted) {
        const original = sampleContexts.find((c) => c.id === item.id);
        expect(original).toBeDefined();
        expect(item.id).toBe(original?.id);
        expect(item.name).toBe(original?.name);
        expect(item.isActive).toBe(original?.isActive);
        expect(item.isPinned).toBe(original?.isPinned);
        expect(item.state).toBe(original?.state);
        expect(item.lastActiveAt).toBe(original?.lastActiveAt);
      }
    });

    it('toggles activeSort between recent and name in launcher store', () => {
      const store = useLauncherStore.getState();
      expect(store.activeSort).toBe('recent');

      store.toggleActiveSort();
      expect(useLauncherStore.getState().activeSort).toBe('name');

      store.toggleActiveSort();
      expect(useLauncherStore.getState().activeSort).toBe('recent');

      store.setActiveSort('name');
      expect(useLauncherStore.getState().activeSort).toBe('name');
    });

    it('tracks selected context position when sort order is changed', () => {
      // Suppose ctx-4 is selected in 'recent' order
      const recentList = sortContextList(sampleContexts, 'recent');
      const targetId = 'ctx-4';
      const recentIndex = recentList.findIndex((c) => c.id === targetId);

      // Now switch to 'name' order
      const nameList = sortContextList(sampleContexts, 'name');
      const nameIndex = nameList.findIndex((c) => c.id === targetId);

      // The item still exists and has a stable ID
      expect(recentIndex).toBe(2);
      expect(nameIndex).toBe(3);
      expect(nameList[nameIndex].id).toBe(targetId);
    });
  });

  // =========================================================================
  // 6. Search Ranking with Sorting Integration
  // =========================================================================
  describe('Search Ranking & Sorting Integration', () => {
    it('prioritizes match tier over sorting, but respects name sort within equal match tiers', () => {
      const items: LauncherContextItem[] = [
        { id: '1', name: 'Sync Core', lastActiveAt: '2026-10-10' },
        { id: '2', name: 'Sync Agent', lastActiveAt: '2026-10-15' },
        { id: '3', name: 'Alpha Sync', lastActiveAt: '2026-10-20' }, // Tier 3 (contains "Sync")
      ];

      // When searching "Sync" with 'name' sort:
      // Tier 2 (starts with): 'Sync Agent' (id 2) vs 'Sync Core' (id 1) -> alphabetically 'Agent' comes before 'Core'
      // Tier 3: 'Alpha Sync' (id 3)
      const rankedByName = sortContextMatches(items, 'Sync', 'name');
      expect(rankedByName.map((i) => i.id)).toEqual(['2', '1', '3']);

      // When searching "Sync" with 'recent' sort:
      // Within Tier 2, 'Sync Agent' (Oct 15) is more recent than 'Sync Core' (Oct 10)
      const rankedByRecent = sortContextMatches(items, 'Sync', 'recent');
      expect(rankedByRecent.map((i) => i.id)).toEqual(['2', '1', '3']);
    });
  });
});
