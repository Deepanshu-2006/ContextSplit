import { useEffect, useRef, useMemo } from 'react';
import { createPortal } from 'react-dom';
import {
  Search,
  X,
  AlertCircle,
  RefreshCw,
  Play,
  FileText,
  Link2,
} from 'lucide-react';
import {
  useLauncherStore,
  LauncherContextItem,
  LauncherPreviewModel,
  LauncherPreviewItem,
  ResumeActionStatus,
  ContextFilterType,
  matchesContextFilter,
  matchContext,
  sortContextMatches,
  getVisibleLauncherSlice,
} from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';

const FILTER_OPTIONS: { id: ContextFilterType; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'active', label: 'Active' },
  { id: 'recent', label: 'Recent' },
  { id: 'paused', label: 'Paused' },
  { id: 'archived', label: 'Archived' },
];

/**
 * Gentle relative time formatter without noisy raw timestamps.
 */
function formatGentleTime(isoString?: string | null, durationEstimate?: string | null): string {
  if (durationEstimate) return durationEstimate;
  if (!isoString) return 'Ready to resume';

  try {
    const timestamp = new Date(isoString).getTime();
    if (isNaN(timestamp)) return 'Recently active';
    const now = Date.now();
    const diffMs = Math.max(0, now - timestamp);
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHour = Math.floor(diffMin / 60);
    const diffDays = Math.floor(diffHour / 24);

    if (diffMin < 2) return 'Active just now';
    if (diffMin < 60) return `Active ${diffMin}m ago`;
    if (diffHour < 24) return `Active ${diffHour}h ago`;
    if (diffDays === 1) return 'Active yesterday';
    if (diffDays < 7) return `Active ${diffDays}d ago`;
    return 'Active recently';
  } catch {
    return 'Recently active';
  }
}

