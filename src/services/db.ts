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
  item_type: string;
  content: string;
  title: string | null;
  metadata: string;
  created_at: string;
  updated_at: string;
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
  }
};
