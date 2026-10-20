import { create } from 'zustand';
import { contextInteractionService } from '../services/contextInteractionService';
import { useWorkspaceStore } from './workspaceStore';
import { toast } from 'sonner';
import {
  ContextActionId,
  ItemActionId,
  QuickActionResult,
} from '../models/quickActions';
import { quickActionsService, ExecuteContextActionOptions } from '../services/quickActionsService';

export type ResumeActionStatus = 'idle' | 'loading' | 'success' | 'error' | 'not_ready' | 'cancelled';

export interface LauncherResumeReadiness {
  readiness: 'ready' | 'mostly_ready' | 'partial' | 'insufficient' | string;
  formattedExplanation?: string;
  summary?: string;
}

export interface LauncherContextItem {
  id: string;
  name: string;
  description?: string | null;
  projectIdentifier?: string | null;
  repository?: string | null;
  isActive?: boolean;
  isPinned?: boolean;
  state?: string;
  lastActiveAt?: string | null;
  durationEstimate?: string | null;
  taskTitle?: string | null;
  itemCount?: number;
  sessionCount?: number;
  confidence?: string;
  recentActivitySummary?: string | null;
  memoryStatement?: string | null;
  resumeReadiness?: LauncherResumeReadiness | null;
  workspacePath?: string | null;
  createdAt?: string | null;
}

export type ContextFilterType = 'all' | 'active' | 'recent' | 'paused' | 'archived';

/**
 * Matches a context item against the active lifecycle filter.
 * Composes with search queries to enable refined discovery.
 */
export function matchesContextFilter(item: LauncherContextItem, filter: ContextFilterType): boolean {
  if (filter === 'all') return true;

  const state = (item.state || '').toLowerCase();

  switch (filter) {
    case 'active':
      return Boolean(item.isActive || state === 'active');
    case 'recent':
      return Boolean(
        state === 'recent' ||
        (!item.isActive && state !== 'paused' && state !== 'dormant' && state !== 'archived' && state !== 'inactive')
      );
    case 'paused':
      return Boolean(state === 'paused' || state === 'dormant');
    case 'archived':
      return Boolean(state === 'archived' || state === 'inactive');
    default:
      return true;
  }
}

/**
 * Deterministic, whitespace-tolerant, case-insensitive context search matcher.
 * Matches against structured context metadata without searching arbitrary raw user files.
 * Optimized to avoid intermediate array allocations and string joins per item during typing.
 */
export function matchContext(item: LauncherContextItem, query: string): boolean {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) return true;

  // Split query into terms to support whitespace-tolerant multi-word matching
  const terms = normalizedQuery.split(/\s+/).filter(Boolean);
  if (terms.length === 0) return true;

  const name = item.name.toLowerCase();
  const desc = item.description?.toLowerCase();
  const proj = item.projectIdentifier?.toLowerCase();
  const repo = item.repository?.toLowerCase();
  const task = item.taskTitle?.toLowerCase();
  const activity = item.recentActivitySummary?.toLowerCase();
  const memory = item.memoryStatement?.toLowerCase();
  const state = item.state?.toLowerCase();

  // All query terms must match at least one searchable field
  return terms.every(
    (term) =>
      name.includes(term) ||
      (desc !== undefined && desc.includes(term)) ||
      (proj !== undefined && proj.includes(term)) ||
      (repo !== undefined && repo.includes(term)) ||
      (task !== undefined && task.includes(term)) ||
      (activity !== undefined && activity.includes(term)) ||
      (memory !== undefined && memory.includes(term)) ||
      (state !== undefined && state.includes(term))
  );
}

/**
 * Default maximum number of visible context results rendered in the launcher DOM.
 * Bounds UI rendering cost to <2ms during fast typing bursts even with 5,000+ items.
 */
export const MAX_VISIBLE_LAUNCHER_RESULTS = 50;

/**
 * Computes windowed visible slice of contexts to keep DOM rendering cost <2ms
 * while allowing continuous keyboard navigation past initial boundaries.
 */
export function getVisibleLauncherSlice<T>(
  items: T[],
  selectedIndex: number,
  maxVisible = MAX_VISIBLE_LAUNCHER_RESULTS
): { visibleItems: T[]; totalCount: number; hasMore: boolean } {
  const visibleLimit = Math.max(maxVisible, Math.min(items.length, selectedIndex + 10));
  return {
    visibleItems: items.slice(0, visibleLimit),
    totalCount: items.length,
    hasMore: items.length > visibleLimit,
  };
}

/**
 * Deterministic search ranking tier for a context item:
 * 1. Exact context name match
 * 2. Context name starts with query
 * 3. Context name contains query
 * 4. Description/project metadata match
 * 5. Non-matching fallback
 */
export type ContextSearchTier = 1 | 2 | 3 | 4 | 5;

/**
 * Computes deterministic search ranking tier for a context item.
 */
