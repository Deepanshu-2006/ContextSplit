import { create } from 'zustand';
import { db, Workspace, Item } from '../services/db';
import { confirm } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';

export type CaptureTargetState = 'hidden' | 'visible' | 'approaching' | 'active' | 'success' | 'failure';

interface WorkspaceState {
  workspaces: Workspace[];
  activeWorkspaceId: string | "inbox" | null;
  activeItems: Item[];
  inboxCount: number;
  isLoading: boolean;
  captureState: CaptureTargetState;
  loadWorkspaces: () => Promise<void>;
  loadItems: (workspaceId: string | "inbox" | null) => Promise<void>;
  createWorkspace: (name: string) => Promise<void>;
  selectWorkspace: (id: string | "inbox") => void;
  renameWorkspace: (id: string, newName: string) => Promise<void>;
  deleteWorkspace: (id: string) => Promise<void>;
  listenForClipboard: () => Promise<() => void>;
  setCaptureState: (state: CaptureTargetState) => void;
  updateItemPosition: (id: string, x: number, y: number) => Promise<void>;
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  workspaces: [],
  activeWorkspaceId: null,
  activeItems: [],
  inboxCount: 0,
  isLoading: true,
  captureState: 'hidden',

  setCaptureState: (state) => set({ captureState: state }),

  loadWorkspaces: async () => {
    try {
      const [workspaces, inboxItems] = await Promise.all([
        db.listWorkspaces(),
        db.listItems(null)
      ]);
      
      const defaultActiveId = workspaces.length > 0 ? workspaces[0].id : null;
      
      set({
        workspaces,
        inboxCount: inboxItems.length,
        activeWorkspaceId: defaultActiveId,
        isLoading: false,
      });
      
      if (defaultActiveId) {
        get().loadItems(defaultActiveId);
      }
    } catch (err) {
      console.error("Failed to load workspaces:", err);
      set({ isLoading: false });
    }
  },

  loadItems: async (workspaceId: string | "inbox" | null) => {
    if (!workspaceId) {
      set({ activeItems: [] });
      return;
    }
    try {
      const idToFetch = workspaceId === "inbox" ? null : workspaceId;
      const items = await db.listItems(idToFetch);
      set({ activeItems: items });
      
      if (workspaceId === "inbox") {
        set({ inboxCount: items.length });
      }
    } catch (err) {
      console.error("Failed to load items:", err);
    }
  },

  updateItemPosition: async (id: string, x: number, y: number) => {
    try {
      const state = get();
      const item = state.activeItems.find(i => i.id === id);
      if (!item) return;

      let meta = {};
      try {
        meta = JSON.parse(item.metadata || '{}');
      } catch (e) {}

      meta = { ...meta, pos_x: x, pos_y: y };
      const newMetadataStr = JSON.stringify(meta);

      // Optimistic UI update
      set((state) => ({
        activeItems: state.activeItems.map(i => 
          i.id === id ? { ...i, metadata: newMetadataStr } : i
        )
      }));

      // Background DB sync
      await db.updateItem(id, undefined, undefined, newMetadataStr);
    } catch (err) {
      console.error("Failed to update item position:", err);
    }
  },

  createWorkspace: async (name: string) => {
    try {
      const newWs = await db.createWorkspace(name);
      set((state) => ({
        workspaces: [newWs, ...state.workspaces],
      }));
      get().selectWorkspace(newWs.id);
    } catch (err) {
      console.error("Failed to create workspace:", err);
    }
  },

  selectWorkspace: (id: string | "inbox") => {
    set({ activeWorkspaceId: id });
    get().loadItems(id);
  },

  renameWorkspace: async (id: string, newName: string) => {
    try {
      const updated = await db.updateWorkspace(id, newName);
      set((state) => ({
        workspaces: state.workspaces.map((ws) => (ws.id === id ? updated : ws)),
      }));
    } catch (err) {
      console.error("Failed to rename workspace:", err);
    }
  },

  deleteWorkspace: async (id: string) => {
    const isConfirmed = await confirm(
      "Are you sure you want to delete this workspace? This action cannot be undone.",
      { title: "Delete Workspace", kind: "warning" }
    );
    
    if (!isConfirmed) return;

    try {
      await db.deleteWorkspace(id);
      const state = get();
      const filtered = state.workspaces.filter((ws) => ws.id !== id);
      
      let nextActiveId = state.activeWorkspaceId;
      if (state.activeWorkspaceId === id) {
        nextActiveId = filtered.length > 0 ? filtered[0].id : null;
      }
      
      set({
        workspaces: filtered,
        activeWorkspaceId: nextActiveId,
      });
      
      if (nextActiveId) {
        get().loadItems(nextActiveId);
      } else {
        set({ activeItems: [] });
      }
    } catch (err) {
      console.error("Failed to delete workspace:", err);
    }
  },

  listenForClipboard: async () => {
    const unlisten = await listen<Item>('clipboard://new-item', (event) => {
      const newItem = event.payload;
      const state = get();
      
      // Prevent duplicates by checking if the item already exists
      if (state.activeWorkspaceId === "inbox" && state.activeItems.some(item => item.id === newItem.id)) {
        return;
      }
      
      set({ inboxCount: state.inboxCount + 1 });
      
      if (state.activeWorkspaceId === "inbox") {
        set({ activeItems: [newItem, ...state.activeItems] });
      }
    });
    return unlisten;
  }
}));
