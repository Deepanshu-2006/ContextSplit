import { useEffect, useMemo } from 'react';
import {
  Inbox,
  LayoutList,
  Layers,
  Clock,
  LayoutTemplate,
  Trash2,
  ArrowDownUp,
  Filter,
  Search,
  RefreshCw,
  X,
  CheckSquare,
} from 'lucide-react';
import { confirm } from '@tauri-apps/plugin-dialog';
import { Item, Workspace } from '../../services/db';
import {
  useInboxStore,
  deriveItemSection,
  DerivedInboxItem,
  InboxSectionKey,
} from '../../store/inboxStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { InboxSection } from './InboxSection';
import { InboxItemCard } from './InboxItemCard';
import { InboxItemGroupCard, InboxItemGroup } from './InboxItemGroupCard';
import { BulkActionsBar } from './BulkActionsBar';

interface InboxViewProps {
  items: Item[];
  workspaces: Workspace[];
  onMoveToWorkspace: (itemId: string, workspaceId: string) => void;
  onSelectCanvasView: () => void;
}

export function InboxView({
  items,
  workspaces,
  onMoveToWorkspace,
  onSelectCanvasView,
}: InboxViewProps) {
  const {
    viewMode,
    sortOrder,
    searchQuery,
    selectedSectionFilter,
    attentionMap,
    inboxStateMap,
    candidateItemMap,
    isLoading,
    isBatchProcessing,
    selectedItemIds,
    isMultiSelectActive,
    setViewMode,
    setSortOrder,
    setSearchQuery,
    setSelectedSectionFilter,
    fetchInboxAnalysis,
    selectAllItems,
    clearSelection,
    setMultiSelectActive,
    bulkAssignToWorkspace,
    bulkDismissSuggestions,
    bulkArchiveOrClear,
    assignItemIdsToWorkspace,
    dismissItemIdsSuggestions,
  } = useInboxStore();

  const { emptyInbox } = useWorkspaceStore();

  // Load fresh attention & candidate analysis whenever inbox items change
  useEffect(() => {
    fetchInboxAnalysis(items);
  }, [items, fetchInboxAnalysis]);

  // Derive logical sections without any item duplication
  const derivedItems: DerivedInboxItem[] = useMemo(() => {
    return items.map((item) => {
      const att = attentionMap[item.id];
      const state = inboxStateMap[item.id];
      const cand = candidateItemMap[item.id];
      const section = deriveItemSection(item, att, state, cand);

      return {
        item,
        section,
        attention: att,
        inboxState: state,
        candidateInfo: cand,
      };
    });
  }, [items, attentionMap, inboxStateMap, candidateItemMap]);

  // Apply search query filter
  const filteredDerivedItems = useMemo(() => {
    let list = derivedItems;
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      list = list.filter(
        (d) =>
          (d.item.title && d.item.title.toLowerCase().includes(q)) ||
          d.item.content.toLowerCase().includes(q) ||
          (d.candidateInfo && d.candidateInfo.candidateName.toLowerCase().includes(q)) ||
          (d.attention?.reason && d.attention.reason.toLowerCase().includes(q))
      );
    }
    return list;
  }, [derivedItems, searchQuery]);

  // Chronological sort
  const sortedDerivedItems = useMemo(() => {
    const list = [...filteredDerivedItems];
    list.sort((a, b) => {
      const tA = new Date(a.item.created_at).getTime();
      const tB = new Date(b.item.created_at).getTime();
      return sortOrder === 'newest' ? tB - tA : tA - tB;
    });
    return list;
  }, [filteredDerivedItems, sortOrder]);

  // Partition strictly into the 5 logical sections
  const sectionPartitions = useMemo(() => {
    const partitions: Record<InboxSectionKey, DerivedInboxItem[]> = {
      needs_attention: [],
      context_suggestions: [],
      new_candidates: [],
      recently_captured: [],
      already_understood: [],
    };

    for (const d of sortedDerivedItems) {
      partitions[d.section].push(d);
    }

    return partitions;
  }, [sortedDerivedItems]);

  // Derive all contextual clusters for the 'grouped' view mode without duplicating any item records
  const { allGroups, ungroupedItems } = useMemo(() => {
    const candidateMap: Record<string, DerivedInboxItem[]> = {};
    const workspaceMap: Record<string, DerivedInboxItem[]> = {};
    const ungrouped: DerivedInboxItem[] = [];

    for (const d of sortedDerivedItems) {
      if (d.candidateInfo?.candidateId) {
        const cId = d.candidateInfo.candidateId;
        if (!candidateMap[cId]) candidateMap[cId] = [];
        candidateMap[cId].push(d);
      } else if (d.attention?.factors?.suggested_workspace_id) {
        const wsId = d.attention.factors.suggested_workspace_id;
        if (!workspaceMap[wsId]) workspaceMap[wsId] = [];
        workspaceMap[wsId].push(d);
      } else {
        ungrouped.push(d);
      }
    }

    const groups: InboxItemGroup[] = [];

    // 1. Candidate Groups
    for (const [cId, memberItems] of Object.entries(candidateMap)) {
      const first = memberItems[0];
      groups.push({
        id: `cand-${cId}`,
        type: 'candidate',
        name: first.candidateInfo?.candidateName || 'Discovered Context Candidate',
        itemCount: memberItems.length,
        confidence: first.candidateInfo?.confidence ?? 0.85,
        strongestExplanation:
          first.candidateInfo?.explanation ||
          'Discovered cluster of items with strong contextual similarity and co-occurrence.',
        items: memberItems,
        candidateId: cId,
      });
    }

    // 2. Workspace Suggestion Groups (>= 2 items)
    for (const [wsId, memberItems] of Object.entries(workspaceMap)) {
      if (memberItems.length >= 2) {
        const ws = workspaces.find((w) => w.id === wsId);
        const maxConf = Math.max(
          ...memberItems.map((m) => m.attention?.factors?.contextual_confidence ?? 0.8)
        );
        const bestExplanation =
          memberItems.find((m) => m.attention?.reason)?.attention?.reason ||
          `Multiple items suggested for workspace "${ws?.name || 'Workspace'}".`;

        groups.push({
          id: `ws-${wsId}`,
          type: 'workspace_suggestion',
          name: ws?.name || 'Suggested Workspace',
          itemCount: memberItems.length,
          confidence: maxConf,
          strongestExplanation: bestExplanation,
          items: memberItems,
          suggestedWorkspaceId: wsId,
        });
      } else {
        ungrouped.push(...memberItems);
      }
    }

    return { allGroups: groups, ungroupedItems: ungrouped };
  }, [sortedDerivedItems, workspaces]);

  const handleEmptyInbox = async () => {
    const isConfirmed = await confirm(
      'Are you sure you want to empty the inbox? All items in the inbox will be permanently deleted.',
      { title: 'Empty Inbox', kind: 'warning' }
    );
    if (isConfirmed) {
      await emptyInbox();
    }
  };

  const handleSelectAll = () => {
    selectAllItems(filteredDerivedItems.map((d) => d.item.id));
  };

  const handleBulkAssign = async (workspaceId: string) => {
    await bulkAssignToWorkspace(workspaceId);
  };

  const handleBulkDismiss = async () => {
    await bulkDismissSuggestions();
  };

  const handleBulkClear = async () => {
    const isConfirmed = await confirm(
      `Are you sure you want to clear ${selectedItemIds.length} item(s) from the Inbox?`,
      { title: 'Clear Selected Items', kind: 'warning' }
    );
    if (isConfirmed) {
      await bulkArchiveOrClear();
    }
  };

  const totalNeedsAttention = sectionPartitions.needs_attention.length;
  const totalSuggestions = sectionPartitions.context_suggestions.length;
  const totalCandidates = sectionPartitions.new_candidates.length;

  return (
    <div className="flex-1 w-full h-full bg-[#FBFBFB] flex flex-col overflow-hidden">
      {/* Top Inbox Header & Control Toolbar */}
      <div className="px-8 py-5 border-b border-black/4 bg-white/70 backdrop-blur-md flex flex-col gap-4 shrink-0">
        <div className="flex items-center justify-between flex-wrap gap-4">
          {/* Left Title & Status Pills */}
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-xl bg-gray-900 text-white flex items-center justify-center shadow-xs">
              <Inbox size={18} strokeWidth={2} />
            </div>
            <div>
              <div className="flex items-center gap-2.5">
                <h1 className="text-xl font-semibold tracking-tight text-gray-900">Inbox</h1>
                <span className="text-xs font-semibold px-2 py-0.5 rounded-full bg-gray-100 text-gray-700">
                  {items.length} {items.length === 1 ? 'item' : 'items'}
                </span>
                {totalNeedsAttention > 0 && (
                  <span className="text-xs font-semibold px-2 py-0.5 rounded-full bg-amber-100 text-amber-800 border border-amber-200 animate-pulse">
                    {totalNeedsAttention} need attention
                  </span>
                )}
                {totalSuggestions > 0 && (
                  <span className="text-xs font-semibold px-2 py-0.5 rounded-full bg-indigo-50 text-indigo-700 border border-indigo-200 hidden md:inline">
                    {totalSuggestions} suggested
                  </span>
                )}
                {totalCandidates > 0 && (
                  <span className="text-xs font-semibold px-2 py-0.5 rounded-full bg-cyan-50 text-cyan-700 border border-cyan-200 hidden md:inline">
                    {totalCandidates} candidates
                  </span>
                )}
              </div>
              <p className="text-[12px] text-gray-400 mt-0.5">
                Context-aware capture queue. Organize items into workspaces or let context evolve.
              </p>
            </div>
          </div>

          {/* Right Toolbar Controls */}
          <div className="flex items-center gap-2.5 flex-wrap">
            {/* Search Input Filter */}
            <div className="relative group w-48 sm:w-56">
              <div className="absolute inset-y-0 left-0 pl-2.5 flex items-center pointer-events-none text-gray-400 group-hover:text-gray-600">
                <Search size={13} />
              </div>
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Filter inbox items..."
                className="w-full bg-gray-100/80 hover:bg-gray-100 border border-black/5 rounded-lg py-1.5 pl-8 pr-7 text-[12px] placeholder-gray-400 focus:outline-none focus:bg-white focus:ring-1 focus:ring-black/10 transition-all shadow-2xs"
              />
              {searchQuery && (
                <button
                  onClick={() => setSearchQuery('')}
                  className="absolute inset-y-0 right-0 pr-2 flex items-center text-gray-400 hover:text-gray-600 cursor-pointer"
                >
                  <X size={12} />
                </button>
              )}
            </div>

            {/* View Mode Selector */}
            <div className="flex items-center p-0.5 bg-gray-100/90 rounded-lg border border-black/5 text-[12px] font-medium text-gray-600">
              <button
                onClick={() => setViewMode('sections')}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md transition-all cursor-pointer ${
                  viewMode === 'sections'
                    ? 'bg-white text-gray-900 shadow-2xs font-semibold'
                    : 'hover:text-gray-900'
                }`}
                title="Categorized Section View"
              >
                <LayoutList size={14} />
                <span>Sections</span>
              </button>

              <button
                onClick={() => setViewMode('grouped')}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md transition-all cursor-pointer ${
                  viewMode === 'grouped'
                    ? 'bg-white text-gray-900 shadow-2xs font-semibold'
                    : 'hover:text-gray-900'
                }`}
                title="Contextual Groups View"
              >
                <Layers size={14} />
                <span>Groups</span>
              </button>

              <button
                onClick={() => setViewMode('chronological')}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md transition-all cursor-pointer ${
                  viewMode === 'chronological'
                    ? 'bg-white text-gray-900 shadow-2xs font-semibold'
                    : 'hover:text-gray-900'
                }`}
                title="Chronological Stream"
              >
                <Clock size={14} />
                <span>Timeline</span>
              </button>

              <button
                onClick={() => {
                  setViewMode('canvas');
                  onSelectCanvasView();
                }}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md transition-all cursor-pointer ${
                  viewMode === 'canvas'
                    ? 'bg-white text-gray-900 shadow-2xs font-semibold'
                    : 'hover:text-gray-900'
                }`}
                title="Freeform Board View"
              >
                <LayoutTemplate size={14} />
                <span>Canvas</span>
              </button>
            </div>

            {/* Multi-Select Toggle */}
            {items.length > 0 && (
              <button
                onClick={() => setMultiSelectActive(!isMultiSelectActive)}
                className={`flex items-center gap-1.5 px-2.5 py-1.5 border rounded-lg text-[12px] font-medium shadow-2xs transition-colors cursor-pointer ${
                  isMultiSelectActive || selectedItemIds.length > 0
                    ? 'bg-indigo-50 border-indigo-200 text-indigo-700'
                    : 'bg-white hover:bg-gray-50 text-gray-700 border-black/6'
                }`}
                title="Toggle Multi-Select Mode"
              >
                <CheckSquare size={13} className={isMultiSelectActive ? 'text-indigo-600' : 'text-gray-400'} />
                <span>Select</span>
                {selectedItemIds.length > 0 && (
                  <span className="ml-0.5 bg-indigo-600 text-white text-[10px] font-bold px-1.5 py-0.2 rounded-full">
                    {selectedItemIds.length}
                  </span>
                )}
              </button>
            )}

            {/* Sort Order Toggle */}
            <button
              onClick={() => setSortOrder(sortOrder === 'newest' ? 'oldest' : 'newest')}
              className="flex items-center gap-1.5 px-2.5 py-1.5 bg-white hover:bg-gray-50 text-gray-700 border border-black/6 rounded-lg text-[12px] font-medium shadow-2xs transition-colors cursor-pointer"
              title={`Sorting: ${sortOrder === 'newest' ? 'Newest First' : 'Oldest First'}`}
            >
              <ArrowDownUp size={13} className="text-gray-400" />
              <span>{sortOrder === 'newest' ? 'Newest' : 'Oldest'}</span>
            </button>

            {/* Refresh Analysis */}
            <button
              onClick={() => fetchInboxAnalysis(items)}
              disabled={isLoading}
              className="p-1.5 bg-white hover:bg-gray-50 text-gray-500 hover:text-gray-800 border border-black/6 rounded-lg shadow-2xs transition-colors cursor-pointer"
              title="Refresh attention analysis"
            >
              <RefreshCw size={14} className={isLoading ? 'animate-spin text-indigo-600' : ''} />
            </button>

            {/* Empty Inbox Button */}
            {items.length > 0 && (
              <button
                onClick={handleEmptyInbox}
                className="flex items-center gap-1.5 px-2.5 py-1.5 bg-white hover:bg-rose-50 text-gray-600 hover:text-rose-600 border border-black/6 hover:border-rose-200 rounded-lg text-[12px] font-medium shadow-2xs transition-colors cursor-pointer"
                title="Delete all items in inbox"
              >
                <Trash2 size={13} className="text-gray-400 hover:text-rose-500" />
                <span className="hidden sm:inline">Empty</span>
              </button>
            )}
          </div>
        </div>

        {/* Filter Badges Bar in Sections View */}
        <div className="flex items-center gap-2 overflow-x-auto pt-0.5 pb-1 no-scrollbar text-[12px]">
          <span className="text-gray-400 text-[11px] font-medium flex items-center gap-1 shrink-0">
            <Filter size={11} /> Filter:
          </span>

          <button
            onClick={() => setSelectedSectionFilter('all')}
            className={`px-2.5 py-0.5 rounded-full font-medium transition-colors cursor-pointer shrink-0 ${
              selectedSectionFilter === 'all'
                ? 'bg-gray-900 text-white'
                : 'bg-gray-100 text-gray-600 hover:bg-gray-200/80'
            }`}
          >
            All ({items.length})
          </button>

          <button
            onClick={() => setSelectedSectionFilter('needs_attention')}
            className={`px-2.5 py-0.5 rounded-full font-medium transition-colors cursor-pointer shrink-0 ${
              selectedSectionFilter === 'needs_attention'
                ? 'bg-amber-600 text-white'
                : 'bg-amber-50 text-amber-800 hover:bg-amber-100'
            }`}
          >
            Needs Attention ({sectionPartitions.needs_attention.length})
          </button>

          <button
            onClick={() => setSelectedSectionFilter('context_suggestions')}
            className={`px-2.5 py-0.5 rounded-full font-medium transition-colors cursor-pointer shrink-0 ${
              selectedSectionFilter === 'context_suggestions'
                ? 'bg-indigo-600 text-white'
                : 'bg-indigo-50 text-indigo-800 hover:bg-indigo-100'
            }`}
          >
            Suggestions ({sectionPartitions.context_suggestions.length})
          </button>

          <button
            onClick={() => setSelectedSectionFilter('new_candidates')}
            className={`px-2.5 py-0.5 rounded-full font-medium transition-colors cursor-pointer shrink-0 ${
              selectedSectionFilter === 'new_candidates'
                ? 'bg-cyan-600 text-white'
                : 'bg-cyan-50 text-cyan-800 hover:bg-cyan-100'
            }`}
          >
            Candidates ({sectionPartitions.new_candidates.length})
          </button>

          <button
            onClick={() => setSelectedSectionFilter('recently_captured')}
            className={`px-2.5 py-0.5 rounded-full font-medium transition-colors cursor-pointer shrink-0 ${
              selectedSectionFilter === 'recently_captured'
                ? 'bg-sky-600 text-white'
                : 'bg-sky-50 text-sky-800 hover:bg-sky-100'
            }`}
          >
            Recent ({sectionPartitions.recently_captured.length})
          </button>

          <button
            onClick={() => setSelectedSectionFilter('already_understood')}
            className={`px-2.5 py-0.5 rounded-full font-medium transition-colors cursor-pointer shrink-0 ${
              selectedSectionFilter === 'already_understood'
                ? 'bg-slate-700 text-white'
                : 'bg-slate-100 text-slate-700 hover:bg-slate-200'
            }`}
          >
            Understood ({sectionPartitions.already_understood.length})
          </button>
        </div>
      </div>

      {/* Main Scrollable Content Area */}
      <div className="flex-1 overflow-y-auto px-8 py-6">
        {items.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center p-8 text-center opacity-60">
            <div className="w-16 h-16 rounded-2xl bg-gray-100 flex items-center justify-center mb-4">
              <Inbox size={28} className="text-gray-400" />
            </div>
            <h2 className="text-base font-semibold text-gray-800 mb-1">Inbox is Empty</h2>
            <p className="text-[13px] text-gray-500 max-w-sm">
              Copy text, images, URLs, or files to capture items here.
            </p>
          </div>
        ) : viewMode === 'sections' ? (
          /* 1. SECTIONS VIEW MODE */
          <div className="flex flex-col gap-6 max-w-7xl mx-auto">
            {(selectedSectionFilter === 'all' || selectedSectionFilter === 'needs_attention') && (
              <InboxSection
                sectionKey="needs_attention"
                items={sectionPartitions.needs_attention}
                workspaces={workspaces}
                onMoveToWorkspace={onMoveToWorkspace}
              />
            )}

            {(selectedSectionFilter === 'all' || selectedSectionFilter === 'context_suggestions') && (
              <InboxSection
                sectionKey="context_suggestions"
                items={sectionPartitions.context_suggestions}
                workspaces={workspaces}
                onMoveToWorkspace={onMoveToWorkspace}
              />
            )}

            {(selectedSectionFilter === 'all' || selectedSectionFilter === 'new_candidates') && (
              <InboxSection
                sectionKey="new_candidates"
                items={sectionPartitions.new_candidates}
                workspaces={workspaces}
                onMoveToWorkspace={onMoveToWorkspace}
              />
            )}

            {(selectedSectionFilter === 'all' || selectedSectionFilter === 'recently_captured') && (
              <InboxSection
                sectionKey="recently_captured"
                items={sectionPartitions.recently_captured}
                workspaces={workspaces}
                onMoveToWorkspace={onMoveToWorkspace}
              />
            )}

            {(selectedSectionFilter === 'all' || selectedSectionFilter === 'already_understood') && (
              <InboxSection
                sectionKey="already_understood"
                items={sectionPartitions.already_understood}
                workspaces={workspaces}
                onMoveToWorkspace={onMoveToWorkspace}
              />
            )}
          </div>
        ) : viewMode === 'grouped' ? (
          /* 2. CONTEXTUAL GROUPS VIEW MODE */
          <div className="flex flex-col gap-6 max-w-7xl mx-auto">
            <div className="flex items-center justify-between px-1">
              <div>
                <h2 className="text-sm font-semibold text-gray-800">
                  Context Groups & Clusters ({allGroups.length} {allGroups.length === 1 ? 'cluster' : 'clusters'})
                </h2>
                <p className="text-[11px] text-gray-400">
                  Items grouped by context candidate discoveries and workspace recommendations.
                </p>
              </div>
            </div>

            {/* Context Group Cards */}
            {allGroups.length > 0 ? (
              <div className="flex flex-col gap-4">
                {allGroups.map((group) => (
                  <InboxItemGroupCard
                    key={group.id}
                    group={group}
                    workspaces={workspaces}
                    onMoveToWorkspace={onMoveToWorkspace}
                    onBulkAssignGroup={assignItemIdsToWorkspace}
                    onBulkDismissGroup={dismissItemIdsSuggestions}
                  />
                ))}
              </div>
            ) : (
              <div className="px-6 py-8 border border-dashed border-black/5 rounded-xl text-center bg-gray-50/30">
                <p className="text-[12px] text-gray-400">No multi-item contextual clusters currently detected.</p>
              </div>
            )}

            {/* Ungrouped / Individual Items */}
            {ungroupedItems.length > 0 && (
              <div className="flex flex-col gap-3 pt-2">
                <div className="flex items-center gap-2 px-1">
                  <h3 className="text-xs font-semibold text-gray-600 uppercase tracking-wider">
                    Individual Items ({ungroupedItems.length})
                  </h3>
                  <div className="h-px flex-1 bg-black/5" />
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3.5">
                  {ungroupedItems.map((derived) => (
                    <InboxItemCard
                      key={derived.item.id}
                      derived={derived}
                      workspaces={workspaces}
                      onMoveToWorkspace={onMoveToWorkspace}
                      showSectionBadge={true}
                    />
                  ))}
                </div>
              </div>
            )}
          </div>
        ) : (
          /* 3. CHRONOLOGICAL TIMELINE STREAM VIEW */
          <div className="max-w-7xl mx-auto flex flex-col gap-4">
            <div className="flex items-center justify-between px-1">
              <p className="text-[12px] font-semibold text-gray-500 uppercase tracking-wider">
                All Inbox Items ({sortedDerivedItems.length})
              </p>
              <span className="text-[11px] text-gray-400">
                Sorted by capture timestamp ({sortOrder === 'newest' ? 'Newest' : 'Oldest'})
              </span>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3.5">
              {sortedDerivedItems.map((derived) => (
                <InboxItemCard
                  key={derived.item.id}
                  derived={derived}
                  workspaces={workspaces}
                  onMoveToWorkspace={onMoveToWorkspace}
                  showSectionBadge={true}
                />
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Floating Multi-Select Bulk Actions Bar */}
      <BulkActionsBar
        selectedCount={selectedItemIds.length}
        totalCount={filteredDerivedItems.length}
        workspaces={workspaces}
        isProcessing={isBatchProcessing}
        onSelectAll={handleSelectAll}
        onClearSelection={clearSelection}
        onBulkAssign={handleBulkAssign}
        onBulkDismiss={handleBulkDismiss}
        onBulkClear={handleBulkClear}
      />
    </div>
  );
}
