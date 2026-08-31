import { invoke } from "@tauri-apps/api/core";

export interface Workspace {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
}

export interface Item {
  id: string;
  workspace_id: string | null;
  session_id: string | null;
  item_type: string;
  content: string;
  title: string | null;
  metadata: string;
  created_at: string;
  updated_at: string;
}

export interface ContextSignal {
  id: string;
  workspace_id: string | null;
  item_id: string | null;
  signal_type: string;
  session_id: string | null;
  payload: string;
  created_at: string;
  updated_at: string;
}

export interface WorkspaceScore {
  workspace_id: string;
  score: number;
  reasons: string[];
}

export interface ItemRelationship {
  id: string;
  item_a_id: string;
  item_b_id: string;
  relationship_type: string;
  strength: number;
  created_at: string;
  metadata: string | null;
}

export const db = {
  async createWorkspace(name: string): Promise<Workspace> {
    return invoke("create_workspace", { name });
  },

  async getWorkspace(id: string): Promise<Workspace> {
    return invoke("get_workspace", { id });
  },

  async listWorkspaces(): Promise<Workspace[]> {
    return invoke("list_workspaces");
  },

  async updateWorkspace(id: string, name: string): Promise<Workspace> {
    return invoke("update_workspace", { id, name });
  },

  async deleteWorkspace(id: string): Promise<void> {
    return invoke("delete_workspace", { id });
  },

  // Item operations
  async createItem(workspaceId: string | null, itemType: string, content: string, title: string | null = null, metadata: string = "{}"): Promise<Item> {
    return invoke("create_item", { workspaceId, itemType, content, title, metadata });
  },

  async getItem(id: string): Promise<Item> {
    return invoke("get_item", { id });
  },

  async listItems(workspaceId: string | null): Promise<Item[]> {
    return invoke("list_items", { workspaceId });
  },

  async updateItem(id: string, content?: string, title?: string, metadata?: string): Promise<Item> {
    return invoke("update_item", { id, content, title, metadata });
  },

  async moveItem(id: string, workspaceId: string | null): Promise<Item> {
    return invoke("move_item", { id, workspaceId });
  },

  async deleteItem(id: string): Promise<void> {
    return invoke("delete_item", { id });
  },

  // Context Signal operations
  async createContextSignal(
    workspaceId: string | null,
    itemId: string | null,
    signalType: string,
    sessionId: string | null,
    payload: string
  ): Promise<ContextSignal> {
    return invoke("create_context_signal", { workspaceId, itemId, signalType, sessionId, payload });
  },

  async getContextSignal(id: string): Promise<ContextSignal> {
    return invoke("get_context_signal", { id });
  },

  async listContextSignals(workspaceId: string | null = null, itemId: string | null = null): Promise<ContextSignal[]> {
    return invoke("list_context_signals", { workspaceId, itemId });
  },

  // Context Engine
  async scoreItem(itemId: string): Promise<WorkspaceScore[]> {
    return invoke("score_item", { itemId });
  },

  async getStrongestRelationships(itemId: string, limit: number): Promise<ItemRelationship[]> {
    return invoke("get_strongest_relationships", { itemId, limit });
  }
};
