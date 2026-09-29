import { create } from 'zustand';
import { contextInteractionService } from '../services/contextInteractionService';
import { useWorkspaceStore } from './workspaceStore';
import { toast } from 'sonner';

export interface LauncherContextItem {
  id: string;
  name: string;
  isActive?: boolean;
  state?: string;
  lastActiveAt?: string | null;
  durationEstimate?: string | null;
  taskTitle?: string | null;
  itemCount?: number;
  confidence?: string;
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
      // 1. Fetch active context from the single source of truth (Context Engine)
      let activeItem: LauncherContextItem | null = null;
      try {
        const engineActive = await contextInteractionService.getActiveContext();
        if (engineActive && (engineActive.context_id || engineActive.id)) {
          const id = engineActive.context_id || engineActive.id;
          const name = engineActive.context || engineActive.name || 'Active Context';
          activeItem = {
            id,
            name,
            isActive: true,
            state: engineActive.state || 'active',
            lastActiveAt: engineActive.last_active_at || null,
            taskTitle: engineActive.task || (engineActive.recent_task ? engineActive.recent_task.title : null),
            confidence: engineActive.context_confidence || undefined,
            itemCount: Array.isArray(engineActive.recent_items) ? engineActive.recent_items.length : undefined,
          };
        }
      } catch (err) {
        console.warn('Could not fetch active context from engine:', err);
      }

      // 2. Fetch recent contexts from engine
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

            recents.push({
              id,
              name: entry.context_name || entry.name || 'Unnamed Context',
              isActive: false,
              state: entry.transition_type || 'recent',
              lastActiveAt: entry.ended_at || entry.last_active_at || null,
              durationEstimate: entry.duration_estimate || null,
              taskTitle: entry.task_title || entry.title || null,
              itemCount: entry.item_count ?? (Array.isArray(entry.recent_items) ? entry.recent_items.length : undefined),
              confidence: entry.task_confidence_band || undefined,
            });
          }
        }
      } catch (err) {
        console.warn('Could not fetch recent contexts from engine:', err);
      }

      // 3. Complement with workspaces from workspaceStore if engine returned empty recents
      const existingWorkspaces = useWorkspaceStore.getState().workspaces;
      if (recents.length === 0 && existingWorkspaces.length > 0) {
        for (const ws of existingWorkspaces) {
          if (seenIds.has(ws.id)) continue;
          seenIds.add(ws.id);
          recents.push({
            id: ws.id,
            name: ws.name,
            isActive: ws.id === activeItem?.id,
            state: ws.lifecycle_state || 'active',
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
