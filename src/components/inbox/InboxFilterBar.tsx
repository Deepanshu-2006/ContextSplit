import React from 'react';
import {
  Filter,
  Sparkles,
  X,
  Calendar,
  Layers,
  Folder,
  FileType,
  Activity,
  RotateCcw,
  SlidersHorizontal,
} from 'lucide-react';
import { Workspace, ContextCandidate } from '../../services/db';
import { useInboxStore, DatePreset } from '../../store/inboxStore';

interface InboxFilterBarProps {
  workspaces: Workspace[];
  candidates: ContextCandidate[];
}

export const InboxFilterBar: React.FC<InboxFilterBarProps> = ({
  workspaces,
  candidates,
}) => {
  const {
    searchQuery,
    filterWorkspaceId,
    filterCandidateId,
    filterItemType,
    filterDatePreset,
    filterInboxState,
    isSemanticSearchActive,
    isFilterBarOpen,
    searchResults,
    isSearching,
    setFilterWorkspaceId,
    setFilterCandidateId,
    setFilterItemType,
    setFilterDatePreset,
    setFilterInboxState,
    setIsSemanticSearchActive,
    toggleFilterBar,
    resetAllFilters,
    getActiveFilterCount,
  } = useInboxStore();

  const activeFilterCount = getActiveFilterCount();
  const isAnySearchActive = searchQuery.trim().length > 0 || activeFilterCount > 0;

  const itemTypes = [
    { label: 'All Types', value: null },
    { label: 'Text', value: 'text' },
    { label: 'Image', value: 'image' },
    { label: 'URL', value: 'url' },
    { label: 'File', value: 'file' },
  ];

  const datePresets: { label: string; value: DatePreset }[] = [
    { label: 'Any Date', value: 'all' },
    { label: 'Today', value: 'today' },
    { label: 'Last 7 Days', value: '7d' },
    { label: 'Last 30 Days', value: '30d' },
  ];

  const inboxStates = [
    { label: 'All States', value: null },
    { label: 'New', value: 'new' },
    { label: 'Processing', value: 'processing' },
    { label: 'Suggested', value: 'suggested' },
    { label: 'Needs Review', value: 'needs_review' },
    { label: 'Understood', value: 'understood' },
    { label: 'Organized', value: 'organized' },
    { label: 'Dismissed', value: 'dismissed' },
  ];

  const selectedWorkspace = workspaces.find((w) => w.id === filterWorkspaceId);
  const selectedCandidate = candidates.find((c) => c.id === filterCandidateId);

  return (
    <div className="flex flex-col gap-2 w-full">
      {/* Quick Filter Bar Controls Header */}
      <div className="flex items-center justify-between flex-wrap gap-2 text-[12px]">
        <div className="flex items-center gap-2 flex-wrap">
          {/* Toggle Filter Panel Button */}
          <button
            onClick={toggleFilterBar}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg border font-medium transition-all cursor-pointer shadow-2xs ${
              isFilterBarOpen || activeFilterCount > 0
                ? 'bg-indigo-50 border-indigo-200 text-indigo-700'
                : 'bg-white hover:bg-gray-50 border-black/6 text-gray-700'
            }`}
            title="Toggle advanced multi-faceted filters"
          >
            <SlidersHorizontal size={12} className={activeFilterCount > 0 ? 'text-indigo-600' : 'text-gray-500'} />
            <span>Filters</span>
            {activeFilterCount > 0 && (
              <span className="bg-indigo-600 text-white text-[10px] font-bold px-1.5 py-0.2 rounded-full">
                {activeFilterCount}
              </span>
            )}
          </button>

          {/* Local Semantic Search Toggle Pill */}
          <button
            onClick={() => setIsSemanticSearchActive(!isSemanticSearchActive)}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-lg border font-medium transition-all cursor-pointer shadow-2xs ${
              isSemanticSearchActive
                ? 'bg-gradient-to-r from-violet-600 to-indigo-600 text-white border-transparent shadow-xs ring-2 ring-indigo-200'
                : 'bg-white hover:bg-gray-50 border-black/6 text-gray-700'
            }`}
            title="Toggle semantic vector similarity search across items"
          >
            <Sparkles size={12} className={isSemanticSearchActive ? 'text-amber-200 animate-spin-slow' : 'text-indigo-500'} />
            <span>Semantic Search</span>
            {isSemanticSearchActive && (
              <span className="text-[10px] bg-white/20 px-1 py-0.2 rounded font-semibold">ON</span>
            )}
          </button>

          {/* Active Search / Result Status Indicator */}
          {isSearching && (
            <span className="text-indigo-600 flex items-center gap-1.5 text-[11px] font-medium animate-pulse">
              <Activity size={12} className="animate-spin" /> Searching...
            </span>
          )}

          {!isSearching && searchResults !== null && (
            <span className="text-gray-600 text-[11px] font-medium bg-gray-100/90 px-2 py-0.5 rounded-full border border-black/5">
              Found <strong className="text-gray-900">{searchResults.length}</strong> {searchResults.length === 1 ? 'match' : 'matches'}
              {isSemanticSearchActive && ' (Vector Ranked)'}
            </span>
          )}
        </div>

        {/* Clear All Filters Button */}
        {isAnySearchActive && (
          <button
            onClick={resetAllFilters}
            className="flex items-center gap-1 text-[11px] font-medium text-gray-500 hover:text-gray-800 transition-colors cursor-pointer px-2 py-0.5 rounded hover:bg-gray-100"
          >
            <RotateCcw size={11} />
            <span>Reset filters</span>
          </button>
        )}
      </div>

      {/* Expandable Advanced Multi-Filter Options Panel */}
      {isFilterBarOpen && (
        <div className="bg-white/90 backdrop-blur-md border border-black/6 rounded-xl p-3 shadow-xs flex flex-wrap gap-4 items-center animate-in fade-in slide-in-from-top-1 duration-150">
          {/* 1. Item Type Filter */}
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] font-semibold text-gray-500 flex items-center gap-1">
              <FileType size={12} /> Type:
            </span>
            <select
              value={filterItemType || ''}
              onChange={(e) => setFilterItemType(e.target.value || null)}
              aria-label="Filter by capture type"
              className="bg-gray-50 hover:bg-gray-100/80 border border-black/8 rounded-lg px-2 py-1 text-[11px] font-medium text-gray-800 focus:outline-none focus:ring-1 focus:ring-indigo-500 cursor-pointer"
            >
              {itemTypes.map((t) => (
                <option key={t.label} value={t.value || ''}>
                  {t.label}
                </option>
              ))}
            </select>
          </div>

          {/* 2. Workspace Filter */}
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] font-semibold text-gray-500 flex items-center gap-1">
              <Folder size={12} /> Workspace:
            </span>
            <select
              value={filterWorkspaceId || ''}
              onChange={(e) => setFilterWorkspaceId(e.target.value || null)}
              aria-label="Filter by workspace"
              className="bg-gray-50 hover:bg-gray-100/80 border border-black/8 rounded-lg px-2 py-1 text-[11px] font-medium text-gray-800 focus:outline-none focus:ring-1 focus:ring-indigo-500 cursor-pointer max-w-[140px] truncate"
            >
              <option value="">All Workspaces</option>
              {workspaces.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          </div>

          {/* 3. Context Candidate Filter */}
          {candidates.length > 0 && (
            <div className="flex items-center gap-1.5">
              <span className="text-[11px] font-semibold text-gray-500 flex items-center gap-1">
                <Layers size={12} /> Candidate:
              </span>
              <select
                value={filterCandidateId || ''}
                onChange={(e) => setFilterCandidateId(e.target.value || null)}
                aria-label="Filter by context candidate"
                className="bg-gray-50 hover:bg-gray-100/80 border border-black/8 rounded-lg px-2 py-1 text-[11px] font-medium text-gray-800 focus:outline-none focus:ring-1 focus:ring-indigo-500 cursor-pointer max-w-[140px] truncate"
              >
                <option value="">All Candidates</option>
                {candidates.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.label || 'Candidate ' + c.id.slice(0, 6)}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* 4. Date Range Filter */}
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] font-semibold text-gray-500 flex items-center gap-1">
              <Calendar size={12} /> Date:
            </span>
            <select
              value={filterDatePreset}
              onChange={(e) => setFilterDatePreset(e.target.value as DatePreset)}
              aria-label="Filter by date range"
              className="bg-gray-50 hover:bg-gray-100/80 border border-black/8 rounded-lg px-2 py-1 text-[11px] font-medium text-gray-800 focus:outline-none focus:ring-1 focus:ring-indigo-500 cursor-pointer"
            >
              {datePresets.map((d) => (
                <option key={d.value} value={d.value}>
                  {d.label}
                </option>
              ))}
            </select>
          </div>

          {/* 5. Inbox State Filter */}
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] font-semibold text-gray-500 flex items-center gap-1">
              <Filter size={12} /> State:
            </span>
            <select
              value={filterInboxState || ''}
              onChange={(e) => setFilterInboxState(e.target.value || null)}
              aria-label="Filter by inbox item state"
              className="bg-gray-50 hover:bg-gray-100/80 border border-black/8 rounded-lg px-2 py-1 text-[11px] font-medium text-gray-800 focus:outline-none focus:ring-1 focus:ring-indigo-500 cursor-pointer"
            >
              {inboxStates.map((s) => (
                <option key={s.label} value={s.value || ''}>
                  {s.label}
                </option>
              ))}
            </select>
          </div>
        </div>
      )}

      {/* Active Filter Badges / Tags */}
      {activeFilterCount > 0 && (
        <div className="flex items-center gap-1.5 flex-wrap pt-0.5">
          <span className="text-[10px] uppercase font-bold tracking-wider text-gray-400 mr-1">
            Active:
          </span>

          {filterItemType && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-blue-50 text-blue-700 text-[11px] font-medium border border-blue-200">
              <span>Type: {filterItemType}</span>
              <button
                onClick={() => setFilterItemType(null)}
                className="hover:text-blue-900 cursor-pointer"
              >
                <X size={10} />
              </button>
            </span>
          )}

          {selectedWorkspace && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-violet-50 text-violet-700 text-[11px] font-medium border border-violet-200">
              <span>Workspace: {selectedWorkspace.name}</span>
              <button
                onClick={() => setFilterWorkspaceId(null)}
                className="hover:text-violet-900 cursor-pointer"
              >
                <X size={10} />
              </button>
            </span>
          )}

          {selectedCandidate && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-cyan-50 text-cyan-700 text-[11px] font-medium border border-cyan-200">
              <span>Candidate: {selectedCandidate.label || selectedCandidate.id.slice(0, 6)}</span>
              <button
                onClick={() => setFilterCandidateId(null)}
                className="hover:text-cyan-900 cursor-pointer"
              >
                <X size={10} />
              </button>
            </span>
          )}

          {filterDatePreset !== 'all' && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-emerald-50 text-emerald-700 text-[11px] font-medium border border-emerald-200">
              <span>Date: {datePresets.find((d) => d.value === filterDatePreset)?.label}</span>
              <button
                onClick={() => setFilterDatePreset('all')}
                className="hover:text-emerald-900 cursor-pointer"
              >
                <X size={10} />
              </button>
            </span>
          )}

          {filterInboxState && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-amber-50 text-amber-800 text-[11px] font-medium border border-amber-200">
              <span>State: {filterInboxState}</span>
              <button
                onClick={() => setFilterInboxState(null)}
                className="hover:text-amber-950 cursor-pointer"
              >
                <X size={10} />
              </button>
            </span>
          )}

          {isSemanticSearchActive && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-indigo-50 text-indigo-700 text-[11px] font-medium border border-indigo-200">
              <Sparkles size={10} />
              <span>Semantic Search</span>
              <button
                onClick={() => setIsSemanticSearchActive(false)}
                className="hover:text-indigo-900 cursor-pointer"
              >
                <X size={10} />
              </button>
            </span>
          )}
        </div>
      )}
    </div>
  );
};