export function getContextSearchTier(item: LauncherContextItem, query: string): ContextSearchTier {
  const q = query.trim().toLowerCase();
  if (!q) return 1;

  const name = item.name.trim().toLowerCase();

  // 1. Exact context name match
  if (name === q) {
    return 1;
  }

  // 2. Context name starts with query
  if (name.startsWith(q)) {
    return 2;
  }

  const terms = q.split(/\s+/).filter(Boolean);

  // 3. Context name contains query or contains all query terms
  if (name.includes(q) || (terms.length > 0 && terms.every((t) => name.includes(t)))) {
    return 3;
  }

  // 4. Description or project/repo/task metadata match
  const desc = item.description?.toLowerCase();
  const proj = item.projectIdentifier?.toLowerCase();
  const repo = item.repository?.toLowerCase();
  const task = item.taskTitle?.toLowerCase();
  const activity = item.recentActivitySummary?.toLowerCase();
  const memory = item.memoryStatement?.toLowerCase();
  const state = item.state?.toLowerCase();

  const termMatches = (t: string) =>
    name.includes(t) ||
    (desc !== undefined && desc.includes(t)) ||
    (proj !== undefined && proj.includes(t)) ||
    (repo !== undefined && repo.includes(t)) ||
    (task !== undefined && task.includes(t)) ||
    (activity !== undefined && activity.includes(t)) ||
    (memory !== undefined && memory.includes(t)) ||
    (state !== undefined && state.includes(t));

  if (terms.length > 0 && terms.every(termMatches)) {
    return 4;
  }

  return 5;
}

export {
  ContextSortOrder,
  LauncherPreferences,
  DEFAULT_LAUNCHER_PREFERENCES,
  loadLauncherPreferences,
  saveLauncherPreferences,
} from '../models/launcherPreferences';
import {
  ContextSortOrder,
  loadLauncherPreferences,
  saveLauncherPreferences,
} from '../models/launcherPreferences';

/**
 * Extracts a deterministic numeric timestamp for recency tie-breaking.
 * Safely falls back to createdAt, or 0 if timestamps are absent or invalid.
 */
export function getContextRecencyTimestamp(item: LauncherContextItem): number {
  if (item.isActive) {
    if (item.lastActiveAt) {
      const parsed = new Date(item.lastActiveAt).getTime();
      if (!isNaN(parsed) && parsed > 0) return parsed;
    }
    return Date.now();
  }

  if (item.lastActiveAt) {
    const parsed = new Date(item.lastActiveAt).getTime();
    if (!isNaN(parsed) && parsed > 0) return parsed;
  }

  if (item.createdAt) {
    const parsed = new Date(item.createdAt).getTime();
    if (!isNaN(parsed) && parsed > 0) return parsed;
  }

  return 0;
}

/**
 * Deterministically sorts contexts according to the requested sort order while keeping
 * pinned contexts elevated at the top without duplication.
 */
export function sortContextList(
  items: LauncherContextItem[],
  sortOrder: ContextSortOrder = 'recent'
): LauncherContextItem[] {
  if (items.length <= 1) return items;

  const pinned = items.filter((item) => Boolean(item.isPinned));
  const unpinned = items.filter((item) => !Boolean(item.isPinned));

  const compareItems = (a: LauncherContextItem, b: LauncherContextItem): number => {
    if (sortOrder === 'name') {
      // Alphabetical A-Z (case-insensitive, trimmed)
      const nameA = a.name.trim().toLowerCase();
      const nameB = b.name.trim().toLowerCase();
      const comp = nameA.localeCompare(nameB);
      if (comp !== 0) return comp;

      // Secondary tie-breaker: Recency timestamp (most recent first)
      const recA = getContextRecencyTimestamp(a);
      const recB = getContextRecencyTimestamp(b);
      if (recA !== recB) return recB - recA;

      // Final tie-breaker: Deterministic stable ID
      return a.id.localeCompare(b.id);
    }

    // Default 'recent' sort:
    // Active context comes first if among unpinned or pinned
    if (a.isActive !== b.isActive) {
      return a.isActive ? -1 : 1;
    }

    // Recency timestamp descending
    const recA = getContextRecencyTimestamp(a);
    const recB = getContextRecencyTimestamp(b);
    if (recA !== recB) return recB - recA;

    // Secondary tie-breaker: Alphabetical by name
    const comp = a.name.trim().toLowerCase().localeCompare(b.name.trim().toLowerCase());
    if (comp !== 0) return comp;

    // Final tie-breaker: Deterministic stable ID
    return a.id.localeCompare(b.id);
  };

  const sortedPinned = [...pinned].sort(compareItems);
  const sortedUnpinned = [...unpinned].sort(compareItems);

  return [...sortedPinned, ...sortedUnpinned];
}

/**
 * Sorts matching contexts deterministically based on:
 * 1. Ranking tier (exact match -> prefix match -> substring -> metadata)
 * 2. Pinned priority within equal ranking tiers
 * 3. Requested sort order (alphabetical A-Z or recency)
 * 4. Recency timestamp as a tie-breaker
 * 5. Prefix token length and stable ID
 */
