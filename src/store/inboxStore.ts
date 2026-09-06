import { create } from 'zustand';
import {
  db,
  Item,
  InboxItemState,
  ItemAttention,
  ContextCandidate,
  CandidateWithItems,
} from '../services/db';

export type InboxSectionKey =
  | 'needs_attention'
  | 'new_candidates'
  | 'context_suggestions'
  | 'recently_captured'
  | 'already_understood';

export type InboxViewMode = 'sections' | 'chronological' | 'canvas';
export type InboxSortOrder = 'newest' | 'oldest';

export interface DerivedInboxItem {
  item: Item;
  section: InboxSectionKey;
  attention?: ItemAttention;
  inboxState?: InboxItemState;
  candidateInfo?: {
    candidateId: string;
    candidateName: string;
    confidence: number;
  };
}

interface InboxStoreState {
  viewMode: InboxViewMode;
  sortOrder: InboxSortOrder;
  collapsedSections: Record<InboxSectionKey, boolean>;
  searchQuery: string;
  selectedSectionFilter: InboxSectionKey | 'all';
  
  // Raw Data Cache
  attentionMap: Record<string, ItemAttention>;
  inboxStateMap: Record<string, InboxItemState>;
  candidateItemMap: Record<string, { candidateId: string; candidateName: string; confidence: number }>;
  isLoading: boolean;
  isBatchProcessing: boolean;
  
  // Actions
  setViewMode: (mode: InboxViewMode) => void;
  setSortOrder: (order: InboxSortOrder) => void;
  setSearchQuery: (query: string) => void;
  setSelectedSectionFilter: (filter: InboxSectionKey | 'all') => void;
  toggleSection: (section: InboxSectionKey) => void;
  setSectionCollapsed: (section: InboxSectionKey, collapsed: boolean) => void;
  
  fetchInboxAnalysis: (items: Item[]) => Promise<void>;
  acceptItemSuggestion: (itemId: string, workspaceId: string) => Promise<void>;
  dismissItemSuggestion: (itemId: string, workspaceId?: string) => Promise<void>;
  markItemUnderstood: (itemId: string, reason?: string) => Promise<void>;
  acceptAllHighConfidenceSuggestions: (items: Item[]) => Promise<number>;
}

export function deriveItemSection(
  item: Item,
  attention?: ItemAttention,
  inboxState?: InboxItemState,
  candidateInfo?: { candidateId: string; candidateName: string; confidence: number }
): InboxSectionKey {
  const state = inboxState?.state;

  // 1. Needs Attention (Highest Priority)
  if (
    state === 'needs_review' ||
    attention?.attention_required === true ||
    attention?.attention_level === 'high' ||
    attention?.factors?.is_ambiguous === true
  ) {
    return 'needs_attention';
  }

  // 2. New Context Candidates
  if (
    candidateInfo !== undefined ||
    attention?.factors?.has_new_candidate === true
  ) {
    return 'new_candidates';
  }

  // 3. Context Suggestions
  if (
    state === 'suggested' ||
    attention?.factors?.has_suggested_membership === true ||
    (attention?.factors?.suggested_workspace_id != null && attention.factors.suggested_workspace_id !== '')
  ) {
    return 'context_suggestions';
  }

  // 4. Recently Captured
  if (
    state === 'new' ||
    state === 'processing'
  ) {
    return 'recently_captured';
  }

  // If item is within fresh window (< 24h) and not marked understood/organized, treat as recently captured
  if (state === undefined) {
    const ageHours = (Date.now() - new Date(item.created_at).getTime()) / (1000 * 60 * 60);
    if (ageHours < 24) {
      return 'recently_captured';
    }
  }

  // 5. Already Understood (Default fallback for understood, organized, or mature processed items)
  return 'already_understood';
}

