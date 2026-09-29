import { create } from 'zustand';
import { contextInteractionService } from '../services/contextInteractionService';
import { useWorkspaceStore } from './workspaceStore';
import { toast } from 'sonner';

export interface LauncherResumeReadiness {
  readiness: 'ready' | 'mostly_ready' | 'partial' | 'insufficient' | string;
  formattedExplanation?: string;
  summary?: string;
}

export interface LauncherContextItem {
  id: string;
  name: string;
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
}

interface LauncherState {
  isOpen: boolean;
  searchQuery: string;
  selectedIndex: number;
  isLoading: boolean;
  error: string | null;
  activeContext: LauncherContextItem | null;
  recentContexts: LauncherContextItem[];
  
  // Actions
  openLauncher: () => void;
  closeLauncher: () => void;
  toggleLauncher: () => void;
  setSearchQuery: (query: string) => void;
  setSelectedIndex: (index: number) => void;
  fetchLauncherData: () => Promise<void>;
  resumeContextById: (contextId: string) => Promise<void>;
}

export const useLauncherStore = create<LauncherState>((set, get) => ({
  isOpen: false,
  searchQuery: '',
  selectedIndex: 0,
  isLoading: false,
  error: null,
  activeContext: null,
  recentContexts: [],

  openLauncher: () => {
    set({ isOpen: true, searchQuery: '', selectedIndex: 0, error: null });
    get().fetchLauncherData();
  },

  closeLauncher: () => {
    set({ isOpen: false, searchQuery: '', selectedIndex: 0 });
  },

  toggleLauncher: () => {
    if (get().isOpen) {
      get().closeLauncher();
    } else {
      get().openLauncher();
    }
  },

  setSearchQuery: (query: string) => {
    set({ searchQuery: query, selectedIndex: 0 });
  },

  setSelectedIndex: (index: number) => {
    set({ selectedIndex: index });
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

          // Try retrieving resume readiness for active context
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

          // Build clean activity summary from natural language statements
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
    } catch (err) {
      set({
        isLoading: false,
        error: err instanceof Error ? err.message : 'Failed to load contexts from engine',
      });
    }
  },

  resumeContextById: async (contextId: string) => {
    try {
      // Send resume signal to Context Engine
      await contextInteractionService.resumeContext(contextId);
      // Select workspace in local store for immediate UI canvas alignment
      useWorkspaceStore.getState().selectWorkspace(contextId);
      get().closeLauncher();
      toast.success('Context resumed');
    } catch (err) {
      console.error('Failed to resume context:', err);
      toast.error('Failed to resume context');
    }
  },
}));
