import { create } from 'zustand';
import {
  db,
  Item,
  InboxItemState,
  ItemAttention,
  ContextCandidate,
  CandidateWithItems,
  ContextReasoningResult,
  AIContextReasoningResponse,
  InboxSearchParams,
  InboxSearchResult,
} from '../services/db';
import { useWorkspaceStore } from './workspaceStore';

export type InboxSectionKey =
  | 'needs_attention'
  | 'new_candidates'
  | 'context_suggestions'
  | 'recently_captured'
  | 'already_understood';

export type InboxViewMode = 'sections' | 'grouped' | 'chronological' | 'canvas';
export type InboxSortOrder = 'newest' | 'oldest';
export type DatePreset = 'all' | 'today' | '7d' | '30d' | 'custom';

export interface GroupReasoningState {
  result: ContextReasoningResult | null;
  isLoading: boolean;
  isUnavailable: boolean;
  unavailabilityReason?: string | null;
  hasRun: boolean;
}

export interface DerivedInboxItem {
  item: Item;
  section: InboxSectionKey;
  attention?: ItemAttention;
  inboxState?: InboxItemState;
  candidateInfo?: {
    candidateId: string;
    candidateName: string;
    confidence: number;
    explanation?: string;
  };
  searchResult?: InboxSearchResult;
}

interface InboxStoreState {
  viewMode: InboxViewMode;
  sortOrder: InboxSortOrder;
  collapsedSections: Record<InboxSectionKey, boolean>;
  searchQuery: string;
  selectedSectionFilter: InboxSectionKey | 'all';
  
  // Search & Multi-Filter State
  filterWorkspaceId: string | null;
  filterCandidateId: string | null;
  filterItemType: string | null;
  filterDatePreset: DatePreset;
  filterDateFrom: string | null;
  filterDateTo: string | null;
  filterInboxState: string | null;
  isSemanticSearchActive: boolean;
  isFilterBarOpen: boolean;
  searchResults: InboxSearchResult[] | null;
  isSearching: boolean;

  // Raw Data Cache
  attentionMap: Record<string, ItemAttention>;
  inboxStateMap: Record<string, InboxItemState>;
  candidateMap: Record<string, ContextCandidate>;
  candidateItemMap: Record<
    string,
    { candidateId: string; candidateName: string; confidence: number; explanation?: string }
  >;
  groupReasoningMap: Record<string, GroupReasoningState>;
  isLoading: boolean;
  isBatchProcessing: boolean;
  
  // Multi-Select
  selectedItemIds: string[];
  isMultiSelectActive: boolean;

  // Actions
  setViewMode: (mode: InboxViewMode) => void;
  setSortOrder: (order: InboxSortOrder) => void;
  setSearchQuery: (query: string) => void;
  setSelectedSectionFilter: (filter: InboxSectionKey | 'all') => void;
  toggleSection: (section: InboxSectionKey) => void;
  setSectionCollapsed: (section: InboxSectionKey, collapsed: boolean) => void;

  // Filter Actions
  setFilterWorkspaceId: (workspaceId: string | null) => void;
  setFilterCandidateId: (candidateId: string | null) => void;
  setFilterItemType: (itemType: string | null) => void;
  setFilterDatePreset: (preset: DatePreset, from?: string | null, to?: string | null) => void;
  setFilterInboxState: (state: string | null) => void;
  setIsSemanticSearchActive: (active: boolean) => void;
  setIsFilterBarOpen: (open: boolean) => void;
  toggleFilterBar: () => void;
  resetAllFilters: () => void;
  executeSearch: () => Promise<void>;
  getActiveFilterCount: () => number;
  
  // Multi-Select Actions
  toggleSelectItem: (itemId: string) => void;
  selectItems: (itemIds: string[]) => void;
  deselectItems: (itemIds: string[]) => void;
  selectAllItems: (itemIds: string[]) => void;
  clearSelection: () => void;
  setMultiSelectActive: (active: boolean) => void;

  fetchInboxAnalysis: (items: Item[]) => Promise<void>;
  fetchGroupReasoning: (
    groupId: string,
    params: { candidateId?: string; itemIds: string[]; confidence: number },
    isUserExplicit?: boolean
  ) => Promise<void>;
  acceptItemSuggestion: (itemId: string, workspaceId: string) => Promise<void>;
  dismissItemSuggestion: (itemId: string, workspaceId?: string) => Promise<void>;
  markItemUnderstood: (itemId: string, reason?: string) => Promise<void>;
  acceptAllHighConfidenceSuggestions: (items: Item[]) => Promise<number>;
  
