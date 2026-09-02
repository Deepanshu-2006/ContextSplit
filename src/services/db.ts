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
  total_score: number;
  confidence: number;
  confidence_band: string;
  reasons: string[];
}

export interface ItemSemanticStatus {
  item_id: string;
  status: "pending" | "processing" | "ready" | "failed";
  retries: number;
  model_version?: string;
  updated_at: string;
}

export interface ItemEmbeddingMetadata {
  item_id: string;
  provider: string;
  model: string;
  created_at: string;
  dimensions: number;
}

export interface PrivacySettings {
  enable_active_app: boolean;
  enable_window_title: boolean;
  enable_browser_domain: boolean;
  enable_session_tracking: boolean;
  enable_semantic_processing: boolean;
  semantic_provider: string;
  enable_semantic_suggestions: boolean;
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

// --- Context Discovery ---

export type CandidateStatus = "detected" | "suggested" | "accepted" | "dismissed" | "expired";

export interface ContextCandidate {
  id: string;
  /** "detected" | "suggested" | "accepted" | "dismissed" | "expired" */
  status: CandidateStatus;
  /** 0.0 – 1.0 */
  confidence: number;
  /** Human-readable inferred label, e.g. "KitKat project" */
  label: string | null;
  /** JSON string with detection metadata */
  metadata: string;
  created_at: string;
  updated_at: string;
}

export interface ContextCandidateItem {
  candidate_id: string;
  item_id: string;
  /** How central this item is to the candidate (0.0 – 1.0) */
  relevance_score: number;
  added_at: string;
}

export interface CandidateMember {
  membership: ContextCandidateItem;
  item: Item;
}

export interface CandidateWithItems {
  candidate: ContextCandidate;
  items: CandidateMember[];
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
    return await invoke("score_item", { itemId });
  },

  async getStrongestRelationships(itemId: string, limit: number = 5): Promise<ItemRelationship[]> {
    return await invoke("get_strongest_relationships", { itemId, limit });
  },

  async getItemSemanticStatus(id: string): Promise<ItemSemanticStatus | null> {
    return await invoke("get_item_semantic_status", { id });
  },

  async getItemEmbeddingMetadata(id: string): Promise<ItemEmbeddingMetadata | null> {
    return await invoke("get_item_embedding_metadata", { id });
  },

  // Settings
  getSettings: () => invoke<PrivacySettings>('get_settings'),
  updateSettings: (settings: PrivacySettings) => invoke<void>('update_settings', { settings }),
  
  deleteSemanticData: () => invoke<void>('delete_semantic_data'),
  rebuildSemanticData: () => invoke<void>('rebuild_semantic_data'),

  // --- Context Discovery ---
  /** Trigger the discovery algorithm. Returns newly created candidates only. */
  runContextDiscovery: () => invoke<ContextCandidate[]>('run_context_discovery'),

  /** List all candidates, optionally filtered by status. */
  listContextCandidates: (statusFilter?: CandidateStatus) =>
    invoke<ContextCandidate[]>('list_context_candidates', { statusFilter: statusFilter ?? null }),

  /** Get a single candidate by ID. */
  getContextCandidate: (id: string) =>
    invoke<ContextCandidate>('get_context_candidate', { id }),

  /** Get a candidate together with its member items. */
  getCandidateItems: (candidateId: string) =>
    invoke<CandidateWithItems>('get_candidate_items', { candidateId }),

  /** Advance the candidate status (e.g. detected → suggested → accepted). */
  updateCandidateStatus: (id: string, status: CandidateStatus) =>
    invoke<ContextCandidate>('update_candidate_status', { id, status }),

  /** Expire stale detected/suggested candidates older than maxAgeSecs (default 72h). */
  expireStateCandidates: (maxAgeSecs?: number) =>
    invoke<number>('expire_stale_candidates', { maxAgeSecs: maxAgeSecs ?? null }),
};