export const useInboxStore = create<InboxStoreState>((set, get) => ({
  viewMode: 'sections',
  sortOrder: 'newest',
  collapsedSections: {
    needs_attention: false,
    new_candidates: false,
    context_suggestions: false,
    recently_captured: false,
    already_understood: false,
  },
  searchQuery: '',
  selectedSectionFilter: 'all',
  
  attentionMap: {},
  inboxStateMap: {},
  candidateItemMap: {},
  isLoading: false,
  isBatchProcessing: false,

  setViewMode: (mode) => set({ viewMode: mode }),
  setSortOrder: (order) => set({ sortOrder: order }),
  setSearchQuery: (query) => set({ searchQuery: query }),
  setSelectedSectionFilter: (filter) => set({ selectedSectionFilter: filter }),
  
  toggleSection: (section) =>
    set((s) => ({
      collapsedSections: {
        ...s.collapsedSections,
        [section]: !s.collapsedSections[section],
      },
    })),

  setSectionCollapsed: (section, collapsed) =>
    set((s) => ({
      collapsedSections: {
        ...s.collapsedSections,
        [section]: collapsed,
      },
    })),

  fetchInboxAnalysis: async (items: Item[]) => {
    if (items.length === 0) {
      set({ attentionMap: {}, inboxStateMap: {}, candidateItemMap: {}, isLoading: false });
      return;
    }

    set({ isLoading: true });
    try {
      // 1. Fetch Attention evaluations in batch
      // 2. Fetch Inbox item states
      // 3. Fetch active ContextCandidates
      const [attentions, states, detectedCandidates, suggestedCandidates] = await Promise.all([
        db.evaluateInboxAttentionBatch().catch((err) => {
          console.warn('Could not batch evaluate attention:', err);
          return [] as ItemAttention[];
        }),
        db.listInboxItemStates().catch((err) => {
          console.warn('Could not list inbox item states:', err);
          return [] as InboxItemState[];
        }),
        db.listContextCandidates('detected').catch(() => [] as ContextCandidate[]),
        db.listContextCandidates('suggested').catch(() => [] as ContextCandidate[]),
      ]);

      // Map attentions
      const newAttentionMap: Record<string, ItemAttention> = {};
      for (const att of attentions) {
        newAttentionMap[att.item_id] = att;
      }

      // Map states
      const newStateMap: Record<string, InboxItemState> = {};
      for (const st of states) {
        newStateMap[st.item_id] = st;
      }

      // Fetch member items for active candidates
      const activeCandidates = [...detectedCandidates, ...suggestedCandidates];
      const newCandidateItemMap: Record<string, { candidateId: string; candidateName: string; confidence: number }> = {};

      const candidateDetailsPromises = activeCandidates.map(async (c) => {
        try {
          const details: CandidateWithItems = await db.getCandidateItems(c.id);
          for (const member of details.items) {
            newCandidateItemMap[member.item.id] = {
              candidateId: c.id,
              candidateName: c.label || 'Discovered Context',
              confidence: c.confidence,
            };
          }
        } catch (e) {
          // ignore candidate item fetch error
        }
      });

      await Promise.all(candidateDetailsPromises);

      set({
        attentionMap: newAttentionMap,
        inboxStateMap: newStateMap,
        candidateItemMap: newCandidateItemMap,
        isLoading: false,
      });
    } catch (err) {
      console.error('Failed to analyze inbox items:', err);
      set({ isLoading: false });
    }
  },

  acceptItemSuggestion: async (itemId: string, workspaceId: string) => {
    try {
      // 1. Confirm membership
      await db.confirmContextMembership(itemId, workspaceId);

      // 2. Transition state to 'organized'
      try {
        await db.transitionInboxItemState(itemId, 'organized', `Accepted suggestion for workspace ${workspaceId}`);
      } catch (e) {}

      // 3. Emit feedback signal
      try {
        await db.createContextSignal(
          workspaceId,
          itemId,
          'user_action',
          null,
          JSON.stringify({ action: 'suggestion_accepted', workspace_id: workspaceId })
        );
      } catch (e) {}
    } catch (err) {
      console.error('Failed to accept suggestion:', err);
      throw err;
    }
  },

  dismissItemSuggestion: async (itemId: string, workspaceId?: string) => {
    try {
      if (workspaceId) {
        await db.dismissContextMembership(itemId, workspaceId);
      }
      
      try {
        await db.transitionInboxItemState(itemId, 'understood', 'User dismissed workspace suggestion');
      } catch (e) {}

      try {
        await db.createContextSignal(
          workspaceId ?? null,
          itemId,
          'user_action',
          null,
          JSON.stringify({ action: 'suggestion_dismissed', workspace_id: workspaceId })
        );
      } catch (e) {}

      // Update local state map
      const stateMap = { ...get().inboxStateMap };
      if (stateMap[itemId]) {
        stateMap[itemId] = {
          ...stateMap[itemId],
          state: 'understood',
          reason: 'User dismissed suggestion',
          updated_at: new Date().toISOString(),
        };
        set({ inboxStateMap: stateMap });
      }
    } catch (err) {
      console.error('Failed to dismiss suggestion:', err);
      throw err;
    }
  },

  markItemUnderstood: async (itemId: string, reason?: string) => {
    try {
      const updated = await db.transitionInboxItemState(itemId, 'understood', reason || 'Marked as understood');
      const stateMap = { ...get().inboxStateMap, [itemId]: updated };
      set({ inboxStateMap: stateMap });
    } catch (err) {
      console.error('Failed to mark item understood:', err);
    }
  },

  acceptAllHighConfidenceSuggestions: async (items: Item[]) => {
    const { attentionMap, acceptItemSuggestion } = get();
    set({ isBatchProcessing: true });
    let count = 0;

    try {
      for (const item of items) {
        const att = attentionMap[item.id];
        const targetWsId = att?.factors?.suggested_workspace_id;
        if (targetWsId && (att?.factors?.contextual_confidence ?? 0) >= 0.70) {
          try {
            await acceptItemSuggestion(item.id, targetWsId);
            count++;
          } catch (e) {
            console.warn(`Failed to batch accept item ${item.id}:`, e);
          }
        }
      }
    } finally {
      set({ isBatchProcessing: false });
    }
    return count;
  },
}));