  bulkAssignToWorkspace: (workspaceId: string) => Promise<void>;
  bulkDismissSuggestions: (workspaceId?: string) => Promise<void>;
  bulkArchiveOrClear: () => Promise<number>;
  assignItemIdsToWorkspace: (itemIds: string[], workspaceId: string) => Promise<void>;
  dismissItemIdsSuggestions: (itemIds: string[], workspaceId?: string) => Promise<void>;
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
  
  // Search & Multi-Filter State
  filterWorkspaceId: null,
  filterCandidateId: null,
  filterItemType: null,
  filterDatePreset: 'all',
  filterDateFrom: null,
  filterDateTo: null,
  filterInboxState: null,
  isSemanticSearchActive: false,
  isFilterBarOpen: false,
  searchResults: null,
  isSearching: false,
  
  attentionMap: {},
  inboxStateMap: {},
  candidateMap: {},
  candidateItemMap: {},
  groupReasoningMap: {},
  isLoading: false,
  isBatchProcessing: false,
  selectedItemIds: [],
  isMultiSelectActive: false,

  setViewMode: (mode) => set({ viewMode: mode }),
  setSortOrder: (order) => {
    set({ sortOrder: order });
    get().executeSearch();
  },
  setSearchQuery: (query) => {
    set({ searchQuery: query });
    get().executeSearch();
  },
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

  // Filter Actions
  setFilterWorkspaceId: (workspaceId) => {
    set({ filterWorkspaceId: workspaceId });
    get().executeSearch();
  },
  setFilterCandidateId: (candidateId) => {
    set({ filterCandidateId: candidateId });
    get().executeSearch();
  },
  setFilterItemType: (itemType) => {
    set({ filterItemType: itemType });
    get().executeSearch();
  },
  setFilterDatePreset: (preset, from, to) => {
    let dateFrom: string | null = null;
    let dateTo: string | null = null;

    if (preset === 'today') {
      const d = new Date();
      d.setHours(0, 0, 0, 0);
      dateFrom = d.toISOString();
    } else if (preset === '7d') {
      const d = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000);
      dateFrom = d.toISOString();
    } else if (preset === '30d') {
      const d = new Date(Date.now() - 30 * 24 * 60 * 60 * 1000);
      dateFrom = d.toISOString();
    } else if (preset === 'custom') {
      dateFrom = from ?? null;
      dateTo = to ?? null;
    }

