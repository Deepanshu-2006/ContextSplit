import { create } from 'zustand';
import { contextInteractionService } from '../services/contextInteractionService';
import { useWorkspaceStore } from './workspaceStore';
import { toast } from 'sonner';

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

/**
 * Extracts a deterministic numeric timestamp for recency tie-breaking.
 */
export function getContextRecencyTimestamp(item: LauncherContextItem): number {
  if (item.isActive) {
    if (item.lastActiveAt) {
      const parsed = new Date(item.lastActiveAt).getTime();
      if (!isNaN(parsed) && parsed > 0) return parsed;
    }
    return Date.now();
  }

  if (!item.lastActiveAt) return 0;
  const parsed = new Date(item.lastActiveAt).getTime();
  return isNaN(parsed) ? 0 : parsed;
}

/**
 * Sorts matching contexts deterministically based on:
 * 1. Exact context name match
 * 2. Context name starts with query
 * 3. Context name contains query
 * 4. Description/project metadata match
 * 5. Recency as a tie-breaker
 *
 * Secondary tie-breakers for equal recency:
 * - Shorter matching prefix token (e.g. PayX before Payment for query "pay")
 * - Shorter overall name length
 * - Deterministic alphabetical order by name
 */
export function sortContextMatches(items: LauncherContextItem[], query: string): LauncherContextItem[] {
  const q = query.trim().toLowerCase();
  if (!q || items.length <= 1) return items;

  // Decorate items with ranking tier, recency timestamp, and prefix token length once in O(N)
  // This avoids re-evaluating string metadata operations inside O(N log N) comparator calls
  const decorated = items.map((item) => {
    const tier = getContextSearchTier(item, q);
    const recency = getContextRecencyTimestamp(item);
    const wordLen = tier === 2 ? (item.name.split(/\s+/)[0] || item.name).length : 0;
    return { item, tier, recency, wordLen };
  });

  decorated.sort((a, b) => {
    // 1-4: Ranking tier
    if (a.tier !== b.tier) {
      return a.tier - b.tier;
    }

    // 5. Recency as a tie-breaker
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

interface LauncherState {
  isOpen: boolean;
  searchQuery: string;
  selectedIndex: number;
  selectedContextId: string | null;
  activePreview: LauncherPreviewModel | null;
  isLoading: boolean;
  isLoadingPreview: boolean;
  isResuming: boolean;
  resumeStatus: ResumeActionStatus;
  resumeError: string | null;
  notReadyReason: string | null;
  error: string | null;
  previewError: string | null;
  activeContext: LauncherContextItem | null;
  recentContexts: LauncherContextItem[];
  isPreviewOpen: boolean;
  activeFilter: ContextFilterType;
  
  // Actions
  openLauncher: () => void;
  closeLauncher: () => void;
  toggleLauncher: () => void;
  setSearchQuery: (query: string) => void;
  setActiveFilter: (filter: ContextFilterType) => void;
  setSelectedIndex: (index: number) => void;
  setIsPreviewOpen: (open: boolean) => void;
  selectContextById: (contextId: string) => Promise<void>;
  fetchLauncherData: () => Promise<void>;
  resumeContextById: (contextId: string, force?: boolean) => Promise<boolean>;
  clearResumeState: () => void;
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
  resumeStatus: 'idle',
  resumeError: null,
  notReadyReason: null,
  error: null,
  previewError: null,
  activeContext: null,
  recentContexts: [],
  isPreviewOpen: false,
  activeFilter: 'all',

  openLauncher: () => {
    set({
      isOpen: true,
      searchQuery: '',
      activeFilter: 'all',
      selectedIndex: 0,
      selectedContextId: null,
      activePreview: null,
      isResuming: false,
      resumeStatus: 'idle',
      resumeError: null,
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

        const previewModel: LauncherPreviewModel = {
          contextId,
          name,
          state: rawState,
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
          const name = engineActive.context || engineActive.name || workspaceMap.get(id)?.name || 'Active Context';

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

          const wsMeta = workspaceMap.get(id);

          activeItem = {
            id,
            name,
            description: (wsMeta as any)?.description || (engineActive as any)?.description || null,
            projectIdentifier: (wsMeta as any)?.project_identifier || (engineActive as any)?.project_identifier || null,
            repository: (wsMeta as any)?.repository || (engineActive as any)?.repository || (wsMeta as any)?.repo_name || null,
            isActive: true,
            state: wsMeta?.lifecycle_state || engineActive.state || 'active',
            lastActiveAt: engineActive.last_active_at || wsMeta?.updated_at || null,
            taskTitle: engineActive.task || (engineActive.recent_task ? engineActive.recent_task.title : null),
            confidence: engineActive.context_confidence || undefined,
            itemCount: Array.isArray(engineActive.recent_items) ? engineActive.recent_items.length : undefined,
            recentActivitySummary: activitySummary,
            memoryStatement: engineActive.memory_statement || null,
            resumeReadiness: readiness,
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
              state: wsMeta?.lifecycle_state || entry.transition_type || 'recent',
              lastActiveAt: entry.last_active_at || entry.ended_at || wsMeta?.updated_at || null,
              durationEstimate: entry.duration_estimate || null,
              taskTitle: entry.task_title || entry.title || null,
              itemCount: entry.item_count ?? (Array.isArray(entry.recent_items) ? entry.recent_items.length : undefined),
              sessionCount: entry.session_count,
              confidence: entry.task_confidence_band || undefined,
            });
          }
        }
      } catch (err) {
        console.warn('Could not fetch recent contexts from engine:', err);
      }

      // 3. Fallback: Complement with stored workspaces if engine returned empty recents
      if (recents.length === 0 && existingWorkspaces.length > 0) {
        for (const ws of existingWorkspaces) {
          if (seenIds.has(ws.id)) continue;
          seenIds.add(ws.id);
          recents.push({
            id: ws.id,
            name: ws.name,
            isActive: ws.id === activeItem?.id,
            state: ws.lifecycle_state || 'dormant',
            lastActiveAt: ws.updated_at || ws.created_at,
          });
        }
      }

      set({
        activeContext: activeItem,
        recentContexts: recents,
        isLoading: false,
        selectedIndex: 0,
      });

      // Auto-preview top item if available
      const firstId = activeItem?.id || (recents.length > 0 ? recents[0].id : null);
      if (firstId) {
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
    if (!contextId) return false;

    // Check resume readiness unless forced
    if (!force) {
      const preview = get().activePreview;
      if (preview && preview.contextId === contextId && preview.readinessBand === 'insufficient') {
        const reason =
          preview.readinessExplanation ||
          (preview.reasons.length > 0 ? preview.reasons.join('. ') : 'This context has insufficient recorded working state.');
        set({
          resumeStatus: 'not_ready',
          notReadyReason: reason,
          isResuming: false,
        });
        return false;
      }
    }

    set({
      isResuming: true,
      resumeStatus: 'loading',
      resumeError: null,
      notReadyReason: null,
    });

    try {
      // 1. Send resume signal to Context Engine (authoritative state transition)
      await contextInteractionService.resumeContext(contextId);

      // 2. Select workspace in local store for UI canvas alignment
      useWorkspaceStore.getState().selectWorkspace(contextId);

      set({
        resumeStatus: 'success',
        isResuming: false,
      });

      // 3. Refresh launcher state so active context updates
      await get().fetchLauncherData();

      // 4. Close launcher smoothly and show feedback
      const preview = get().activePreview;
      const contextName = preview?.name || 'Context';
      toast.success(`Resumed "${contextName}"`);
      get().closeLauncher();

      return true;
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to resume context from engine';
      set({
        isResuming: false,
        resumeStatus: 'error',
        resumeError: msg,
      });
      toast.error(msg);
      return false;
    }
  },
}));