export function sortContextMatches(
  items: LauncherContextItem[],
  query: string,
  sortOrder: ContextSortOrder = 'recent'
): LauncherContextItem[] {
  const q = query.trim().toLowerCase();
  if (!q || items.length <= 1) return items;

  // Decorate items with ranking tier, recency timestamp, and prefix token length once in O(N)
  // This avoids re-evaluating string metadata operations inside O(N log N) comparator calls
  const decorated = items.map((item) => {
    const tier = getContextSearchTier(item, q);
    const recency = getContextRecencyTimestamp(item);
    const isPinned = Boolean(item.isPinned);
    const wordLen = tier === 2 ? (item.name.split(/\s+/)[0] || item.name).length : 0;
    return { item, tier, recency, isPinned, wordLen };
  });

  decorated.sort((a, b) => {
    // 1-4: Ranking tier
    if (a.tier !== b.tier) {
      return a.tier - b.tier;
    }

    // 5. Pinned contexts rank higher within equal ranking tiers
    if (a.isPinned !== b.isPinned) {
      return a.isPinned ? -1 : 1;
    }

    // 6. Name sort option: alphabetical within equal tiers
    if (sortOrder === 'name') {
      const nameDiff = a.item.name.trim().toLowerCase().localeCompare(b.item.name.trim().toLowerCase());
      if (nameDiff !== 0) return nameDiff;
    }

    // 7. Recency as primary tie-breaker (or primary comparator for 'recent' sort)
    if (a.recency !== b.recency) {
      return b.recency - a.recency; // Most recent first
    }

    // Secondary deterministic tie-breaker for Tier 2: shorter matching prefix token (e.g. "PayX" vs "Payment")
    if (a.tier === 2 && b.tier === 2 && a.wordLen !== b.wordLen) {
      return a.wordLen - b.wordLen;
    }

    // Final deterministic tie-breaker: alphabetical order by name, then deterministic ID order
    const nameDiff = a.item.name.localeCompare(b.item.name);
    if (nameDiff !== 0) return nameDiff;
    return a.item.id.localeCompare(b.item.id);
  });

  return decorated.map((d) => d.item);
}

export interface LauncherContextGroup {
  id: 'pinned' | 'active' | 'recent' | 'filtered' | 'results';
  title: string;
  items: LauncherContextItem[];
  startIndex: number;
}

/**
 * Organizes contexts into clear, deterministic groups grounded in actual data.
 * Every item appears in exactly one group with a stable index for keyboard selection.
 */
export function groupLauncherContexts(
  items: LauncherContextItem[],
  options: {
    searchQuery?: string;
    activeFilter?: ContextFilterType;
    activeSort?: ContextSortOrder;
    activeContextId?: string | null;
  }
): LauncherContextGroup[] {
  const { searchQuery = '', activeFilter = 'all', activeSort = 'recent', activeContextId = null } = options;

  if (items.length === 0) return [];

  // When search query is active, return a single unified results group
  if (searchQuery.trim()) {
    return [
      {
        id: 'results',
        title: `Results (${items.length})`,
        items,
        startIndex: 0,
      },
    ];
  }

  const groups: LauncherContextGroup[] = [];
  let currentIndex = 0;

  // 1. Pinned Contexts (if any are present in the filtered slice)
  const pinnedItems = items.filter((item) => Boolean(item.isPinned));
  if (pinnedItems.length > 0) {
    groups.push({
      id: 'pinned',
      title: 'Pinned',
      items: pinnedItems,
      startIndex: currentIndex,
    });
    currentIndex += pinnedItems.length;
  }

  // 2. Active Context (if in items, not already pinned, and viewing 'all' filter)
  const unpinned = items.filter((item) => !Boolean(item.isPinned));
  const activeItem = unpinned.find((item) => item.id === activeContextId || Boolean(item.isActive));
  const otherItems = unpinned.filter((item) => item !== activeItem);

  if (activeItem && activeFilter === 'all') {
    groups.push({
      id: 'active',
      title: 'Active Context',
      items: [activeItem],
      startIndex: currentIndex,
    });
    currentIndex += 1;
  }

  // 3. Remaining Contexts (Recent, Filtered, or All)
  const remainingItems = activeItem && activeFilter === 'all' ? otherItems : unpinned;
  if (remainingItems.length > 0) {
    let title: string;
    if (activeFilter === 'all') {
      title = activeSort === 'name' ? 'All Contexts' : 'Recent Contexts';
    } else {
      const filterLabel =
        activeFilter.charAt(0).toUpperCase() + activeFilter.slice(1);
      title = `${filterLabel} Contexts (${remainingItems.length})`;
    }

    groups.push({
      id: activeFilter === 'all' ? 'recent' : 'filtered',
      title,
      items: remainingItems,
      startIndex: currentIndex,
    });
  }

  return groups;
}

export interface LauncherPreviewItem {
  id: string;
  title: string;
  itemType: string;
  relevance?: string;
  isImportant?: boolean;
  appName?: string | null;
  domain?: string | null;
  snippet?: string | null;
}