    set({
      filterDatePreset: preset,
      filterDateFrom: dateFrom,
      filterDateTo: dateTo,
    });
    get().executeSearch();
  },
  setFilterInboxState: (state) => {
    set({ filterInboxState: state });
    get().executeSearch();
  },
  setIsSemanticSearchActive: (active) => {
    set({ isSemanticSearchActive: active });
    get().executeSearch();
  },
  setIsFilterBarOpen: (open) => set({ isFilterBarOpen: open }),
  toggleFilterBar: () => set((s) => ({ isFilterBarOpen: !s.isFilterBarOpen })),
  resetAllFilters: () => {
    set({
      searchQuery: '',
      filterWorkspaceId: null,
      filterCandidateId: null,
      filterItemType: null,
      filterDatePreset: 'all',
      filterDateFrom: null,
      filterDateTo: null,
      filterInboxState: null,
      isSemanticSearchActive: false,
      searchResults: null,
      isSearching: false,
    });
  },
  getActiveFilterCount: () => {
    const s = get();
    let count = 0;
    if (s.filterWorkspaceId) count++;
    if (s.filterCandidateId) count++;
    if (s.filterItemType) count++;
    if (s.filterDatePreset !== 'all') count++;
    if (s.filterInboxState) count++;
    if (s.isSemanticSearchActive) count++;
    return count;
  },
  executeSearch: async () => {
    const s = get();
    const query = s.searchQuery.trim();
    const hasQuery = query.length > 0;
    const hasFilter =
      s.filterWorkspaceId !== null ||
      s.filterCandidateId !== null ||
      s.filterItemType !== null ||
      s.filterDatePreset !== 'all' ||
      s.filterInboxState !== null ||
      s.isSemanticSearchActive;

    if (!hasQuery && !hasFilter) {
      set({ searchResults: null, isSearching: false });
      return;
    }

    set({ isSearching: true });
    try {
      const params: InboxSearchParams = {
        query: hasQuery ? query : undefined,
        workspace_id: s.filterWorkspaceId ?? undefined,
        candidate_id: s.filterCandidateId ?? undefined,
        item_type: s.filterItemType ?? undefined,
        date_from: s.filterDateFrom ?? undefined,
        date_to: s.filterDateTo ?? undefined,
        inbox_state: s.filterInboxState ?? undefined,
        enable_semantic: s.isSemanticSearchActive,
        min_similarity: s.isSemanticSearchActive ? 0.20 : undefined,
        sort_by: s.sortOrder === 'oldest' ? 'oldest' : 'newest',
      };

      const results = await db.searchInboxItems(params);
      set({ searchResults: results, isSearching: false });
    } catch (err) {
      console.error('Failed to execute inbox search:', err);
      set({ isSearching: false });
    }
  },

  toggleSelectItem: (itemId: string) =>
    set((s) => {
      const exists = s.selectedItemIds.includes(itemId);
      const next = exists
        ? s.selectedItemIds.filter((id) => id !== itemId)
        : [...s.selectedItemIds, itemId];
      return {
        selectedItemIds: next,
        isMultiSelectActive: next.length > 0 ? true : s.isMultiSelectActive,
      };
    }),

  selectItems: (itemIds: string[]) =>
    set((s) => {
      const setIds = new Set([...s.selectedItemIds, ...itemIds]);
      return {
        selectedItemIds: Array.from(setIds),
        isMultiSelectActive: true,
      };
    }),

  deselectItems: (itemIds: string[]) =>
    set((s) => {
      const setIds = new Set(itemIds);
      return {
        selectedItemIds: s.selectedItemIds.filter((id) => !setIds.has(id)),
      };
    }),

  selectAllItems: (itemIds: string[]) =>
    set({
      selectedItemIds: [...itemIds],
      isMultiSelectActive: itemIds.length > 0,
    }),

  clearSelection: () =>
    set({
      selectedItemIds: [],
    }),

  setMultiSelectActive: (active: boolean) =>
    set((s) => ({
      isMultiSelectActive: active,
      selectedItemIds: active ? s.selectedItemIds : [],
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
      const newCandidateMap: Record<string, ContextCandidate> = {};
      const newCandidateItemMap: Record<
        string,
        { candidateId: string; candidateName: string; confidence: number; explanation?: string }
      > = {};

      for (const c of activeCandidates) {
        newCandidateMap[c.id] = c;
      }

      const candidateDetailsPromises = activeCandidates.map(async (c) => {
        try {
          let explanation: string | undefined;
          try {
            const meta = JSON.parse(c.metadata || '{}');
            if (meta.evidence && meta.evidence.length > 0) {
              explanation = meta.evidence[0];
            } else if (meta.strongest_relationships && meta.strongest_relationships.length > 0) {
              explanation = meta.strongest_relationships[0].description || undefined;
            }
          } catch (e) {}

          const details: CandidateWithItems = await db.getCandidateItems(c.id);
          for (const member of details.items) {
            newCandidateItemMap[member.item.id] = {
              candidateId: c.id,
              candidateName: c.label || 'Discovered Context',
              confidence: c.confidence,
              explanation,
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
        candidateMap: newCandidateMap,
        candidateItemMap: newCandidateItemMap,
        isLoading: false,
      });
    } catch (err) {
      console.error('Failed to analyze inbox items:', err);
      set({ isLoading: false });
    }
  },

  fetchGroupReasoning: async (
    groupId: string,
    params: { candidateId?: string; itemIds: string[]; confidence: number },
    isUserExplicit: boolean = false
  ) => {
    const existing = get().groupReasoningMap[groupId];
    // If cached result exists and call is not user-explicit, return cached result without IPC/AI invocation
    if (existing?.hasRun && !isUserExplicit) {
      return;
    }
    if (existing?.isLoading) {
      return;
    }

    set((s) => ({
      groupReasoningMap: {
        ...s.groupReasoningMap,
        [groupId]: {
          result: existing?.result || null,
          isLoading: true,
          isUnavailable: false,
          unavailabilityReason: null,
          hasRun: true,
        },
      },
    }));

    try {
      let response: AIContextReasoningResponse;
      if (params.candidateId) {
        response = await db.analyzeCandidateContextAI(params.candidateId, isUserExplicit);
      } else if (params.itemIds.length > 0) {
        response = await db.analyzeItemGroupContextAI(params.itemIds, isUserExplicit);
      } else {
        set((s) => ({
          groupReasoningMap: {
            ...s.groupReasoningMap,
            [groupId]: {
              result: null,
              isLoading: false,
              isUnavailable: true,
              unavailabilityReason: 'No items in group to analyze',
              hasRun: true,
            },
          },
        }));
        return;
      }

      if (response.ai_result) {
        set((s) => ({
          groupReasoningMap: {
            ...s.groupReasoningMap,
            [groupId]: {
              result: response.ai_result,
              isLoading: false,
              isUnavailable: false,
              unavailabilityReason: null,
              hasRun: true,
            },
          },
        }));
      } else {
        set((s) => ({
          groupReasoningMap: {
            ...s.groupReasoningMap,
            [groupId]: {
              result: null,
              isLoading: false,
              isUnavailable: true,
              unavailabilityReason: response.unavailability_reason || 'AI Context Reasoning unavailable',
              hasRun: true,
            },
          },
        }));
      }
    } catch (err: any) {
      console.warn(`AI reasoning failed for group ${groupId}:`, err);
      set((s) => ({
        groupReasoningMap: {
          ...s.groupReasoningMap,
          [groupId]: {
            result: null,
            isLoading: false,
            isUnavailable: true,
            unavailabilityReason: err?.message || 'AI Reasoning request failed',
            hasRun: true,
          },
        },
      }));
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

  assignItemIdsToWorkspace: async (itemIds: string[], workspaceId: string) => {
    if (itemIds.length === 0) return;
    set({ isBatchProcessing: true });
    try {
      await db.bulkAssignInboxItemsToWorkspace(itemIds, workspaceId);

      const currentActive = useWorkspaceStore.getState().activeItems;
      const filtered = currentActive.filter((item) => !itemIds.includes(item.id));
      const removedCount = currentActive.length - filtered.length;

      useWorkspaceStore.setState((s) => ({
        activeItems: filtered,
        inboxCount: Math.max(0, s.inboxCount - removedCount),
      }));

      set((s) => ({
        selectedItemIds: s.selectedItemIds.filter((id) => !itemIds.includes(id)),
      }));
    } catch (err) {
      console.error('Failed to assign items to workspace:', err);
      throw err;
    } finally {
      set({ isBatchProcessing: false });
    }
  },

  dismissItemIdsSuggestions: async (itemIds: string[], workspaceId?: string) => {
    if (itemIds.length === 0) return;
    set({ isBatchProcessing: true });
    try {
      await db.bulkDismissInboxSuggestions(itemIds, workspaceId);

      const stateMap = { ...get().inboxStateMap };
      const now = new Date().toISOString();
      for (const id of itemIds) {
        stateMap[id] = {
          item_id: id,
          state: 'understood',
          previous_state: stateMap[id]?.state,
          reason: 'Dismissed suggestions',
          created_at: stateMap[id]?.created_at || now,
          updated_at: now,
        };
      }
      set({
        inboxStateMap: stateMap,
        selectedItemIds: get().selectedItemIds.filter((id) => !itemIds.includes(id)),
      });
    } catch (err) {
      console.error('Failed to dismiss suggestions:', err);
      throw err;
    } finally {
      set({ isBatchProcessing: false });
    }
  },

  bulkAssignToWorkspace: async (workspaceId: string) => {
    const { selectedItemIds, assignItemIdsToWorkspace } = get();
    if (selectedItemIds.length === 0) return;
    await assignItemIdsToWorkspace(selectedItemIds, workspaceId);
  },

  bulkDismissSuggestions: async (workspaceId?: string) => {
    const { selectedItemIds, dismissItemIdsSuggestions } = get();
    if (selectedItemIds.length === 0) return;
    await dismissItemIdsSuggestions(selectedItemIds, workspaceId);
  },

  bulkArchiveOrClear: async () => {
    const { selectedItemIds } = get();
    if (selectedItemIds.length === 0) return 0;

    set({ isBatchProcessing: true });
    try {
      const deletedCount = await db.bulkArchiveOrClearInboxItems(selectedItemIds);

      // Remove deleted items from activeItems in workspaceStore
      const currentActive = useWorkspaceStore.getState().activeItems;
      const filtered = currentActive.filter((item) => !selectedItemIds.includes(item.id));

      useWorkspaceStore.setState((s) => ({
        activeItems: filtered,
        inboxCount: Math.max(0, s.inboxCount - deletedCount),
      }));

      set({ selectedItemIds: [] });
      return deletedCount;
    } catch (err) {
      console.error('Failed to bulk clear items:', err);
      throw err;
    } finally {
      set({ isBatchProcessing: false });
    }
  },
}));
