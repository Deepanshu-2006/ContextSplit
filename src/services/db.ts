import { invoke } from "@tauri-apps/api/core";

export interface Workspace {
  id: string;
  name: string;
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
  }
};