export interface LauncherPreviewModel {
  contextId: string;
  name: string;
  state: string;
  isPinned?: boolean;
  description?: string | null;
  projectIdentifier?: string | null;
  repository?: string | null;
  workspacePath?: string | null;
  taskTitle?: string | null;
  lastActiveAt?: string | null;
  durationEstimate?: string | null;
  readinessBand: string;
  readinessSummary?: string;
  readinessExplanation?: string;
  reasons: string[];
  importantItems: LauncherPreviewItem[];
  worksetItems: LauncherPreviewItem[];
  itemCount: number;
  activitySummary?: string | null;
  isAmbiguous: boolean;
  isStale: boolean;
}

export interface ActionErrorInfo {
  actionId: string;
  title: string;
  message: string;
  targetId?: string;
  isRetryable?: boolean;
}

export function getActionErrorTitle(actionId: string): string {
  switch (actionId) {
    case 'context.resume':
      return 'Resume Failed';
    case 'context.pause':
      return 'Pause Failed';
    case 'context.rename':
      return 'Rename Failed';
    case 'context.pin':
      return 'Pin Failed';
    case 'context.unpin':
      return 'Unpin Failed';
    case 'context.archive':
      return 'Archive Failed';
    case 'context.open_details':
      return 'Could Not Open Details';
    case 'item.mark_important':
    case 'item.unmark_important':
      return 'Memory Update Failed';
    case 'item.remove_from_context':
      return 'Remove Item Failed';
    default:
      return 'Action Failed';
  }
}

interface LauncherState {
  isOpen: boolean;
  searchQuery: string;
  selectedIndex: number;
  selectedContextId: string | null;
  activePreview: LauncherPreviewModel | null;
  isLoading: boolean;
  isLoadingPreview: boolean;
  isResuming: boolean;
  isActionExecuting: boolean;
  executingActionId: ContextActionId | ItemActionId | null;
  executingTargetId: string | null;
  resumeStatus: ResumeActionStatus;
  resumeError: string | null;
  actionError: ActionErrorInfo | null;
  notReadyReason: string | null;
  error: string | null;
  previewError: string | null;
  activeContext: LauncherContextItem | null;
  recentContexts: LauncherContextItem[];
  isPreviewOpen: boolean;
  activeFilter: ContextFilterType;
  activeSort: ContextSortOrder;
  
  // Actions
  openLauncher: () => void;
  closeLauncher: () => void;
  toggleLauncher: () => void;
  setSearchQuery: (query: string) => void;
  setActiveFilter: (filter: ContextFilterType) => void;
  setActiveSort: (sort: ContextSortOrder) => void;
  toggleActiveSort: () => void;
  setSelectedIndex: (index: number) => void;
  setIsPreviewOpen: (open: boolean) => void;
  selectContextById: (contextId: string) => Promise<void>;
  fetchLauncherData: () => Promise<void>;
  resumeContextById: (contextId: string, force?: boolean) => Promise<boolean>;
  renameContextById: (contextId: string, newName: string) => Promise<QuickActionResult<ContextActionId>>;
  clearResumeState: () => void;
  clearActionError: () => void;
  executeContextAction: (
    actionId: ContextActionId,
    contextId?: string,
    options?: ExecuteContextActionOptions
  ) => Promise<QuickActionResult<ContextActionId>>;
  executeItemAction: (
    actionId: ItemActionId,
    contextId: string,
    itemId: string
  ) => Promise<QuickActionResult<ItemActionId>>;
}

// Debounce timer for preview fetching to avoid IPC jitter
let previewDebounceTimer: ReturnType<typeof setTimeout> | null = null;

