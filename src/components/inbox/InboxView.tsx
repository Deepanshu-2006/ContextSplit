import { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import {
  Search,
  Plus,
  ArrowDownUp,
  X,
  Inbox,
  Sparkles,
} from 'lucide-react';
import { Item, Workspace } from '../../services/db';
import {
  useInboxStore,
  deriveItemSection,
  DerivedInboxItem,
  getModifierKeyName,
} from '../../store/inboxStore';
import { useDiscoveryStore } from '../../store/discoveryStore';
import { InboxItemCard } from './InboxItemCard';
import { InboxDetailDrawer } from './InboxDetailDrawer';
import { InboxFilterPopover } from './InboxFilterPopover';
import { QuickCaptureModal } from './QuickCaptureModal';
import { BulkActionsBar } from './BulkActionsBar';

interface InboxViewProps {
  items: Item[];
  workspaces: Workspace[];
  onMoveToWorkspace: (itemId: string, workspaceId: string) => void;
}

interface TimeGroup {
  label: string;
  items: DerivedInboxItem[];
}

export function InboxView({
  items,
  workspaces,
  onMoveToWorkspace,
}: InboxViewProps) {
  const {
    sortOrder,
    searchQuery,
    attentionMap,
    inboxStateMap,
    candidateItemMap,
    selectedItemIds,
    filterItemType,
    filterInboxState,
    isBatchProcessing,
    focusedItemId,
    setSortOrder,
    setSearchQuery,
    fetchInboxAnalysis,
    selectAllItems,
    clearSelection,
    toggleSelectItem,
    setFocusedItemId,
    acceptItemSuggestion,
    dismissItemIdsSuggestions,
    bulkAssignToWorkspace,
    bulkDismissSuggestions,
    bulkArchiveOrClear,
    resetAllFilters,
  } = useInboxStore();

  const { runDiscoveryScan, isLoading: isDiscoverLoading, candidates } = useDiscoveryStore();

  const [selectedDetailItemId, setSelectedDetailItemId] = useState<string | null>(null);
  const [isCaptureModalOpen, setIsCaptureModalOpen] = useState(false);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const modKey = getModifierKeyName();

  const handleSelectItem = useCallback((id: string) => {
    setSelectedDetailItemId(id);
  }, []);

  // Load attention and candidate analysis when items change
  useEffect(() => {
    fetchInboxAnalysis(items);
  }, [items, fetchInboxAnalysis]);

  // Derive logical items without duplication
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

  // Filter items based on active search and filter popover
  const filteredDerivedItems = useMemo(() => {
    let result = derivedItems;

    // Filter by type
    if (filterItemType) {
      result = result.filter((d) => d.item.item_type === filterItemType);
    }

    // Filter by state
    if (filterInboxState) {
      if (filterInboxState === 'needs_review') {
        result = result.filter(
          (d) =>
            d.section === 'needs_attention' ||
            d.attention?.factors?.is_ambiguous ||
            d.inboxState?.state === 'needs_review'
        );
      } else if (filterInboxState === 'suggested') {
        result = result.filter(
          (d) =>
            d.section === 'context_suggestions' ||
            d.inboxState?.state === 'suggested' ||
            d.attention?.factors?.suggested_workspace_id
        );
      } else if (filterInboxState === 'understood') {
        result = result.filter(
          (d) => d.section === 'already_understood' || d.inboxState?.state === 'understood'
        );
      }
    }

    // Filter by search query
    if (searchQuery.trim().length > 0) {
      const q = searchQuery.toLowerCase().trim();
      result = result.filter((d) => {
        const titleMatch = d.item.title?.toLowerCase().includes(q) ?? false;
        const contentMatch = d.item.content.toLowerCase().includes(q);
        const wsMatch = d.attention?.factors?.suggested_workspace_name?.toLowerCase().includes(q) ?? false;
        const candMatch = d.candidateInfo?.candidateName?.toLowerCase().includes(q) ?? false;
        return titleMatch || contentMatch || wsMatch || candMatch;
      });
    }

    return result;
  }, [derivedItems, filterItemType, filterInboxState, searchQuery]);

  // Sort items
  const sortedDerivedItems = useMemo(() => {
    const list = [...filteredDerivedItems];
    list.sort((a, b) => {
      const tA = new Date(a.item.created_at).getTime();
      const tB = new Date(b.item.created_at).getTime();
      return sortOrder === 'newest' ? tB - tA : tA - tB;
    });
    return list;
  }, [filteredDerivedItems, sortOrder]);

  // Chronological time grouping (TODAY, YESTERDAY, PREVIOUS 7 DAYS, OLDER)
  const timeGroups: TimeGroup[] = useMemo(() => {
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const yesterday = today - 24 * 60 * 60 * 1000;
    const sevenDaysAgo = today - 7 * 24 * 60 * 60 * 1000;

    const todayItems: DerivedInboxItem[] = [];
    const yesterdayItems: DerivedInboxItem[] = [];
    const previousWeekItems: DerivedInboxItem[] = [];
    const olderItems: DerivedInboxItem[] = [];

    for (const d of sortedDerivedItems) {
      const itemTime = new Date(d.item.created_at).getTime();
      if (itemTime >= today) {
        todayItems.push(d);
      } else if (itemTime >= yesterday) {
        yesterdayItems.push(d);
      } else if (itemTime >= sevenDaysAgo) {
        previousWeekItems.push(d);
      } else {
        olderItems.push(d);
      }
    }

    const groups: TimeGroup[] = [];
    if (todayItems.length > 0) groups.push({ label: 'TODAY', items: todayItems });
    if (yesterdayItems.length > 0) groups.push({ label: 'YESTERDAY', items: yesterdayItems });
    if (previousWeekItems.length > 0) groups.push({ label: 'PREVIOUS 7 DAYS', items: previousWeekItems });
    if (olderItems.length > 0) groups.push({ label: 'OLDER', items: olderItems });

    return groups;
  }, [sortedDerivedItems]);

  // Find currently selected item for detail drawer
  const selectedDerivedItem = useMemo(() => {
    if (!selectedDetailItemId) return null;
    return derivedItems.find((d) => d.item.id === selectedDetailItemId) || null;
  }, [selectedDetailItemId, derivedItems]);

  // Global keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const activeEl = document.activeElement;
      const isInputFocused =
        activeEl instanceof HTMLInputElement ||
        activeEl instanceof HTMLTextAreaElement ||
        (activeEl as HTMLElement)?.isContentEditable;

      // Search shortcut (⌘K or /)
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
        return;
      }

      if (isInputFocused) {
        if (e.key === 'Escape') {
          e.preventDefault();
          if (document.activeElement === searchInputRef.current) {
            if (searchQuery) {
              setSearchQuery('');
            } else {
              searchInputRef.current?.blur();
            }
          } else {
            (activeEl as HTMLElement)?.blur();
          }
        }
        return;
      }

      // Quick '/' search
      if (e.key === '/') {
        e.preventDefault();
        searchInputRef.current?.focus();
        return;
      }

      // Escape: close detail drawer or deselect
      if (e.key === 'Escape') {
        e.preventDefault();
        if (selectedDetailItemId) {
          setSelectedDetailItemId(null);
          return;
        }
        if (selectedItemIds.length > 0) {
          clearSelection();
          return;
        }
        if (focusedItemId) {
          setFocusedItemId(null);
          return;
        }
        if (searchQuery) {
          setSearchQuery('');
          return;
        }
      }

      // Up/Down navigation (J/K)
      if (e.key === 'j' || e.key === 'ArrowDown') {
        e.preventDefault();
        if (sortedDerivedItems.length === 0) return;
        const currIdx = sortedDerivedItems.findIndex((d) => d.item.id === focusedItemId);
        const nextIdx = currIdx === -1 ? 0 : Math.min(currIdx + 1, sortedDerivedItems.length - 1);
        const nextId = sortedDerivedItems[nextIdx].item.id;
        setFocusedItemId(nextId);
        document.querySelector(`[data-item-id="${nextId}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
        return;
      }

      if (e.key === 'k' || e.key === 'ArrowUp') {
        e.preventDefault();
        if (sortedDerivedItems.length === 0) return;
        const currIdx = sortedDerivedItems.findIndex((d) => d.item.id === focusedItemId);
        const prevIdx = currIdx === -1 ? 0 : Math.max(currIdx - 1, 0);
        const prevId = sortedDerivedItems[prevIdx].item.id;
        setFocusedItemId(prevId);
        document.querySelector(`[data-item-id="${prevId}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
        return;
      }

      // Selection toggle (X)
      if (e.key === 'x') {
        e.preventDefault();
        if (focusedItemId) {
          toggleSelectItem(focusedItemId);
        } else if (sortedDerivedItems.length > 0) {
          toggleSelectItem(sortedDerivedItems[0].item.id);
        }
        return;
      }

      // Open detail drawer (Enter / O)
      if (e.key === 'Enter' || e.key.toLowerCase() === 'o') {
        e.preventDefault();
        if (focusedItemId) {
          setSelectedDetailItemId(focusedItemId);
        } else if (sortedDerivedItems.length > 0) {
          setSelectedDetailItemId(sortedDerivedItems[0].item.id);
        }
        return;
      }

      // Accept suggestion (A)
      if (e.key.toLowerCase() === 'a') {
        e.preventDefault();
        const targetId = focusedItemId || (selectedItemIds.length > 0 ? selectedItemIds[0] : null);
        if (targetId) {
          const att = attentionMap[targetId];
          const wsId = att?.factors?.suggested_workspace_id;
          if (wsId) {
            acceptItemSuggestion(targetId, wsId).catch(console.error);
          }
        }
        return;
      }

      // Dismiss suggestion (D)
      if (e.key.toLowerCase() === 'd') {
        e.preventDefault();
        if (selectedItemIds.length > 0) {
          dismissItemIdsSuggestions(selectedItemIds).catch(console.error);
        } else if (focusedItemId) {
          dismissItemIdsSuggestions([focusedItemId]).catch(console.error);
        }
        return;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [
    sortedDerivedItems,
    focusedItemId,
    selectedItemIds,
    selectedDetailItemId,
    searchQuery,
    attentionMap,
    setSearchQuery,
    setFocusedItemId,
    toggleSelectItem,
    clearSelection,
    acceptItemSuggestion,
    dismissItemIdsSuggestions,
  ]);

  const handleBulkAssign = async (workspaceId: string) => {
    await bulkAssignToWorkspace(workspaceId);
  };

  const handleBulkDismiss = async () => {
    await bulkDismissSuggestions();
  };

  const handleBulkClear = async () => {
    await bulkArchiveOrClear();
  };

  const detectedCandidateCount = candidates.filter((c) => c.status === 'detected').length;

  return (
    <div className="flex-1 w-full h-full bg-[#FAFAFA] flex flex-col overflow-hidden select-none">
      {/* ==================================================================== */}
      {/* 1. SIMPLE INBOX HEADER */}
      {/* ==================================================================== */}
      <header
        data-tauri-drag-region
        className="px-8 pt-7 pb-5 bg-white border-b border-black/5 flex items-center justify-between flex-wrap gap-4 shrink-0"
      >
        {/* Left: Title & Count */}
        <div className="flex flex-col">
          <h1 className="text-2xl font-bold tracking-tight text-gray-900">
            Inbox
          </h1>
          <p className="text-[13px] text-gray-400 mt-0.5">
            {items.length === 0
              ? 'No items captured'
              : `${items.length} ${items.length === 1 ? 'item' : 'items'} captured`}
          </p>
        </div>

        {/* Right: Search, Filter, Sort, Capture */}
        <div className="flex items-center gap-2.5 flex-wrap">
          {/* Search anything... */}
          <div className="relative group w-56 sm:w-64">
            <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none text-gray-400">
              <Search size={14} />
            </div>
            <input
              ref={searchInputRef}
              id="inbox-search-input"
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search anything..."
              className="w-full bg-gray-50 hover:bg-gray-100/70 focus:bg-white border border-black/8 rounded-lg py-1.5 pl-9 pr-8 text-[13px] placeholder-gray-400 outline-none focus:ring-1 focus:ring-black/20 transition-all"
            />
            {searchQuery ? (
              <button
                onClick={() => setSearchQuery('')}
                className="absolute inset-y-0 right-0 pr-2.5 flex items-center text-gray-400 hover:text-gray-700 cursor-pointer"
              >
                <X size={13} />
              </button>
            ) : (
              <div className="absolute inset-y-0 right-0 pr-2.5 flex items-center pointer-events-none">
                <kbd className="text-[10px] text-gray-400 font-mono bg-gray-100 px-1.5 py-0.2 rounded border border-black/5">
                  {modKey}K
                </kbd>
              </div>
            )}
          </div>

          {/* Subtle Filter Control */}
          <InboxFilterPopover workspaces={workspaces} />

          {/* Subtle Sort Control */}
          <button
            onClick={() => setSortOrder(sortOrder === 'newest' ? 'oldest' : 'newest')}
            className="flex items-center gap-1.5 px-2.5 py-1.5 bg-white hover:bg-gray-50 text-gray-600 hover:text-gray-900 border border-black/8 rounded-lg text-[12px] font-medium transition-colors cursor-pointer"
            title={`Sort by capture date: currently ${sortOrder === 'newest' ? 'Newest' : 'Oldest'}`}
          >
            <ArrowDownUp size={13} className="text-gray-400" />
            <span>{sortOrder === 'newest' ? 'Newest ↓' : 'Oldest ↑'}</span>
          </button>

          {/* Secondary Discover Trigger */}
          <button
            onClick={runDiscoveryScan}
            disabled={isDiscoverLoading}
            title="Scan for context patterns"
            className="flex items-center gap-1.5 px-2.5 py-1.5 bg-white hover:bg-gray-50 text-gray-600 hover:text-gray-900 border border-black/8 rounded-lg text-[12px] font-medium transition-colors cursor-pointer"
          >
            <Sparkles size={13} className={`text-amber-500 ${isDiscoverLoading ? 'animate-spin' : ''}`} />
            <span className="hidden sm:inline">Discover</span>
            {detectedCandidateCount > 0 && (
              <span className="w-1.5 h-1.5 rounded-full bg-amber-500" />
            )}
          </button>

          {/* Primary + Capture Button */}
          <button
            onClick={() => setIsCaptureModalOpen(true)}
            className="flex items-center gap-1.5 px-3.5 py-1.5 bg-gray-900 hover:bg-black text-white rounded-lg text-[12px] font-semibold transition-colors cursor-pointer shadow-xs ml-1"
          >
            <Plus size={14} strokeWidth={2.5} />
            <span>Capture</span>
          </button>
        </div>
      </header>

      {/* ==================================================================== */}
      {/* 2. MAIN SCROLLABLE INBOX LIST */}
      {/* ==================================================================== */}
      <div className="flex-1 overflow-y-auto px-8 py-7">
        <div className="max-w-4xl mx-auto flex flex-col gap-6">
          {/* EMPTY STATE */}
          {items.length === 0 ? (
            <div className="py-24 flex flex-col items-center justify-center text-center">
              <div className="w-12 h-12 rounded-2xl bg-gray-100 flex items-center justify-center mb-4 text-gray-400">
                <Inbox size={24} strokeWidth={1.5} />
              </div>
              <h2 className="text-lg font-semibold text-gray-800 mb-1.5">
                Nothing here yet.
              </h2>
              <p className="text-[13px] text-gray-500 max-w-sm mb-6 leading-relaxed">
                Capture something from anywhere and ContextSplit will help organize it by context.
              </p>
              <button
                onClick={() => setIsCaptureModalOpen(true)}
                className="flex items-center gap-2 px-4 py-2 bg-gray-900 hover:bg-black text-white rounded-xl text-xs font-semibold shadow-xs transition-colors cursor-pointer"
              >
                <Plus size={14} strokeWidth={2.5} />
                <span>Capture something</span>
              </button>
            </div>
          ) : sortedDerivedItems.length === 0 ? (
            /* NO SEARCH / FILTER MATCHES */
            <div className="py-20 flex flex-col items-center justify-center text-center">
              <div className="w-10 h-10 rounded-xl bg-gray-100 flex items-center justify-center mb-3 text-gray-400">
                <Search size={20} />
              </div>
              <h2 className="text-[15px] font-semibold text-gray-800 mb-1">
                No matching items
              </h2>
              <p className="text-[13px] text-gray-400 max-w-xs mb-4">
                No captures match your current search query or active filter settings.
              </p>
              <button
                onClick={resetAllFilters}
                className="px-3 py-1.5 bg-gray-100 hover:bg-gray-200 text-gray-700 rounded-lg text-xs font-medium transition-colors cursor-pointer"
              >
                Reset filters
              </button>
            </div>
          ) : (
            /* CHRONOLOGICAL GROUPS */
            timeGroups.map((group) => (
              <div key={group.label} className="flex flex-col gap-2">
                {/* Group Heading */}
                <div className="px-1 flex items-center justify-between">
                  <span className="text-[11px] font-semibold text-gray-400 tracking-wider uppercase">
                    {group.label}
                  </span>
                  <span className="text-[11px] text-gray-400 font-mono">
                    {group.items.length}
                  </span>
                </div>

                {/* Items List Box */}
                <div className="bg-white rounded-xl border border-black/8 shadow-2xs overflow-hidden divide-y divide-black/5">
                  {group.items.map((derived) => (
                    <InboxItemCard
                      key={derived.item.id}
                      derived={derived}
                      workspaces={workspaces}
                      onSelectItem={handleSelectItem}
                      onMoveToWorkspace={onMoveToWorkspace}
                    />
                  ))}
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      {/* ==================================================================== */}
      {/* 3. MULTI-SELECT BULK ACTIONS BAR */}
      {/* ==================================================================== */}
      <BulkActionsBar
        selectedCount={selectedItemIds.length}
        totalCount={sortedDerivedItems.length}
        workspaces={workspaces}
        isProcessing={isBatchProcessing}
        onSelectAll={() => selectAllItems(sortedDerivedItems.map((d) => d.item.id))}
        onClearSelection={clearSelection}
        onBulkAssign={handleBulkAssign}
        onBulkDismiss={handleBulkDismiss}
        onBulkClear={handleBulkClear}
      />

      {/* ==================================================================== */}
      {/* 4. PROGRESSIVE DISCLOSURE DETAIL DRAWER */}
      {/* ==================================================================== */}
      <InboxDetailDrawer
        derived={selectedDerivedItem}
        workspaces={workspaces}
        onClose={() => setSelectedDetailItemId(null)}
        onSelectItem={(id) => setSelectedDetailItemId(id)}
        onMoveToWorkspace={onMoveToWorkspace}
      />

      {/* ==================================================================== */}
      {/* 5. QUICK CAPTURE MODAL */}
      {/* ==================================================================== */}
      <QuickCaptureModal
        isOpen={isCaptureModalOpen}
        onClose={() => setIsCaptureModalOpen(false)}
      />
    </div>
  );
}
