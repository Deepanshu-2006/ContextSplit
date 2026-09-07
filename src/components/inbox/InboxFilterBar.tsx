import React, { useState } from 'react';
import {
  Sparkles,
  X,
  Calendar,
  Layers,
  Folder,
  SlidersHorizontal,
  RotateCcw,
  FileText,
  Code2,
  Image as ImageIcon,
  Link2,
  File,
  Check,
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
    filterDateFrom,
    filterDateTo,
    filterInboxState,
    isSemanticSearchActive,
    isFilterBarOpen,
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

  const [customFrom, setCustomFrom] = useState(filterDateFrom ? filterDateFrom.slice(0, 10) : '');
  const [customTo, setCustomTo] = useState(filterDateTo ? filterDateTo.slice(0, 10) : '');

  const activeFilterCount = getActiveFilterCount();
  const isAnyFilterActive = searchQuery.trim().length > 0 || activeFilterCount > 0;

  const quickTypes = [
    { label: 'All', value: null, icon: null },
    { label: 'Text', value: 'text', icon: FileText },
    { label: 'Code', value: 'code', icon: Code2 },
    { label: 'Images', value: 'image', icon: ImageIcon },
    { label: 'Links', value: 'url', icon: Link2 },
    { label: 'Files', value: 'file', icon: File },
  ];

  const datePresets: { label: string; value: DatePreset }[] = [
    { label: 'Any Date', value: 'all' },
    { label: 'Today', value: 'today' },
    { label: 'Last 7 Days', value: '7d' },
    { label: 'Last 30 Days', value: '30d' },
    { label: 'Custom Range...', value: 'custom' },
  ];

  const inboxStates = [
    { label: 'All States', value: null },
    { label: 'Needs Review', value: 'needs_review' },
    { label: 'Suggested', value: 'suggested' },
    { label: 'New / Captured', value: 'new' },
    { label: 'Processing', value: 'processing' },
    { label: 'Understood', value: 'understood' },
    { label: 'Organized', value: 'organized' },
    { label: 'Dismissed', value: 'dismissed' },
  ];

  const selectedWorkspace = workspaces.find((w) => w.id === filterWorkspaceId);
  const selectedCandidate = candidates.find((c) => c.id === filterCandidateId);

  const handleCustomDateSubmit = () => {
    const fromIso = customFrom ? new Date(customFrom).toISOString() : null;
    const toIso = customTo ? new Date(new Date(customTo).setHours(23, 59, 59, 999)).toISOString() : null;
    setFilterDatePreset('custom', fromIso, toIso);
  };

  return (
    <div className="flex flex-col gap-2.5 w-full">
      {/* Top Filter Bar: Type Chips & Quick Controls */}
      <div className="flex items-center justify-between flex-wrap gap-2 text-[12px]">
        {/* Left: Quick Content Type Chips */}
        <div className="flex items-center gap-1.5 overflow-x-auto no-scrollbar py-0.5">
          {quickTypes.map((t) => {
            const isSelected = filterItemType === t.value;
            const Icon = t.icon;
            return (
              <button
                key={t.label}
                onClick={() => setFilterItemType(t.value)}
                className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all cursor-pointer shrink-0 border ${
                  isSelected
                    ? 'bg-gray-900 text-white border-gray-900 shadow-2xs font-semibold'
                    : 'bg-white hover:bg-gray-50 border-black/5 text-gray-600 hover:text-gray-900'
                }`}
              >
                {Icon && <Icon size={12} className={isSelected ? 'text-gray-300' : 'text-gray-400'} />}
                <span>{t.label}</span>
              </button>
            );
          })}
        </div>

        {/* Right: Advanced Filters Toggle, Semantic Search Toggle, & Clear */}
        <div className="flex items-center gap-2 flex-wrap">
          {/* Advanced Filters Panel Toggle */}
          <button
            onClick={toggleFilterBar}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg border font-medium text-[11px] transition-all cursor-pointer shadow-2xs ${
              isFilterBarOpen || activeFilterCount > 0
                ? 'bg-indigo-50 border-indigo-200 text-indigo-700'
                : 'bg-white hover:bg-gray-50 border-black/6 text-gray-700'
            }`}
            title="Toggle advanced multi-faceted filters"
          >
            <SlidersHorizontal size={12} className={activeFilterCount > 0 ? 'text-indigo-600' : 'text-gray-500'} />
            <span>More Filters</span>
            {activeFilterCount > 0 && (
              <span className="bg-indigo-600 text-white text-[10px] font-bold px-1.5 py-0.2 rounded-full">
                {activeFilterCount}
              </span>
            )}
          </button>

          {/* Semantic Search Mode Pill */}
          <button
            onClick={() => setIsSemanticSearchActive(!isSemanticSearchActive)}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg border font-medium text-[11px] transition-all cursor-pointer shadow-2xs ${
              isSemanticSearchActive
                ? 'bg-indigo-600 text-white border-indigo-600 shadow-xs'
                : 'bg-white hover:bg-gray-50 border-black/6 text-gray-700'
            }`}
            title="Search by concept & meaning using local representations"
          >
            <Sparkles size={12} className={isSemanticSearchActive ? 'text-amber-200' : 'text-indigo-500'} />
            <span>Semantic</span>
            {isSemanticSearchActive && (
              <span className="text-[9px] bg-white/20 px-1 py-0.2 rounded font-semibold uppercase tracking-wider">ON</span>
            )}
          </button>

          {/* Reset Filters */}
          {isAnyFilterActive && (
            <button
              onClick={resetAllFilters}
              className="flex items-center gap-1 text-[11px] font-medium text-gray-400 hover:text-gray-700 transition-colors cursor-pointer px-1.5 py-0.5 rounded hover:bg-gray-100"
              title="Reset all search queries and filters"
            >
              <RotateCcw size={11} />
              <span>Reset</span>
            </button>
          )}
        </div>
      </div>

      {/* Expanded Multi-Faceted Filter Panel */}
      {isFilterBarOpen && (
        <div className="bg-white border border-black/8 rounded-xl p-3.5 shadow-xs flex flex-wrap gap-4 items-center animate-in fade-in slide-in-from-top-1 duration-150">
          {/* 1. Workspace Filter */}
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] font-semibold text-gray-500 flex items-center gap-1">
              <Folder size={12} /> Workspace:
            </span>
            <select
              value={filterWorkspaceId || ''}
              onChange={(e) => setFilterWorkspaceId(e.target.value || null)}
              aria-label="Filter by workspace"
              className="bg-gray-50 hover:bg-gray-100/80 border border-black/8 rounded-lg px-2.5 py-1 text-[11px] font-medium text-gray-800 focus:outline-none focus:ring-1 focus:ring-indigo-500 cursor-pointer max-w-[150px] truncate shadow-2xs"
            >
              <option value="">All Workspaces</option>
              {workspaces.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          </div>

          {/* 2. Context Candidate Filter */}
          {candidates.length > 0 && (
            <div className="flex items-center gap-1.5">
              <span className="text-[11px] font-semibold text-gray-500 flex items-center gap-1">
                <Layers size={12} /> Candidate:
              </span>
              <select
                value={filterCandidateId || ''}
                onChange={(e) => setFilterCandidateId(e.target.value || null)}
                aria-label="Filter by context candidate"
                className="bg-gray-50 hover:bg-gray-100/80 border border-black/8 rounded-lg px-2.5 py-1 text-[11px] font-medium text-gray-800 focus:outline-none focus:ring-1 focus:ring-indigo-500 cursor-pointer max-w-[150px] truncate shadow-2xs"
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

          {/* 3. Date Range Filter */}
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] font-semibold text-gray-500 flex items-center gap-1">
              <Calendar size={12} /> Date:
            </span>
            <select
              value={filterDatePreset}
              onChange={(e) => setFilterDatePreset(e.target.value as DatePreset)}
              aria-label="Filter by date range"
              className="bg-gray-50 hover:bg-gray-100/80 border border-black/8 rounded-lg px-2.5 py-1 text-[11px] font-medium text-gray-800 focus:outline-none focus:ring-1 focus:ring-indigo-500 cursor-pointer shadow-2xs"
            >
              {datePresets.map((d) => (
                <option key={d.value} value={d.value}>
                  {d.label}
                </option>
              ))}
            </select>
          </div>

          {/* Custom Date Inputs (when 'custom' preset is selected) */}
          {filterDatePreset === 'custom' && (
            <div className="flex items-center gap-1.5 bg-gray-50 p-1 rounded-lg border border-black/5">
              <input
                type="date"
                value={customFrom}
                onChange={(e) => setCustomFrom(e.target.value)}
                className="text-[11px] bg-white px-1.5 py-0.5 rounded border border-black/6 text-gray-700"
                aria-label="From date"
              />
              <span className="text-[10px] text-gray-400">to</span>
              <input
                type="date"
                value={customTo}
                onChange={(e) => setCustomTo(e.target.value)}
                className="text-[11px] bg-white px-1.5 py-0.5 rounded border border-black/6 text-gray-700"
                aria-label="To date"
              />
              <button
                onClick={handleCustomDateSubmit}
                className="p-1 bg-indigo-600 text-white rounded hover:bg-indigo-700 cursor-pointer"
                title="Apply date range"
              >
                <Check size={11} />
              </button>
            </div>
          )}

          {/* 4. Inbox State Filter */}
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] font-semibold text-gray-500">State:</span>
            <select
              value={filterInboxState || ''}
              onChange={(e) => setFilterInboxState(e.target.value || null)}
              aria-label="Filter by inbox item state"
              className="bg-gray-50 hover:bg-gray-100/80 border border-black/8 rounded-lg px-2.5 py-1 text-[11px] font-medium text-gray-800 focus:outline-none focus:ring-1 focus:ring-indigo-500 cursor-pointer shadow-2xs"
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

      {/* Active Filter Pills Bar */}
      {activeFilterCount > 0 && (
        <div className="flex items-center gap-1.5 flex-wrap pt-0.5">
          <span className="text-[10px] uppercase font-bold tracking-wider text-gray-400 mr-1">
            Active:
          </span>

          {filterItemType && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-blue-50 text-blue-700 text-[11px] font-medium border border-blue-200 shadow-2xs">
              <span>Type: {filterItemType}</span>
              <button
                onClick={() => setFilterItemType(null)}
                className="hover:text-blue-900 cursor-pointer"
                title="Remove type filter"
              >
                <X size={10} />
              </button>
            </span>
          )}

          {selectedWorkspace && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-violet-50 text-violet-700 text-[11px] font-medium border border-violet-200 shadow-2xs">
              <span>Workspace: {selectedWorkspace.name}</span>
              <button
                onClick={() => setFilterWorkspaceId(null)}
                className="hover:text-violet-900 cursor-pointer"
                title="Remove workspace filter"
              >
                <X size={10} />
              </button>
            </span>
          )}

          {selectedCandidate && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-cyan-50 text-cyan-700 text-[11px] font-medium border border-cyan-200 shadow-2xs">
              <span>Candidate: {selectedCandidate.label || selectedCandidate.id.slice(0, 6)}</span>
              <button
                onClick={() => setFilterCandidateId(null)}
                className="hover:text-cyan-900 cursor-pointer"
                title="Remove candidate filter"
              >
                <X size={10} />
              </button>
            </span>
          )}

          {filterDatePreset !== 'all' && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-emerald-50 text-emerald-700 text-[11px] font-medium border border-emerald-200 shadow-2xs">
              <span>Date: {datePresets.find((d) => d.value === filterDatePreset)?.label}</span>
              <button
                onClick={() => setFilterDatePreset('all')}
                className="hover:text-emerald-900 cursor-pointer"
                title="Remove date filter"
              >
                <X size={10} />
              </button>
            </span>
          )}

          {filterInboxState && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-amber-50 text-amber-800 text-[11px] font-medium border border-amber-200 shadow-2xs">
              <span>State: {inboxStates.find((s) => s.value === filterInboxState)?.label || filterInboxState}</span>
              <button
                onClick={() => setFilterInboxState(null)}
                className="hover:text-amber-950 cursor-pointer"
                title="Remove state filter"
              >
                <X size={10} />
              </button>
            </span>
          )}

          {isSemanticSearchActive && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-indigo-50 text-indigo-700 text-[11px] font-medium border border-indigo-200 shadow-2xs">
              <Sparkles size={10} />
              <span>Semantic Search</span>
              <button
                onClick={() => setIsSemanticSearchActive(false)}
                className="hover:text-indigo-900 cursor-pointer"
                title="Disable semantic search"
              >
                <X size={10} />
              </button>
            </span>
          )}
        </div>
      )}

      {/* Result Status Indicator */}
      {isSearching && (
        <div className="text-[11px] text-indigo-600 font-medium animate-pulse flex items-center gap-1.5">
          <Sparkles size={11} className="animate-spin" /> Searching inbox items...
        </div>
      )}
    </div>
  );
};
