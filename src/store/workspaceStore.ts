import { create } from 'zustand';
import { db, Workspace } from '../services/db';
import { confirm } from '@tauri-apps/plugin-dialog';

interface WorkspaceState {
  workspaces: Workspace[];
  activeWorkspaceId: string | null;
  isLoading: boolean;
  loadWorkspaces: () => Promise<void>;
  createWorkspace: (name: string) => Promise<void>;
  selectWorkspace: (id: string) => void;
  renameWorkspace: (id: string, newName: string) => Promise<void>;
  deleteWorkspace: (id: string) => Promise<void>;
}

export const useWorkspaceStore = create<WorkspaceState>((set) => ({
  workspaces: [],
  activeWorkspaceId: null,
  isLoading: true,

  loadWorkspaces: async () => {
    try {
      const workspaces = await db.listWorkspaces();
      set({
        workspaces,
        activeWorkspaceId: workspaces.length > 0 ? workspaces[0].id : null,
        isLoading: false,
      });
    } catch (err) {
      console.error("Failed to load workspaces:", err);
      set({ isLoading: false });
    }
  },

  createWorkspace: async (name: string) => {
    try {
      const newWs = await db.createWorkspace(name);
      set((state) => ({
        workspaces: [newWs, ...state.workspaces],
        activeWorkspaceId: newWs.id,
      }));
    } catch (err) {
      console.error("Failed to create workspace:", err);
    }
  },

  selectWorkspace: (id: string) => {
    set({ activeWorkspaceId: id });
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
      set((state) => {
        const filtered = state.workspaces.filter((ws) => ws.id !== id);
        return {
          workspaces: filtered,
          // Select the first available workspace if the active one was deleted
          activeWorkspaceId:
            state.activeWorkspaceId === id
              ? filtered.length > 0
                ? filtered[0].id
                : null
              : state.activeWorkspaceId,
        };
      });
    } catch (err) {
      console.error("Failed to delete workspace:", err);
    }
  },
}));
