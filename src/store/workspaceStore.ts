import { create } from 'zustand';
import { toast } from 'sonner';
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
  isDraggingCard: boolean;
  hoveredWorkspaceId: string | null;
  loadWorkspaces: () => Promise<void>;
  loadItems: (workspaceId: string | "inbox" | null) => Promise<void>;
  createWorkspace: (name: string) => Promise<void>;
  selectWorkspace: (id: string | "inbox") => void;
  renameWorkspace: (id: string, newName: string) => Promise<void>;
  deleteWorkspace: (id: string) => Promise<void>;
  emptyInbox: () => Promise<void>;
  listenForClipboard: () => Promise<() => void>;
  setCaptureState: (state: CaptureTargetState) => void;
  updateItemPosition: (id: string, x: number, y: number) => Promise<void>;
  updateItemPositions: (updates: {id: string, x: number, y: number}[]) => Promise<void>;
  moveItemToWorkspace: (itemId: string, targetWorkspaceId: string | "inbox") => Promise<void>;
  moveItemsToWorkspace: (itemIds: string[], targetWorkspaceId: string | "inbox") => Promise<void>;
  setIsDraggingCard: (isDragging: boolean) => void;
  setHoveredWorkspaceId: (id: string | null) => void;
  isTimelineOpen: boolean;
  timelineWorkspaceId: string | null;
  openTimeline: (workspaceId?: string) => void;
  closeTimeline: () => void;
  isOverviewOpen: boolean;
  overviewWorkspaceId: string | null;
  openOverview: (workspaceId?: string) => void;
  closeOverview: () => void;
  isReconstructionInspectorOpen: boolean;
  reconstructionInspectorWorkspaceId: string | null;
  openReconstructionInspector: (workspaceId?: string) => void;
  closeReconstructionInspector: () => void;
  isDiagnosticsOpen: boolean;
  openDiagnostics: () => void;
  closeDiagnostics: () => void;
  toggleDiagnostics: () => void;
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  workspaces: [],
  activeWorkspaceId: "inbox",
  activeItems: [],
  inboxCount: 0,
  isLoading: true,
  captureState: 'hidden',
  isDraggingCard: false,
  hoveredWorkspaceId: null,
  isTimelineOpen: false,
  timelineWorkspaceId: null,
  isOverviewOpen: false,
  overviewWorkspaceId: null,
  isReconstructionInspectorOpen: false,
  reconstructionInspectorWorkspaceId: null,
  isDiagnosticsOpen: false,

  openTimeline: (workspaceId?: string) => {
    const targetId = workspaceId || (get().activeWorkspaceId !== 'inbox' ? (get().activeWorkspaceId as string) : null);
    if (targetId) {
      set({ isTimelineOpen: true, timelineWorkspaceId: targetId });
    }
  },
  closeTimeline: () => set({ isTimelineOpen: false, timelineWorkspaceId: null }),

  openOverview: (workspaceId?: string) => {
    const targetId = workspaceId || (get().activeWorkspaceId !== 'inbox' ? (get().activeWorkspaceId as string) : null);
    if (targetId) {
      console.log("Opening overview for", targetId);
      toast.info("Opening Overview Panel...");
      set({ isOverviewOpen: true, overviewWorkspaceId: targetId });
    } else {
      console.warn("Cannot open overview: No active workspace");
    }
  },
  closeOverview: () => set({ isOverviewOpen: false, overviewWorkspaceId: null }),

  openReconstructionInspector: (workspaceId?: string) => {
    const targetId = workspaceId || (get().activeWorkspaceId !== 'inbox' ? (get().activeWorkspaceId as string) : null);
    if (targetId) {
      set({ isReconstructionInspectorOpen: true, reconstructionInspectorWorkspaceId: targetId });
    }
  },
  closeReconstructionInspector: () => set({ isReconstructionInspectorOpen: false, reconstructionInspectorWorkspaceId: null }),

  openDiagnostics: () => set({ isDiagnosticsOpen: true }),
  closeDiagnostics: () => set({ isDiagnosticsOpen: false }),
  toggleDiagnostics: () => set((state) => ({ isDiagnosticsOpen: !state.isDiagnosticsOpen })),

  setCaptureState: (state) => set({ captureState: state }),
  setIsDraggingCard: (isDragging) => set({ isDraggingCard: isDragging }),
  setHoveredWorkspaceId: (id) => set({ hoveredWorkspaceId: id }),

  loadWorkspaces: async () => {
    try {
      const [workspaces, inboxItems] = await Promise.all([
        db.listWorkspaces(),
        db.listItems(null)
      ]);
      
      const defaultActiveId = workspaces.length > 0 ? workspaces[0].id : "inbox";
      
      set({
        workspaces,
        inboxCount: inboxItems.length,
        activeWorkspaceId: defaultActiveId,
        isLoading: false,
      });
      
      get().loadItems(defaultActiveId);
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
    get().updateItemPositions([{ id, x, y }]);
  },

  updateItemPositions: async (updates: {id: string, x: number, y: number}[]) => {
    if (updates.length === 0) return;

    try {
      const state = get();
      
      // Map of id -> new metadata string
      const newMetadatas: Record<string, string> = {};
      
      updates.forEach(update => {
        const item = state.activeItems.find(i => i.id === update.id);
        if (item) {
          let meta = {};
          try { meta = JSON.parse(item.metadata || '{}'); } catch (e) {}
          meta = { ...meta, pos_x: update.x, pos_y: update.y };
          newMetadatas[update.id] = JSON.stringify(meta);
        }
      });

      if (Object.keys(newMetadatas).length === 0) return;

      // Optimistic UI update
      set(state => ({
        activeItems: state.activeItems.map(i => 
          newMetadatas[i.id] ? { ...i, metadata: newMetadatas[i.id] } : i
        )
      }));

      // Background DB sync (concurrently)
      await Promise.all(
        Object.entries(newMetadatas).map(([id, metaStr]) => 
          db.updateItem(id, undefined, undefined, metaStr)
        )
      );
    } catch (err) {
      console.error("Failed to update item positions:", err);
    }
  },

  moveItemToWorkspace: async (itemId: string, targetWorkspaceId: string | "inbox") => {
    get().moveItemsToWorkspace([itemId], targetWorkspaceId);
  },

  moveItemsToWorkspace: async (itemIds: string[], targetWorkspaceId: string | "inbox") => {
    if (itemIds.length === 0) return;
    
    const state = get();
    const itemsToMove = state.activeItems.filter(i => itemIds.includes(i.id));
    
    if (itemsToMove.length === 0) return;

    try {
      // Optimistically remove from current view
      set(state => ({
        activeItems: state.activeItems.filter(item => !itemIds.includes(item.id))
      }));

      // Update inbox count if necessary
      if (state.activeWorkspaceId === "inbox") {
        set(state => ({ inboxCount: Math.max(0, state.inboxCount - itemsToMove.length) }));
      } else if (targetWorkspaceId === "inbox") {
        set(state => ({ inboxCount: state.inboxCount + itemsToMove.length }));
      }

      // Background DB sync (concurrently)
      const dbTargetId = targetWorkspaceId === "inbox" ? null : targetWorkspaceId;
      await Promise.all(itemsToMove.map(item => db.moveItem(item.id, dbTargetId)));
      
    } catch (err) {
      console.error("Failed to move items:", err);
      // Rollback the optimistic update
      set(state => ({
        activeItems: [...itemsToMove, ...state.activeItems]
      }));

      // Revert inbox count if necessary
      if (state.activeWorkspaceId === "inbox") {
        set(state => ({ inboxCount: state.inboxCount + itemsToMove.length }));
      } else if (targetWorkspaceId === "inbox") {
        set(state => ({ inboxCount: Math.max(0, state.inboxCount - itemsToMove.length) }));
      }
      
      alert("Failed to move items to workspace. Please try again.");
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

  emptyInbox: async () => {
    try {
      await db.emptyInbox();
      set({ inboxCount: 0 });
      if (get().activeWorkspaceId === "inbox") {
        set({ activeItems: [] });
      }
    } catch (err) {
      console.error("Failed to empty inbox:", err);
    }
  },

  listenForClipboard: async () => {
    const unlistenNew = await listen<Item>('clipboard://new-item', (event) => {
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

    const unlistenUpdated = await listen('inbox://updated', async () => {
      try {
        const inboxItems = await db.listItems(null);
        set({ inboxCount: inboxItems.length });
        if (get().activeWorkspaceId === "inbox") {
          set({ activeItems: inboxItems });
        }
      } catch (err) {
        console.error("Failed to refresh inbox:", err);
      }
    });

    return () => {
      unlistenNew();
      unlistenUpdated();
    };
  }
}));