export const useLauncherStore = create<LauncherState>((set, get) => ({
  isOpen: false,
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
  resumeStatus: 'idle',
  resumeError: null,
  actionError: null,
  notReadyReason: null,
  error: null,
  previewError: null,
  activeContext: null,
  recentContexts: [],
  isPreviewOpen: false,
  activeFilter: 'all',
  activeSort: loadLauncherPreferences().sortOrder,

  openLauncher: () => {
    set({
      isOpen: true,
      searchQuery: '',
      activeFilter: 'all',
      selectedIndex: 0,
      selectedContextId: null,
      activePreview: null,
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
      isPreviewOpen: false,
    });
    get().fetchLauncherData();
  },

  closeLauncher: () => {
    if (previewDebounceTimer) {
      clearTimeout(previewDebounceTimer);
      previewDebounceTimer = null;
    }
    set({
      isOpen: false,
      searchQuery: '',
      activeFilter: 'all',
      selectedIndex: 0,
      selectedContextId: null,
      activePreview: null,
      isResuming: false,
      resumeStatus: 'idle',
      resumeError: null,
      notReadyReason: null,
      previewError: null,
      isPreviewOpen: false,
    });
  },

  toggleLauncher: () => {
    if (get().isOpen) {
      get().closeLauncher();
    } else {
      get().openLauncher();
    }
  },

  setSearchQuery: (query: string) => {
    set({ searchQuery: query, selectedIndex: 0, isPreviewOpen: false });
  },

  setActiveFilter: (filter: ContextFilterType) => {
    set({ activeFilter: filter, selectedIndex: 0, isPreviewOpen: false });
  },

  setActiveSort: (sort: ContextSortOrder) => {
    saveLauncherPreferences({ sortOrder: sort });
    set({ activeSort: sort, isPreviewOpen: false });
  },

  toggleActiveSort: () => {
    const nextSort: ContextSortOrder = get().activeSort === 'recent' ? 'name' : 'recent';
    saveLauncherPreferences({ sortOrder: nextSort });
    set({ activeSort: nextSort, isPreviewOpen: false });
  },

  setSelectedIndex: (index: number) => {
    set({ selectedIndex: index });
  },

  setIsPreviewOpen: (open: boolean) => {
    set({ isPreviewOpen: open });
  },

  clearResumeState: () => {
    set({
      isResuming: false,
      resumeStatus: 'idle',
      resumeError: null,
      notReadyReason: null,
      actionError: null,
    });
  },

  clearActionError: () => {
    set({
      actionError: null,
      resumeError: null,
      notReadyReason: null,
    });
  },

  selectContextById: async (contextId: string) => {
    if (!contextId) return;
    set({
      selectedContextId: contextId,
      isLoadingPreview: true,
      previewError: null,
      resumeStatus: 'idle',
      resumeError: null,
      actionError: null,
      notReadyReason: null,
    });

    if (previewDebounceTimer) {
      clearTimeout(previewDebounceTimer);
    }

    previewDebounceTimer = setTimeout(async () => {
      try {
        const existingWorkspaces = useWorkspaceStore.getState().workspaces;
        const wsMeta = existingWorkspaces.find((w) => w.id === contextId);

        // Fetch state, workset, and readiness concurrently from authoritative engine APIs
        const [stateRes, worksetRes, readinessRes] = await Promise.allSettled([
          contextInteractionService.getContextState(contextId),
          contextInteractionService.getContextWorkset(contextId),
          contextInteractionService.getResumeReadiness(contextId),
        ]);

        const stateData = stateRes.status === 'fulfilled' ? stateRes.value : null;
        const worksetData = worksetRes.status === 'fulfilled' ? worksetRes.value : null;
        const readinessData = readinessRes.status === 'fulfilled' ? readinessRes.value : null;

        const name =
          wsMeta?.name ||
          stateData?.context_name ||
          worksetData?.context_name ||
          readinessData?.context_name ||
          'Context';

        const rawState = (stateData?.state || wsMeta?.lifecycle_state || 'recent').toLowerCase();
        const isAmbiguous = rawState === 'uncertain' || rawState === 'insufficient_evidence';
        const isStale = rawState === 'dormant' || rawState === 'archived' || rawState === 'very_stale';

        // Extract and categorize workset items
        const previewItems: LauncherPreviewItem[] = [];
        const importantItems: LauncherPreviewItem[] = [];

        if (worksetData && Array.isArray(worksetData.items)) {
          for (const item of worksetData.items.slice(0, 6)) {
            const isImportant = item.relevance === 'primary';
            const mapped: LauncherPreviewItem = {
              id: item.id,
              title: item.title || item.content_snippet || 'Captured Item',
              itemType: item.item_type || 'note',
              relevance: item.relevance,
              isImportant,
              appName: item.app_name,
              domain: item.domain,
              snippet: item.content_snippet,
            };
            if (isImportant && importantItems.length < 2) {
              importantItems.push(mapped);
            }
            previewItems.push(mapped);
          }
        }

        // Extract readiness
        const readinessBand =
          readinessData?.readiness?.readiness ||
          stateData?.readiness?.readiness ||
          (isStale ? 'partial' : 'ready');

        const readinessSummary =
          readinessData?.formatted_resume_summary ||
          stateData?.resume_state?.formatted_resume_summary ||
          'Ready to continue working in this context.';

        const readinessExplanation =
          readinessData?.readiness?.formatted_explanation ||
          stateData?.readiness?.formatted_explanation ||
          undefined;

        const reasons =
          readinessData?.readiness?.reasons ||
          stateData?.readiness?.reasons ||
          [];

        const taskTitle =
          stateData?.task_title ||
          worksetData?.task_title ||
          readinessData?.left_off_on ||
          null;

        const targetContextItem =
          (get().activeContext?.id === contextId ? get().activeContext : null) ||
          get().recentContexts.find((c) => c.id === contextId);

        const projectIdentifier =
          targetContextItem?.projectIdentifier ||
          (wsMeta as any)?.project_identifier ||
          (stateData as any)?.project_identifier ||
          null;

        const repository =
          targetContextItem?.repository ||
          (wsMeta as any)?.repository ||
          (wsMeta as any)?.repo_name ||
          (stateData as any)?.repository ||
          null;

        const workspacePath =
          targetContextItem?.workspacePath ||
          (wsMeta as any)?.path ||
          null;

        const description =
          targetContextItem?.description ||
          (wsMeta as any)?.description ||
          null;

        const isPinned = Boolean(
          targetContextItem?.isPinned ??
          (wsMeta as any)?.is_pinned ??
          false
        );

        const previewModel: LauncherPreviewModel = {
          contextId,
          name,
          state: rawState,
          isPinned,
          description,
          projectIdentifier,
          repository,
          workspacePath,
          taskTitle,
          lastActiveAt: stateData?.last_active_at || wsMeta?.updated_at || null,
          durationEstimate: stateData?.freshness_display || null,
          readinessBand,
          readinessSummary,
          readinessExplanation,
          reasons,
          importantItems,
          worksetItems: previewItems,
          itemCount: worksetData?.items?.length || previewItems.length,
          activitySummary: stateData?.last_meaningful_activity || readinessData?.last_meaningful_activity || null,
          isAmbiguous,
          isStale,
        };

        // Only update if this context is still selected
        if (get().selectedContextId === contextId) {
          set({
            activePreview: previewModel,
            isLoadingPreview: false,
            previewError: null,
          });
        }
      } catch (err) {
        if (get().selectedContextId === contextId) {
          set({
            isLoadingPreview: false,
            previewError: err instanceof Error ? err.message : 'Context unavailable or failed to load preview',
          });
        }
      }
    }, 40);
  },

  fetchLauncherData: async () => {
    set({ isLoading: true, error: null });
    try {
      const existingWorkspaces = useWorkspaceStore.getState().workspaces;
      const workspaceMap = new Map(existingWorkspaces.map((w) => [w.id, w]));

      // 1. Fetch active context from the single source of truth (Context Engine)
      let activeItem: LauncherContextItem | null = null;
      try {
        const engineActive = await contextInteractionService.getActiveContext();
        if (engineActive && (engineActive.context_id || engineActive.id)) {
          const id = engineActive.context_id || engineActive.id;
          const wsMeta = workspaceMap.get(id);
          const name = wsMeta?.name || engineActive.context || engineActive.name || 'Active Context';

          let readiness: LauncherResumeReadiness | null = null;
          try {
            const readinessRes = await contextInteractionService.getResumeReadiness(id);
            if (readinessRes) {
              readiness = {
                readiness: readinessRes.readiness?.readiness || readinessRes.state || 'ready',
                formattedExplanation: readinessRes.readiness?.formatted_explanation || readinessRes.formatted_resume_summary,
                summary: readinessRes.formatted_resume_summary,
              };
            }
          } catch (readinessErr) {
            console.debug('Could not fetch resume readiness:', readinessErr);
          }

          let activitySummary: string | null = null;
          if (engineActive.recent_activity) {
            activitySummary = engineActive.recent_activity;
          } else if (Array.isArray(engineActive.evidence) && engineActive.evidence.length > 0) {
            activitySummary = engineActive.evidence[0];
          } else if (engineActive.memory_statement) {
            activitySummary = engineActive.memory_statement;
          }

          activeItem = {
            id,
            name,
            description: (wsMeta as any)?.description || (engineActive as any)?.description || null,
            projectIdentifier: (wsMeta as any)?.project_identifier || (engineActive as any)?.project_identifier || null,
            repository: (wsMeta as any)?.repository || (engineActive as any)?.repository || (wsMeta as any)?.repo_name || null,
            isActive: true,
            isPinned: Boolean((wsMeta as any)?.is_pinned),
            state: wsMeta?.lifecycle_state || engineActive.state || 'active',
            lastActiveAt: engineActive.last_active_at || wsMeta?.updated_at || null,
            taskTitle: engineActive.task || (engineActive.recent_task ? engineActive.recent_task.title : null),
            confidence: engineActive.context_confidence || undefined,
            itemCount: Array.isArray(engineActive.recent_items) ? engineActive.recent_items.length : undefined,
            recentActivitySummary: activitySummary,
            memoryStatement: engineActive.memory_statement || null,
            resumeReadiness: readiness,
            createdAt: wsMeta?.created_at || null,
          };
        }
      } catch (err) {
        console.warn('Could not fetch active context from engine:', err);
      }

      // 2. Fetch recent contexts strictly preserving engine's recency order
      const recents: LauncherContextItem[] = [];
      const seenIds = new Set<string>();
      if (activeItem) {
        seenIds.add(activeItem.id);
      }

      try {
        const engineRecents = await contextInteractionService.getRecentContexts(7);
        if (Array.isArray(engineRecents)) {
          for (const entry of engineRecents) {
            const id = entry.context_id || entry.id;
            if (!id || seenIds.has(id)) continue;
            seenIds.add(id);

            const wsMeta = workspaceMap.get(id);
            const contextName = entry.context_name || entry.name || wsMeta?.name || 'Unnamed Context';

            recents.push({
              id,
              name: contextName,
              description: (wsMeta as any)?.description || (entry as any)?.description || null,
              projectIdentifier: (wsMeta as any)?.project_identifier || (entry as any)?.project_identifier || null,
              repository: (wsMeta as any)?.repository || (entry as any)?.repository || (wsMeta as any)?.repo_name || null,
              isActive: false,
              isPinned: Boolean((wsMeta as any)?.is_pinned || (entry as any)?.is_pinned),
              state: entry.state || entry.transition_type || wsMeta?.lifecycle_state || 'recent',
              lastActiveAt: entry.last_active_at || entry.ended_at || wsMeta?.updated_at || null,
              durationEstimate: entry.duration_estimate || null,
              taskTitle: entry.task_title || entry.title || null,
              itemCount: entry.item_count ?? (Array.isArray(entry.recent_items) ? entry.recent_items.length : undefined),
              sessionCount: entry.session_count,
              confidence: entry.task_confidence_band || undefined,
              createdAt: wsMeta?.created_at || (entry as any)?.created_at || null,
            });
          }
        }
      } catch (err) {
        console.warn('Could not fetch recent contexts from engine:', err);
      }

      // 3. Complement recents with any remaining stored workspaces (preserving recents priority first)
      if (existingWorkspaces.length > 0) {
        for (const ws of existingWorkspaces) {
          if (seenIds.has(ws.id)) continue;
          seenIds.add(ws.id);
          recents.push({
            id: ws.id,
            name: ws.name,
            isActive: ws.id === activeItem?.id,
            isPinned: Boolean(ws.is_pinned),
            state: ws.lifecycle_state || 'dormant',
            lastActiveAt: ws.last_activity_at || ws.updated_at || ws.created_at,
            createdAt: ws.created_at || null,
          });
        }
      }

      set({
        activeContext: activeItem,
        recentContexts: recents,
        isLoading: false,
        selectedIndex: 0,
      });

      // Auto-preview top item if available and not currently displaying an action error
      const firstId = activeItem?.id || (recents.length > 0 ? recents[0].id : null);
      if (firstId && !get().actionError) {
        get().selectContextById(firstId);
      }
    } catch (err) {
      set({
        isLoading: false,
        error: err instanceof Error ? err.message : 'Failed to load contexts from engine',
      });
    }
  },

  resumeContextById: async (contextId: string, force: boolean = false): Promise<boolean> => {
    const result = await get().executeContextAction('context.resume', contextId, { force });
    return result.success;
  },

  renameContextById: async (contextId: string, newName: string) => {
    return await get().executeContextAction('context.rename', contextId, { newName });
  },

  executeContextAction: async (
    actionId: ContextActionId,
    contextIdOrOptions?: string | ExecuteContextActionOptions,
    maybeOptions?: ExecuteContextActionOptions
  ) => {
    // 1. Concurrency guard: Prevent duplicate submissions while an action is in progress
    if (get().isActionExecuting || (actionId === 'context.resume' && get().isResuming)) {
      return {
        actionId,
        success: false,
        outcome: 'BLOCKED',
        message: 'An action is already in progress.',
      };
    }

    let targetId: string | undefined;
    let effectiveOptions: ExecuteContextActionOptions | undefined;

    if (typeof contextIdOrOptions === 'object' && contextIdOrOptions !== null) {
      effectiveOptions = contextIdOrOptions as ExecuteContextActionOptions;
      targetId = get().selectedContextId || get().activePreview?.contextId || undefined;
    } else {
      targetId =
        typeof contextIdOrOptions === 'string'
          ? contextIdOrOptions
          : get().selectedContextId || get().activePreview?.contextId || undefined;
      effectiveOptions = maybeOptions;
    }

    if (!targetId || targetId.trim().length === 0) {
      const isExplicitEmpty =
        typeof contextIdOrOptions === 'string' && contextIdOrOptions.trim().length === 0;
      return {
        actionId,
        success: false,
        outcome: 'ERROR',
        error: isExplicitEmpty ? 'Context ID cannot be empty' : 'No context selected',
        message: 'No context selected',
      };
    }

    if (actionId === 'context.resume') {
      set({
        isResuming: true,
        resumeStatus: 'loading',
        resumeError: null,
        notReadyReason: null,
      });
    }
    set({
      isActionExecuting: true,
      executingActionId: actionId,
      executingTargetId: targetId,
      actionError: null,
    });

    try {
      const preview = get().activePreview;
      const result = await quickActionsService.executeContextAction(actionId, targetId, {
        force: effectiveOptions?.force,
        lifecycleState: effectiveOptions?.lifecycleState || preview?.state,
        readinessBand: effectiveOptions?.readinessBand || preview?.readinessBand,
        readinessExplanation: effectiveOptions?.readinessExplanation || preview?.readinessExplanation,
        newName: effectiveOptions?.newName,
      });

      if (actionId === 'context.resume') {
        set({
          isResuming: false,
          resumeStatus: result.success ? 'success' : result.outcome === 'BLOCKED' ? 'not_ready' : 'error',
          resumeError: result.outcome === 'ERROR' ? (result.error || result.message || 'Failed to resume') : null,
        });
      }

      if (result.success) {
        if (actionId === 'context.resume') {
          // Refresh launcher data only after operation succeeds
          await get().fetchLauncherData();
          // Display success feedback only after receiving confirmation
          const preview = get().activePreview;
          const contextName = preview?.name || result.resolvedContextId || 'Context';
          toast.success(result.message || `Resumed "${contextName}"`);
          get().closeLauncher();
        } else if (actionId === 'context.rename') {
          const newName = (effectiveOptions?.newName || '').trim();
          if (newName) {
            if (get().activePreview?.contextId === targetId) {
              set((state) => ({
                activePreview: state.activePreview ? { ...state.activePreview, name: newName } : null,
              }));
            }
            if (get().activeContext?.id === targetId) {
              set((state) => ({
                activeContext: state.activeContext ? { ...state.activeContext, name: newName } : null,
              }));
            }
            set((state) => ({
              recentContexts: state.recentContexts.map((c) => (c.id === targetId ? { ...c, name: newName } : c)),
            }));
          }
          await get().fetchLauncherData();
          toast.success(result.message || 'Context renamed');
        } else if (actionId === 'context.pin' || actionId === 'context.unpin') {
          const newPinnedState = actionId === 'context.pin';
          if (get().activePreview?.contextId === targetId) {
            set((state) => ({
              activePreview: state.activePreview ? { ...state.activePreview, isPinned: newPinnedState } : null,
            }));
          }
          if (get().activeContext?.id === targetId) {
            set((state) => ({
              activeContext: state.activeContext ? { ...state.activeContext, isPinned: newPinnedState } : null,
            }));
          }
          set((state) => ({
            recentContexts: state.recentContexts.map((c) => (c.id === targetId ? { ...c, isPinned: newPinnedState } : c)),
          }));
          await get().fetchLauncherData();
          if (get().selectedContextId === targetId) {
            await get().selectContextById(targetId);
          }
          toast.success(result.message || (newPinnedState ? 'Context pinned' : 'Context unpinned'));
        } else if (actionId === 'context.pause' || actionId === 'context.archive') {
          await get().fetchLauncherData();
          if (get().selectedContextId === targetId) {
            await get().selectContextById(targetId);
          }
          toast.success(result.message || 'Action completed');
        } else if (actionId === 'context.open_details') {
          get().closeLauncher();
        }
      } else if (result.outcome === 'BLOCKED') {
        if (actionId === 'context.resume' && !result.message?.startsWith('Action is no longer valid:')) {
          set({
            isResuming: false,
            resumeStatus: 'not_ready',
            notReadyReason: result.message || 'Context not ready',
          });
        } else {
          toast.info(result.message || 'Action is no longer valid');
          set({
            actionError: {
              actionId,
              title: getActionErrorTitle(actionId),
              message: result.message || 'Action is no longer valid in current state',
              targetId,
              isRetryable: false,
            },
          });
        }

        // When an action is blocked due to lifecycle transition invalidation or state change during interaction,
        // refresh launcher data and selected preview to resync the UI with actual context state.
        if (result.message?.startsWith('Action is no longer valid:')) {
          await get().fetchLauncherData();
          if (get().selectedContextId === targetId) {
            await get().selectContextById(targetId);
          }
        }
      } else if (result.outcome === 'ERROR') {
        const errorMsg = result.error || result.message || 'Action failed';
        const fullMsg = (result.message || '') + ' ' + (result.error || '');
        const isUnavailable =
          fullMsg.includes('no longer available') ||
          fullMsg.includes('not found') ||
          fullMsg.includes('does not exist');

        set({
          isResuming: false,
          resumeStatus: 'error',
          resumeError: actionId === 'context.resume' ? errorMsg : null,
          actionError: {
            actionId,
            title: isUnavailable ? 'Context Unavailable' : getActionErrorTitle(actionId),
            message: errorMsg,
            targetId,
            isRetryable: !isUnavailable,
          },
        });
        toast.error(result.message || errorMsg);

        // Handle contexts that become unavailable between selection and execution
        if (isUnavailable) {
          if (get().activePreview?.contextId === targetId) {
            set({ activePreview: null });
          }
          await get().fetchLauncherData();
          if (get().selectedContextId === targetId) {
            set({ selectedContextId: null });
          }
        }
      }

      return result;
    } finally {
      set({
        isActionExecuting: false,
        executingActionId: null,
        executingTargetId: null,
      });
    }
  },

  executeItemAction: async (actionId: ItemActionId, contextId: string, itemId: string) => {
    // Concurrency guard: Prevent duplicate submissions while an action is in progress
    if (get().isActionExecuting) {
      return {
        actionId,
        success: false,
        outcome: 'BLOCKED',
        message: 'An action is already in progress.',
      };
    }

    set({
      isActionExecuting: true,
      executingActionId: actionId,
      executingTargetId: itemId,
      actionError: null,
    });
    try {
      const result = await quickActionsService.executeItemAction(actionId, contextId, itemId);
      if (result.success) {
        toast.success(result.message || 'Item updated');
        if (get().selectedContextId === contextId) {
          await get().selectContextById(contextId);
        }
      } else {
        const errorMsg = result.message || result.error || 'Item action failed';
        set({
          actionError: {
            actionId,
            title: getActionErrorTitle(actionId),
            message: errorMsg,
            targetId: itemId,
            isRetryable: true,
          },
        });
        toast.error(errorMsg);
      }
      return result;
    } finally {
      set({
        isActionExecuting: false,
        executingActionId: null,
        executingTargetId: null,
      });
    }
  },
}));