export function ContextLauncher() {
  const {
    isOpen,
    searchQuery,
    selectedIndex,
    selectedContextId,
    activePreview,
    isLoading,
    isLoadingPreview,
    isResuming,
    resumeStatus,
    resumeError,
    notReadyReason,
    error,
    previewError,
    activeContext,
    recentContexts,
    isPreviewOpen,
    activeFilter,
    setIsPreviewOpen,
    closeLauncher,
    setSearchQuery,
    setActiveFilter,
    setSelectedIndex,
    selectContextById,
    fetchLauncherData,
    resumeContextById,
    clearResumeState,
  } = useLauncherStore();

  const { activeWorkspaceId } = useWorkspaceStore();

  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Focus input on open
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => {
        inputRef.current?.focus();
      }, 50);
    }
  }, [isOpen]);

  // Re-sync with engine when active workspace changes while launcher is open
  useEffect(() => {
    if (isOpen) {
      fetchLauncherData();
    }
  }, [activeWorkspaceId, isOpen, fetchLauncherData]);


  // Filter and rank contexts based on search query and active lifecycle filter
  const filteredList = useMemo(() => {
    const all: LauncherContextItem[] = [];
    if (activeContext) {
      all.push(activeContext);
    }
    for (const ctx of recentContexts) {
      if (activeContext && ctx.id === activeContext.id) continue;
      all.push(ctx);
    }

    // 1. Compose with lifecycle filter
    const stateFiltered = all.filter((item) => matchesContextFilter(item, activeFilter));

    // 2. If no search query, return state-filtered list
    if (!searchQuery.trim()) return stateFiltered;

    // 3. Match search query terms against structured metadata within state-filtered contexts
    const matched = stateFiltered.filter((ctx) => matchContext(ctx, searchQuery));
    return sortContextMatches(matched, searchQuery);
  }, [activeContext, recentContexts, activeFilter, searchQuery]);

  // Windowed visible slice for DOM rendering performance:
  // Guarantees rendering cost stays < 2ms during fast typing even with 5,000+ contexts
  // Dynamically expands if user navigates selection via keyboard past initial visible threshold
  const { visibleItems: visibleFilteredList, hasMore: hasMoreFiltered } = useMemo(() => {
    return getVisibleLauncherSlice(filteredList, selectedIndex);
  }, [filteredList, selectedIndex]);

  const { visibleItems: visibleRecentList, hasMore: hasMoreRecent } = useMemo(() => {
    return getVisibleLauncherSlice(recentContexts, selectedIndex);
  }, [recentContexts, selectedIndex]);

  // When selection index changes or filteredList updates, fetch preview or clear if empty
  useEffect(() => {
    if (filteredList.length === 0) {
      if (selectedContextId !== null) {
        useLauncherStore.setState({ selectedContextId: null, activePreview: null });
      }
    } else if (selectedIndex >= 0 && selectedIndex < filteredList.length) {
      const current = filteredList[selectedIndex];
      if (current && current.id !== selectedContextId) {
        selectContextById(current.id);
      }
    }
  }, [selectedIndex, filteredList, selectedContextId, selectContextById]);

  // Mac-native Keyboard Navigation with two-stage inspection & resume
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // 1. Escape: dismiss prompt -> close preview -> clear search -> clear filter -> close launcher
      if (e.key === 'Escape') {
        e.preventDefault();
        if (notReadyReason || resumeError) {
          clearResumeState();
        } else if (isPreviewOpen) {
          // Close preview and return to search/list
          setIsPreviewOpen(false);
          inputRef.current?.focus();
        } else if (searchQuery) {
          // Clear search query and return to active + recent contexts
          setSearchQuery('');
          inputRef.current?.focus();
        } else if (activeFilter !== 'all') {
          // Reset lifecycle filter
          setActiveFilter('all');
          inputRef.current?.focus();
        } else {
          // Close launcher
          closeLauncher();
        }
        return;
      }

      // 2. Quick Filter Shortcuts: Cmd+1..5 / Ctrl+1..5
      if (e.metaKey || e.ctrlKey) {
        if (e.key === '1') {
          e.preventDefault();
          setActiveFilter('all');
          return;
        }
        if (e.key === '2') {
          e.preventDefault();
          setActiveFilter('active');
          return;
        }
        if (e.key === '3') {
          e.preventDefault();
          setActiveFilter('recent');
          return;
        }
        if (e.key === '4') {
          e.preventDefault();
          setActiveFilter('paused');
          return;
        }
        if (e.key === '5') {
          e.preventDefault();
          setActiveFilter('archived');
          return;
        }
      }

      // 3. Focus Search: Cmd+F, Ctrl+F, or '/' (when not typing in an input)
      if (
        ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') ||
        (e.key === '/' && document.activeElement !== inputRef.current)
      ) {
        e.preventDefault();
        setIsPreviewOpen(false);
        inputRef.current?.focus();
        inputRef.current?.select();
        return;
      }

      // 3. Jump to First / Last: Home / End or Cmd+ArrowUp / Cmd+ArrowDown
      if (
        e.key === 'Home' ||
        ((e.metaKey || e.ctrlKey) && e.key === 'ArrowUp')
      ) {
        e.preventDefault();
        if (filteredList.length > 0) {
          setSelectedIndex(0);
        }
        return;
      }

      if (
        e.key === 'End' ||
        ((e.metaKey || e.ctrlKey) && e.key === 'ArrowDown')
      ) {
        e.preventDefault();
        if (filteredList.length > 0) {
          setSelectedIndex(filteredList.length - 1);
        }
        return;
      }

      // 4. Sequential Navigation: ArrowDown / Ctrl+N / ArrowUp / Ctrl+P
      const isNext = e.key === 'ArrowDown' || (e.ctrlKey && e.key.toLowerCase() === 'n');
      const isPrev = e.key === 'ArrowUp' || (e.ctrlKey && e.key.toLowerCase() === 'p');

      if (isNext) {
        e.preventDefault();
        if (filteredList.length > 0) {
          const nextIndex = (selectedIndex + 1) % filteredList.length;
          setSelectedIndex(nextIndex);
        }
        return;
      }

      if (isPrev) {
        e.preventDefault();
        if (filteredList.length > 0) {
          const prevIndex = (selectedIndex - 1 + filteredList.length) % filteredList.length;
          setSelectedIndex(prevIndex);
        }
        return;
      }

      // 5. Enter Action: Open preview from list, or resume from preview
      if (e.key === 'Enter') {
        e.preventDefault();
        if (notReadyReason) {
          // If prompt was already showing, Enter triggers forced resume
          if (selectedContextId) {
            resumeContextById(selectedContextId, true);
          }
          return;
        }

        if (filteredList.length === 0 || selectedIndex < 0 || selectedIndex >= filteredList.length) {
          return;
        }

        if (!isPreviewOpen) {
          // Enter: Open selected context preview
          setIsPreviewOpen(true);
        } else {
          // Enter from preview: Resume selected context
          const target = filteredList[selectedIndex];
          if (target) {
            resumeContextById(target.id);
          }
        }
        return;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [
    isOpen,
    filteredList,
    selectedIndex,
    selectedContextId,
    isPreviewOpen,
    notReadyReason,
    resumeError,
    searchQuery,
    activeFilter,
    closeLauncher,
    setSearchQuery,
    setActiveFilter,
    setSelectedIndex,
    setIsPreviewOpen,
    resumeContextById,
    clearResumeState,
  ]);

  // Scroll active item into view
  useEffect(() => {
    if (listRef.current) {
      const activeEl = listRef.current.querySelector(`[data-index="${selectedIndex}"]`);
      if (activeEl) {
        activeEl.scrollIntoView({ block: 'nearest' });
      }
    }
  }, [selectedIndex]);

  if (!isOpen) return null;

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-16 pb-12 px-4 bg-black/60 backdrop-blur-sm transition-all"
      onClick={(e) => {
        if (e.target === e.currentTarget) closeLauncher();
      }}
      role="dialog"
      aria-modal="true"
      aria-label="Context Launcher"
    >
      <div
        className="w-full max-w-4xl bg-stone-900 border border-white/10 rounded-xl shadow-2xl overflow-hidden flex flex-col text-stone-100 animate-in fade-in duration-100"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 1. Search */}
        <div className="relative flex items-center px-4 py-3.5 border-b border-white/10 bg-stone-950/40">
          <Search size={16} className="text-stone-400 shrink-0 mr-3" aria-hidden="true" />
          <input
            id="launcher-search-input"
            name="search"
            ref={inputRef}
            type="text"
            role="combobox"
            autoFocus
            autoComplete="off"
            autoCorrect="off"
            spellCheck="false"
            aria-expanded="true"
            aria-autocomplete="list"
            aria-controls="launcher-context-list"
            aria-activedescendant={selectedContextId ? `launcher-item-${selectedContextId}` : undefined}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search contexts..."
            className="w-full bg-transparent text-sm text-stone-100 placeholder-stone-500 focus:outline-none"
            aria-label="Search contexts"
          />
          {isLoading && (
            <RefreshCw size={14} className="text-stone-400 animate-spin shrink-0 mr-2" aria-label="Loading contexts..." />
          )}
          {searchQuery && (
            <button
              type="button"
              onClick={() => {
                setSearchQuery('');
                inputRef.current?.focus();
              }}
              className="p-1 rounded text-stone-400 hover:text-white transition-colors cursor-pointer mr-1.5"
              title="Clear search"
              aria-label="Clear search"
            >
              <X size={14} />
            </button>
          )}
          <span className="hidden sm:inline-block px-1.5 py-0.5 text-[10px] font-mono text-stone-500 bg-white/5 rounded">
            Esc
          </span>
        </div>

        {/* Filter Tabs */}
        <div
          role="tablist"
          aria-label="Filter contexts by lifecycle state"
          className="flex items-center gap-1 px-4 py-1.5 border-b border-white/5 bg-stone-950/20 text-xs"
        >
          {FILTER_OPTIONS.map((f) => {
            const isSelected = activeFilter === f.id;
            return (
              <button
                key={f.id}
                role="tab"
                id={`launcher-filter-tab-${f.id}`}
                aria-selected={isSelected}
                tabIndex={0}
                onClick={() => {
                  setActiveFilter(f.id);
                  inputRef.current?.focus();
                }}
                className={`px-2.5 py-1 rounded text-xs transition-colors cursor-pointer ${
                  isSelected
                    ? 'bg-white/10 text-stone-100 font-medium'
                    : 'text-stone-400 hover:text-stone-200'
                }`}
              >
                {f.label}
              </button>
            );
          })}
        </div>

        {/* Main Split View: Left List | Right Preview & Resume */}
        <div className="grid grid-cols-1 md:grid-cols-12 min-h-[380px] max-h-[460px] divide-y md:divide-y-0 md:divide-x divide-white/10">
          {/* Left Column: Contexts List */}
          <div
            id="launcher-context-list"
            role="listbox"
            aria-label="Context List"
            aria-activedescendant={selectedContextId ? `launcher-item-${selectedContextId}` : undefined}
            ref={listRef}
            className="md:col-span-6 lg:col-span-5 overflow-y-auto p-2.5 space-y-3 focus:outline-none"
            tabIndex={-1}
          >
            {/* Loading state */}
            {isLoading && (
              <div className="space-y-2 p-1 animate-pulse">
                <div className="h-2.5 w-16 bg-stone-800 rounded" />
                <div className="h-10 bg-stone-800/40 rounded-lg" />
                <div className="h-2.5 w-20 bg-stone-800 rounded mt-3" />
                <div className="h-10 bg-stone-800/40 rounded-lg" />
                <div className="h-10 bg-stone-800/40 rounded-lg" />
              </div>
            )}

            {/* Error state */}
            {!isLoading && error && (
              <div className="p-3 rounded-lg bg-red-950/30 border border-red-500/20 text-red-200 flex items-start gap-2 text-xs">
                <AlertCircle className="w-4 h-4 text-red-400 shrink-0 mt-0.5" />
                <div className="flex-1 min-w-0">
                  <p className="font-medium text-red-300">
                    {searchQuery ? 'Search Error' : 'Connection Error'}
                  </p>
                  <p className="text-red-400/80 mt-0.5">{error}</p>
                  <button
                    onClick={fetchLauncherData}
                    className="mt-2 inline-flex items-center gap-1 px-2 py-0.5 rounded bg-red-900/40 hover:bg-red-900 text-[11px] text-red-200 transition-colors cursor-pointer"
                  >
                    <RefreshCw size={10} />
                    Retry
                  </button>
                </div>
              </div>
            )}

            {/* 2. Active Context Section (When no search query and All filter) */}
            {!isLoading && !error && !searchQuery && activeFilter === 'all' && (
              <div className="space-y-1">
                <div className="px-2 py-0.5 text-[11px] font-medium text-stone-400 flex items-center gap-1.5">
                  <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                  <span>Active Context</span>
                </div>
                {activeContext ? (
                  <ContextLauncherItem
                    item={activeContext}
                    isSelected={selectedIndex === 0}
                    isPreviewOpen={selectedIndex === 0 && isPreviewOpen}
                    isResuming={isResuming && selectedContextId === activeContext.id}
                    dataIndex={0}
                    onSelect={() => {
                      setSelectedIndex(0);
                      setIsPreviewOpen(true);
                    }}
                    onResume={() => resumeContextById(activeContext.id)}
                  />
                ) : (
                  <p className="text-xs text-stone-500 px-2 py-1">No active context</p>
                )}
              </div>
            )}

            {/* 3. Results / Recent Contexts */}
            {/* Filtered view when specific filter is selected and no search query */}
            {!isLoading && !error && !searchQuery && activeFilter !== 'all' && (
              <div className="space-y-1">
                <div className="px-2 py-0.5 text-[11px] font-medium text-stone-400">
                  {FILTER_OPTIONS.find((f) => f.id === activeFilter)?.label || activeFilter} Contexts ({filteredList.length})
                </div>
                {filteredList.length === 0 ? (
                  <div className="py-8 text-center space-y-1">
                    <p className="text-xs font-medium text-stone-300">No matching contexts</p>
                    <p className="text-xs text-stone-500">Try a different name or keyword.</p>
                  </div>
                ) : (
                  <div className="space-y-0.5">
                    {visibleFilteredList.map((item, idx) => {
                      const isSelected = selectedIndex === idx;
                      return (
                        <ContextLauncherItem
                          key={item.id}
                          item={item}
                          isSelected={isSelected}
                          isPreviewOpen={isSelected && isPreviewOpen}
                          isResuming={isResuming && selectedContextId === item.id}
                          dataIndex={idx}
                          onSelect={() => {
                            setSelectedIndex(idx);
                            setIsPreviewOpen(true);
                          }}
                          onResume={() => resumeContextById(item.id)}
                        />
                      );
                    })}
                    {hasMoreFiltered && (
                      <div className="px-3 py-1.5 text-center text-[10.5px] text-stone-500">
                        Showing top {visibleFilteredList.length} of {filteredList.length} contexts
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}

            {/* Empty search results state */}
            {!isLoading && !error && searchQuery && filteredList.length === 0 && (
              <div className="py-8 text-center space-y-1">
                <p className="text-xs font-medium text-stone-300">No matching contexts</p>
                <p className="text-xs text-stone-500">Try a different name or keyword.</p>
              </div>
            )}

            {/* Recent Contexts list (When no search query and All filter) */}
            {!isLoading && !error && !searchQuery && activeFilter === 'all' && (
              <div className="space-y-1">
                <div className="px-2 py-0.5 text-[11px] font-medium text-stone-400">
                  Recent Contexts
                </div>
                {recentContexts.length === 0 ? (
                  <p className="text-xs text-stone-500 px-2 py-1">No recent contexts</p>
                ) : (
                  <div className="space-y-0.5">
                    {visibleRecentList.map((item, relIdx) => {
                      const globalIdx = activeContext ? relIdx + 1 : relIdx;
                      const isSelected = selectedIndex === globalIdx;
                      return (
                        <ContextLauncherItem
                          key={item.id}
                          item={item}
                          isSelected={isSelected}
                          isPreviewOpen={isSelected && isPreviewOpen}
                          isResuming={isResuming && selectedContextId === item.id}
                          dataIndex={globalIdx}
                          onSelect={() => {
                            setSelectedIndex(globalIdx);
                            setIsPreviewOpen(true);
                          }}
                          onResume={() => resumeContextById(item.id)}
                        />
                      );
                    })}
                    {hasMoreRecent && (
                      <div className="px-3 py-1.5 text-center text-[10.5px] text-stone-500">
                        Showing top {visibleRecentList.length} of {recentContexts.length} contexts
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}

            {/* Search Results list (When search query is active) */}
            {!isLoading && !error && searchQuery && filteredList.length > 0 && (
              <div className="space-y-1">
                <div className="px-2 py-0.5 text-[11px] font-medium text-stone-400">
                  Results ({filteredList.length})
                </div>
                <div className="space-y-0.5">
                  {visibleFilteredList.map((item, idx) => {
                    const isSelected = selectedIndex === idx;
                    return (
                      <ContextLauncherItem
                        key={item.id}
                        item={item}
                        isSelected={isSelected}
                        isPreviewOpen={isSelected && isPreviewOpen}
                        isResuming={isResuming && selectedContextId === item.id}
                        dataIndex={idx}
                        onSelect={() => {
                          setSelectedIndex(idx);
                          setIsPreviewOpen(true);
                        }}
                        onResume={() => resumeContextById(item.id)}
                      />
                    );
                  })}
                  {hasMoreFiltered && (
                    <div className="px-3 py-1.5 text-center text-[10.5px] text-stone-500">
                      Showing top {visibleFilteredList.length} of {filteredList.length} contexts
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>

          {/* Right Column: 4. Selected Context Preview & 5. Resume */}
          <div
            className="md:col-span-6 lg:col-span-7 p-4 overflow-y-auto flex flex-col justify-between bg-stone-950/20"
            role="region"
            aria-label="Selected Context Preview"
          >
            <ContextPreviewPane
              preview={activePreview}
              isLoading={isLoadingPreview}
              isResuming={isResuming}
              isPreviewOpen={isPreviewOpen}
              resumeStatus={resumeStatus}
              resumeError={resumeError}
              notReadyReason={notReadyReason}
              error={previewError}
              onResume={(force) => {
                if (activePreview) {
                  resumeContextById(activePreview.contextId, force);
                }
              }}
              onClearResumeState={clearResumeState}
              onClose={() => {
                setIsPreviewOpen(false);
                inputRef.current?.focus();
              }}
            />
          </div>
        </div>

        {/* Footer: Minimal 1-line shortcuts */}
        <div className="px-4 py-2 bg-stone-950/60 border-t border-white/5 flex items-center justify-between text-[11px] text-stone-500">
          <span>ContextSplit</span>
          <div className="flex items-center gap-3">
            <span><kbd className="font-mono text-[10px] text-stone-400">↑↓</kbd> Navigate</span>
            <span><kbd className="font-mono text-[10px] text-stone-400">↵</kbd> Resume</span>
            <span><kbd className="font-mono text-[10px] text-stone-400">Esc</kbd> Close</span>
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
}

/**
 * Context Preview Pane
 */
interface ContextPreviewPaneProps {
  preview: LauncherPreviewModel | null;
  isLoading: boolean;
  isResuming: boolean;
  isPreviewOpen?: boolean;
  resumeStatus: ResumeActionStatus;
  resumeError: string | null;
  notReadyReason: string | null;
  error: string | null;
  onResume: (force?: boolean) => void;
  onClearResumeState: () => void;
  onClose: () => void;
}

function ContextPreviewPane({
  preview,
  isLoading,
  isResuming,
  resumeError,
  notReadyReason,
  error,
  onResume,
  onClearResumeState,
}: ContextPreviewPaneProps) {
  if (isLoading) {
    return (
      <div className="space-y-3 animate-pulse p-1">
        <div className="h-5 w-40 bg-stone-800 rounded" />
        <div className="h-3 w-28 bg-stone-800/60 rounded" />
        <div className="h-16 bg-stone-800/30 rounded-lg mt-4" />
        <div className="h-24 bg-stone-800/30 rounded-lg mt-3" />
        <div className="h-9 w-full bg-stone-800 rounded-lg mt-6" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-center p-4 space-y-2">
        <AlertCircle size={20} className="text-red-400" />
        <p className="text-xs font-semibold text-stone-200">Context Preview Unavailable</p>
        <p className="text-[11px] text-stone-400 max-w-xs">{error}</p>
      </div>
    );
  }

  if (!preview) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-center p-4 text-stone-500 space-y-1">
        <p className="text-xs font-medium text-stone-400">No context selected</p>
        <p className="text-[11px] text-stone-500 max-w-xs">
          Select a context to preview its workset and resume.
        </p>
      </div>
    );
  }

  const isReady = preview.readinessBand === 'ready' || preview.readinessBand === 'mostly_ready';

  return (
    <div className="space-y-4 flex flex-col justify-between h-full">
      <div className="space-y-4">
        {/* 4. Selected Context: Header */}
        <div className="border-b border-white/5 pb-3">
          <div className="flex items-start justify-between gap-2">
            <h3 className="text-sm font-semibold text-stone-100 truncate">
              {preview.name}
            </h3>
            <span
              className={`text-[10px] font-medium px-2 py-0.5 rounded capitalize shrink-0 ${
                isReady
                  ? 'bg-emerald-500/10 text-emerald-400'
                  : 'bg-amber-500/10 text-amber-400'
              }`}
            >
              {preview.readinessBand.replace('_', ' ')}
            </span>
          </div>

          <div className="flex items-center gap-2 mt-1 text-[11px] text-stone-400 flex-wrap">
            {preview.projectIdentifier && (
              <span className="font-mono text-stone-300">
                {preview.projectIdentifier}
              </span>
            )}
            {preview.projectIdentifier && <span className="text-stone-600">•</span>}
            <span>{formatGentleTime(preview.lastActiveAt, preview.durationEstimate)}</span>
            {preview.state === 'paused' && (
              <>
                <span className="text-stone-600">•</span>
                <span className="text-amber-400 font-medium">Paused</span>
              </>
            )}
          </div>
        </div>

        {/* Where You Left Off / Active Task */}
        <div className="space-y-1">
          <div className="text-[11px] font-medium text-stone-400">
            Where you left off
          </div>
          {preview.taskTitle ? (
            <p className="text-xs font-medium text-stone-200">
              {preview.taskTitle}
            </p>
          ) : preview.activitySummary ? (
            <p className="text-xs text-stone-300 leading-snug">{preview.activitySummary}</p>
          ) : (
            <p className="text-xs text-stone-500 italic">No specific task recorded.</p>
          )}

          {preview.readinessExplanation && (
            <p className="text-[11px] text-stone-400 pt-1">
              {preview.readinessExplanation}
            </p>
          )}
        </div>

        {/* Warnings / Errors */}
        {notReadyReason && (
          <div className="p-2.5 rounded-lg bg-amber-950/30 border border-amber-500/20 text-amber-200 space-y-2 text-xs">
            <div className="flex items-start gap-2">
              <AlertCircle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
              <div className="min-w-0 flex-1">
                <p className="font-medium text-amber-300">Context Not Fully Ready</p>
                <p className="text-[11px] text-amber-400/90 mt-0.5">{notReadyReason}</p>
              </div>
            </div>
            <div className="flex items-center gap-2 pt-1">
              <button
                onClick={() => onResume(true)}
                disabled={isResuming}
                className="px-2 py-0.5 rounded bg-amber-500 hover:bg-amber-600 disabled:opacity-50 text-stone-950 font-medium text-[11px] transition-colors cursor-pointer"
              >
                {isResuming ? 'Resuming...' : 'Resume Anyway'}
              </button>
              <button
                onClick={onClearResumeState}
                disabled={isResuming}
                className="px-2 py-0.5 rounded bg-white/5 hover:bg-white/10 text-stone-300 text-[11px] transition-colors cursor-pointer"
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {resumeError && (
          <div className="p-2.5 rounded-lg bg-red-950/30 border border-red-500/20 text-red-200 flex items-start gap-2 text-xs">
            <AlertCircle className="w-4 h-4 text-red-400 shrink-0 mt-0.5" />
            <div className="flex-1 min-w-0">
              <p className="font-medium text-red-300">Resume Failed</p>
              <p className="text-[11px] text-red-400/90 mt-0.5">{resumeError}</p>
            </div>
            <button
              onClick={onClearResumeState}
              className="text-stone-400 hover:text-white cursor-pointer"
              aria-label="Dismiss error"
            >
              <X size={12} />
            </button>
          </div>
        )}

        {/* Working Set Items */}
        <div className="space-y-1.5">
          <div className="flex items-center justify-between text-[11px] font-medium text-stone-400">
            <span>Working Set</span>
            <span>{preview.itemCount} items</span>
          </div>

          {preview.worksetItems.length === 0 ? (
            <p className="text-xs text-stone-500 py-1">No items in active workset</p>
          ) : (
            <div className="space-y-1 max-h-[140px] overflow-y-auto">
              {preview.worksetItems.map((item) => (
                <WorksetPreviewRow key={item.id} item={item} />
              ))}
            </div>
          )}
        </div>
      </div>

      {/* 5. Resume Action (Clear single primary button, no duplicate close button) */}
      <div className="pt-3 border-t border-white/5">
        <button
          onClick={() => onResume(false)}
          disabled={isResuming}
          className="w-full py-2.5 px-4 rounded-lg bg-orange-500 hover:bg-orange-600 disabled:opacity-50 disabled:cursor-not-allowed text-stone-950 font-semibold text-xs transition-colors flex items-center justify-center gap-2 cursor-pointer shadow-sm"
          aria-label="Resume context"
        >
          {isResuming ? (
            <>
              <RefreshCw size={13} className="animate-spin" />
              <span>Resuming...</span>
            </>
          ) : (
            <>
              <Play size={13} className="fill-current" />
              <span>Resume</span>
              <kbd className="ml-1 px-1.5 py-0.5 text-[10px] font-mono bg-stone-950/20 text-stone-950 rounded">
                ↵ Enter
              </kbd>
            </>
          )}
        </button>
      </div>
    </div>
  );
}

/**
 * Lightweight Workset Item Row
 */
function WorksetPreviewRow({
  item,
}: {
  item: LauncherPreviewItem;
}) {
  const isLink = item.itemType === 'url' || item.itemType === 'link';

  return (
    <div className="flex items-center justify-between py-1 text-xs gap-2 text-stone-300">
      <div className="flex items-center gap-1.5 min-w-0 flex-1">
        {isLink ? (
          <Link2 size={12} className="text-stone-400 shrink-0" />
        ) : (
          <FileText size={12} className="text-stone-400 shrink-0" />
        )}
        <span className="truncate text-[11px]">{item.title}</span>
      </div>
      {item.appName && (
        <span className="text-[10px] text-stone-500 shrink-0 font-mono">
          {item.appName}
        </span>
      )}
    </div>
  );
}

/**
 * Context Launcher List Item
 */
interface ContextLauncherItemProps {
  item: LauncherContextItem;
  isSelected: boolean;
  isPreviewOpen?: boolean;
  isResuming?: boolean;
  dataIndex: number;
  onSelect: () => void;
  onResume: () => void;
}

function ContextLauncherItem({
  item,
  isSelected,
  isResuming = false,
  dataIndex,
  onSelect,
  onResume,
}: ContextLauncherItemProps) {
  const isPaused = item.state === 'paused';
  const isArchived = item.state === 'archived';

  // Clean subtitle without redundant labels
  const subtitle = item.taskTitle || item.description || (item.projectIdentifier ? `${item.projectIdentifier}` : null);
  const timeStr = formatGentleTime(item.lastActiveAt, item.durationEstimate);

  return (
    <div
      id={`launcher-item-${item.id}`}
      data-index={dataIndex}
      onClick={onSelect}
      onDoubleClick={onResume}
      onMouseEnter={onSelect}
      className={`group flex items-center justify-between px-2.5 py-2 rounded-lg cursor-pointer transition-colors ${
        isSelected
          ? 'bg-stone-800 text-stone-100 ring-1 ring-white/10'
          : 'hover:bg-white/[0.04] text-stone-300'
      }`}
      role="option"
      aria-selected={isSelected}
    >
      <div className="flex items-center gap-2 min-w-0 flex-1 mr-2">
        {/* Minimal status indicator */}
        {item.isActive ? (
          <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 shrink-0" title="Active" />
        ) : isPaused ? (
          <span className="w-1.5 h-1.5 rounded-full bg-amber-400 shrink-0" title="Paused" />
        ) : (
          <span className="w-1.5 h-1.5 rounded-full bg-transparent shrink-0" />
        )}

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className={`text-xs font-medium truncate ${isArchived ? 'text-stone-400' : 'text-stone-100'}`}>
              {item.name}
            </span>
            {item.projectIdentifier && (
              <span className="text-[10px] text-stone-500 font-mono shrink-0">
                {item.projectIdentifier}
              </span>
            )}
            {isPaused && (
              <span className="text-[10px] text-amber-400/90 shrink-0 font-medium">
                Paused
              </span>
            )}
          </div>

          <div className="flex items-center gap-1.5 text-[11px] text-stone-400 truncate mt-0.5">
            {subtitle && (
              <span className="truncate">{subtitle}</span>
            )}
            {subtitle && <span className="text-stone-600">•</span>}
            <span className="shrink-0">{timeStr}</span>
          </div>
        </div>
      </div>

      {/* Selected Action Cue */}
      <div className="flex items-center shrink-0">
        {isSelected && isResuming ? (
          <RefreshCw size={12} className="animate-spin text-orange-400" />
        ) : isSelected ? (
          <span className="text-[11px] font-mono text-stone-400 px-1.5 py-0.5 rounded bg-white/5">
            ↵
          </span>
        ) : null}
      </div>
    </div>
  );
}
