import { useEffect, useRef, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  Search,
  X,
  AlertCircle,
  RefreshCw,
  Play,
  FileText,
  Link2,
  MoreHorizontal,
  Pause,
  Archive,
  Star,
  ExternalLink,
  Trash2,
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
import {
  ContextActionId,
  ItemActionId,
  getAvailableContextActions,
  getAvailableItemActions,
  getPrimaryContextActionDescriptor,
  resolveContextLifecycleState,
} from '../../models/quickActions';

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
    isActionExecuting,
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
    executeContextAction,
    executeItemAction,
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
      // If focus is currently inside an open menu or active menu trigger, let the menu manage keyboard navigation
      const activeEl = document.activeElement as HTMLElement | null;
      if (
        activeEl?.closest('[role="menu"]') ||
        (activeEl?.getAttribute('aria-expanded') === 'true' && activeEl?.closest('[data-menu-trigger="true"]'))
      ) {
        return;
      }

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

      // 5. Context Quick Actions Shortcuts:
      // Cmd+O / Ctrl+O: Open Context Details
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'o') {
        e.preventDefault();
        const targetId = selectedContextId || filteredList[selectedIndex]?.id;
        if (targetId) {
          executeContextAction('context.open_details', targetId);
        }
        return;
      }

      // Alt+P / Option+P: Pause Active Context
      if (e.altKey && e.key.toLowerCase() === 'p') {
        e.preventDefault();
        const targetId = selectedContextId || filteredList[selectedIndex]?.id;
        if (targetId) {
          executeContextAction('context.pause', targetId);
        }
        return;
      }

      // 6. Enter Action: Open preview from list, or resume from preview
      if (e.key === 'Enter') {
        e.preventDefault();
        if (isResuming || isActionExecuting) return;

        if (notReadyReason) {
          // If prompt was already showing, Enter triggers forced resume
          if (selectedContextId) {
            executeContextAction('context.resume', selectedContextId, { force: true });
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
            executeContextAction('context.resume', target.id);
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
    isResuming,
    isActionExecuting,
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
    executeContextAction,
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
              isActionExecuting={isActionExecuting}
              isPreviewOpen={isPreviewOpen}
              resumeStatus={resumeStatus}
              resumeError={resumeError}
              notReadyReason={notReadyReason}
              error={previewError}
              isActiveContext={Boolean(
                activeContext && activePreview && activePreview.contextId === activeContext.id
              )}
              onResume={(force) => {
                if (activePreview) {
                  executeContextAction('context.resume', activePreview.contextId, { force });
                }
              }}
              onExecuteContextAction={(actionId, options) => {
                if (activePreview) {
                  executeContextAction(actionId, activePreview.contextId, options);
                }
              }}
              onExecuteItemAction={(actionId, contextId, itemId) => {
                executeItemAction(actionId, contextId, itemId);
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
            <span><kbd className="font-mono text-[10px] text-stone-400">⌘O</kbd> Details</span>
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
  isActionExecuting?: boolean;
  isPreviewOpen?: boolean;
  resumeStatus: ResumeActionStatus;
  resumeError: string | null;
  notReadyReason: string | null;
  error: string | null;
  isActiveContext?: boolean;
  onResume: (force?: boolean) => void;
  onExecuteContextAction?: (actionId: ContextActionId, options?: { force?: boolean }) => void;
  onExecuteItemAction?: (actionId: ItemActionId, contextId: string, itemId: string) => void;
  onClearResumeState: () => void;
  onClose: () => void;
}

function ContextPreviewPane({
  preview,
  isLoading,
  isResuming,
  isActionExecuting = false,
  resumeError,
  notReadyReason,
  error,
  isActiveContext = false,
  onResume,
  onExecuteContextAction,
  onExecuteItemAction,
  onClearResumeState,
}: ContextPreviewPaneProps) {
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const [focusedMenuIndex, setFocusedMenuIndex] = useState(-1);
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuItemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  // Close menu when selected context changes
  useEffect(() => {
    setIsMenuOpen(false);
    setFocusedMenuIndex(-1);
  }, [preview?.contextId]);

  // Click outside handling for context secondary menu
  useEffect(() => {
    if (!isMenuOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setIsMenuOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isMenuOpen]);

  const isCurrentActive = Boolean(isActiveContext || (preview && preview.state === 'active'));

  const contextLifecycleState = useMemo(() => {
    return resolveContextLifecycleState({
      lifecycleState: preview?.state,
      isActive: isCurrentActive,
    });
  }, [preview?.state, isCurrentActive]);

  const primaryActionDescriptor = useMemo(() => {
    return getPrimaryContextActionDescriptor(contextLifecycleState);
  }, [contextLifecycleState]);

  const isArchived = contextLifecycleState === 'archived';

  const availableActions = useMemo(() => {
    if (!preview) return [];
    return getAvailableContextActions({
      contextId: preview.contextId,
      lifecycleState: preview.state,
      isActive: isCurrentActive,
      readinessBand: preview.readinessBand,
    });
  }, [preview, isCurrentActive]);

  // Secondary actions: all context actions except primary resume
  const secondaryActions = useMemo(() => {
    return availableActions.filter((a) => a.id !== 'context.resume');
  }, [availableActions]);

  // Focus management when menu opens
  useEffect(() => {
    if (isMenuOpen) {
      const firstEnabledIdx = secondaryActions.findIndex((a) => a.isSupported);
      const targetIdx = firstEnabledIdx >= 0 ? firstEnabledIdx : 0;
      setFocusedMenuIndex(targetIdx);
      setTimeout(() => {
        menuItemRefs.current[targetIdx]?.focus();
      }, 30);
    } else {
      setFocusedMenuIndex(-1);
    }
  }, [isMenuOpen, secondaryActions]);

  const handleMenuKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      e.stopPropagation();
      setFocusedMenuIndex((prev) => {
        const next = (prev + 1) % secondaryActions.length;
        menuItemRefs.current[next]?.focus();
        return next;
      });
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      e.stopPropagation();
      setFocusedMenuIndex((prev) => {
        const next = (prev - 1 + secondaryActions.length) % secondaryActions.length;
        menuItemRefs.current[next]?.focus();
        return next;
      });
    } else if (e.key === 'Home') {
      e.preventDefault();
      e.stopPropagation();
      setFocusedMenuIndex(0);
      menuItemRefs.current[0]?.focus();
    } else if (e.key === 'End') {
      e.preventDefault();
      e.stopPropagation();
      const last = secondaryActions.length - 1;
      setFocusedMenuIndex(last);
      menuItemRefs.current[last]?.focus();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      setIsMenuOpen(false);
      triggerRef.current?.focus();
    } else if (e.key === 'Tab') {
      setIsMenuOpen(false);
    }
  };

  const primaryButtonLabel = isResuming
    ? primaryActionDescriptor.inProgressLabel
    : primaryActionDescriptor.label === 'Resume Context'
    ? 'Resume'
    : primaryActionDescriptor.label;

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
            {isArchived && (
              <>
                <span className="text-stone-600">•</span>
                <span className="text-stone-500 font-medium">Archived</span>
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
                onClick={() => {
                  if (onExecuteContextAction) {
                    onExecuteContextAction('context.resume', { force: true });
                  } else {
                    onResume(true);
                  }
                }}
                disabled={isResuming || isActionExecuting}
                className="px-2 py-0.5 rounded bg-amber-500 hover:bg-amber-600 disabled:opacity-50 text-stone-950 font-medium text-[11px] transition-colors cursor-pointer"
              >
                {isResuming ? 'Resuming...' : 'Resume Anyway'}
              </button>
              <button
                onClick={onClearResumeState}
                disabled={isResuming || isActionExecuting}
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
                <WorksetPreviewRow
                  key={item.id}
                  item={item}
                  contextId={preview.contextId}
                  contextState={preview.state}
                  isExecuting={isActionExecuting}
                  onExecuteItemAction={onExecuteItemAction}
                />
              ))}
            </div>
          )}
        </div>
      </div>

      {/* 5. Primary Action & Compact Secondary Actions Menu */}
      <div className="pt-3 border-t border-white/5 flex items-center gap-2 relative">
        <button
          id="launcher-primary-action-btn"
          onClick={() => {
            if (onExecuteContextAction) {
              onExecuteContextAction('context.resume', { force: false });
            } else {
              onResume(false);
            }
          }}
          disabled={isResuming || isActionExecuting}
          className="flex-1 py-2.5 px-4 rounded-lg bg-orange-500 hover:bg-orange-600 disabled:opacity-50 disabled:cursor-not-allowed text-stone-950 font-semibold text-xs transition-colors flex items-center justify-center gap-2 cursor-pointer shadow-sm"
          aria-label={primaryButtonLabel}
        >
          {isResuming ? (
            <>
              <RefreshCw size={13} className="animate-spin" />
              <span>{primaryButtonLabel}</span>
            </>
          ) : (
            <>
              <Play size={13} className="fill-current" />
              <span>{primaryButtonLabel}</span>
              <kbd className="ml-1 px-1.5 py-0.5 text-[10px] font-mono bg-stone-950/20 text-stone-950 rounded">
                ↵ Enter
              </kbd>
            </>
          )}
        </button>

        {/* Compact Secondary Actions Menu Trigger */}
        <div className="relative shrink-0" ref={menuRef}>
          <button
            id="launcher-more-actions-btn"
            ref={triggerRef}
            data-menu-trigger="true"
            type="button"
            onClick={() => setIsMenuOpen((prev) => !prev)}
            disabled={isResuming || isActionExecuting}
            className={`p-2.5 rounded-lg border border-white/10 transition-colors cursor-pointer flex items-center justify-center ${
              isMenuOpen
                ? 'bg-stone-800 text-stone-100 ring-1 ring-white/20'
                : 'bg-stone-950/40 hover:bg-stone-800/80 text-stone-400 hover:text-stone-200'
            }`}
            title="More context actions"
            aria-label="More actions for selected context"
            aria-haspopup="menu"
            aria-expanded={isMenuOpen}
            aria-controls="launcher-context-secondary-menu"
          >
            <MoreHorizontal size={14} />
          </button>

          {/* Compact Popover Menu */}
          {isMenuOpen && (
            <div
              id="launcher-context-secondary-menu"
              role="menu"
              aria-label="Context secondary actions"
              onKeyDown={handleMenuKeyDown}
              tabIndex={-1}
              className="absolute bottom-full mb-1.5 right-0 w-64 bg-stone-900 border border-white/10 rounded-lg shadow-2xl p-1.5 z-30 animate-in fade-in zoom-in-95 duration-100 focus:outline-none"
            >
              <div className="px-2.5 py-1 text-[10px] font-medium text-stone-500 uppercase tracking-wider">
                Context Actions
              </div>
              <div className="space-y-0.5">
                {secondaryActions.map((action, idx) => {
                  const isEnabled = action.isSupported;
                  const isFocused = focusedMenuIndex === idx;
                  return (
                    <button
                      key={action.id}
                      ref={(el) => {
                        menuItemRefs.current[idx] = el;
                      }}
                      role="menuitem"
                      type="button"
                      tabIndex={isFocused ? 0 : -1}
                      disabled={!isEnabled || isResuming || isActionExecuting}
                      aria-disabled={!isEnabled || isResuming || isActionExecuting}
                      onClick={() => {
                        if (!isEnabled || isResuming || isActionExecuting) return;
                        setIsMenuOpen(false);
                        if (onExecuteContextAction) {
                          onExecuteContextAction(action.id);
                        }
                      }}
                      title={!isEnabled ? action.unsupportedReason : action.description}
                      className={`w-full flex items-start gap-2.5 px-2.5 py-1.5 rounded-md text-xs text-left transition-colors ${
                        !isEnabled
                          ? 'opacity-40 cursor-not-allowed text-stone-500'
                          : isFocused
                          ? 'bg-white/10 text-stone-100 cursor-pointer'
                          : 'text-stone-300 hover:text-stone-100 hover:bg-white/10 cursor-pointer'
                      }`}
                    >
                      <div className="mt-0.5 shrink-0">
                        {action.id === 'context.open_details' && (
                          <ExternalLink size={13} />
                        )}
                        {action.id === 'context.pause' && (
                          <Pause size={13} />
                        )}
                        {action.id === 'context.archive' && (
                          <Archive size={13} />
                        )}
                        {action.id === 'context.delete' && (
                          <Trash2 size={13} />
                        )}
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center justify-between gap-1">
                          <span className="truncate font-medium">{action.label}</span>
                          {action.shortcut && (
                            <kbd className="text-[10px] font-mono text-stone-500 bg-white/5 px-1.5 py-0.5 rounded shrink-0">
                              {action.shortcut}
                            </kbd>
                          )}
                        </div>
                        <p className="text-[10.5px] text-stone-500 line-clamp-1 leading-tight mt-0.5">
                          {!isEnabled && action.unsupportedReason
                            ? action.unsupportedReason
                            : action.description}
                        </p>
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * Lightweight Workset Item Row with quick item actions and secondary menu
 */
interface WorksetPreviewRowProps {
  item: LauncherPreviewItem;
  contextId: string;
  contextState?: string;
  isExecuting?: boolean;
  onExecuteItemAction?: (actionId: ItemActionId, contextId: string, itemId: string) => void;
}

function WorksetPreviewRow({
  item,
  contextId,
  contextState,
  isExecuting = false,
  onExecuteItemAction,
}: WorksetPreviewRowProps) {
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const [focusedMenuIndex, setFocusedMenuIndex] = useState(-1);
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const itemMenuItemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const isLink = item.itemType === 'url' || item.itemType === 'link';
  const isImportant = Boolean(item.isImportant);

  const itemActions = useMemo(() => {
    return getAvailableItemActions({
      contextId,
      itemId: item.id,
      isImportant,
    });
  }, [contextId, item.id, isImportant]);

  // Close item menu when contextId changes
  useEffect(() => {
    setIsMenuOpen(false);
    setFocusedMenuIndex(-1);
  }, [contextId]);

  // Click outside handling for item menu
  useEffect(() => {
    if (!isMenuOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setIsMenuOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isMenuOpen]);

  // Focus management when item menu opens
  useEffect(() => {
    if (isMenuOpen) {
      const firstEnabledIdx = itemActions.findIndex((a) => a.isSupported);
      const targetIdx = firstEnabledIdx >= 0 ? firstEnabledIdx : 0;
      setFocusedMenuIndex(targetIdx);
      setTimeout(() => {
        itemMenuItemRefs.current[targetIdx]?.focus();
      }, 30);
    } else {
      setFocusedMenuIndex(-1);
    }
  }, [isMenuOpen, itemActions]);

  const handleMenuKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      e.stopPropagation();
      setFocusedMenuIndex((prev) => {
        const next = (prev + 1) % itemActions.length;
        itemMenuItemRefs.current[next]?.focus();
        return next;
      });
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      e.stopPropagation();
      setFocusedMenuIndex((prev) => {
        const next = (prev - 1 + itemActions.length) % itemActions.length;
        itemMenuItemRefs.current[next]?.focus();
        return next;
      });
    } else if (e.key === 'Home') {
      e.preventDefault();
      e.stopPropagation();
      setFocusedMenuIndex(0);
      itemMenuItemRefs.current[0]?.focus();
    } else if (e.key === 'End') {
      e.preventDefault();
      e.stopPropagation();
      const last = itemActions.length - 1;
      setFocusedMenuIndex(last);
      itemMenuItemRefs.current[last]?.focus();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      setIsMenuOpen(false);
      triggerRef.current?.focus();
    } else if (e.key === 'Tab') {
      setIsMenuOpen(false);
    }
  };

  return (
    <div className="group relative flex items-center justify-between py-1 px-1.5 rounded hover:bg-white/[0.04] text-xs gap-2 text-stone-300 transition-colors">
      <div className="flex items-center gap-1.5 min-w-0 flex-1">
        {isLink ? (
          <Link2 size={12} className="text-stone-400 shrink-0" />
        ) : (
          <FileText size={12} className="text-stone-400 shrink-0" />
        )}
        <span className="truncate text-[11px]">{item.title}</span>
        {isImportant && (
          <span
            className="text-[9.5px] px-1 py-0.2 rounded bg-amber-500/10 text-amber-400 font-medium shrink-0"
            title="Working set memory retention in this context. Does not affect parent context status."
          >
            Pinned Memory
          </span>
        )}
      </div>

      <div className="flex items-center gap-1 shrink-0 relative" ref={menuRef}>
        {item.appName && (
          <span className="text-[10px] text-stone-500 font-mono mr-1">
            {item.appName}
          </span>
        )}

        {/* Quick Shortcut: Toggle Importance */}
        {onExecuteItemAction && (
          <button
            type="button"
            disabled={isExecuting}
            onClick={(e) => {
              e.stopPropagation();
              onExecuteItemAction(
                isImportant ? 'item.unmark_important' : 'item.mark_important',
                contextId,
                item.id
              );
            }}
            className={`p-1 rounded hover:bg-white/10 transition-all cursor-pointer ${
              isImportant
                ? 'opacity-100 text-amber-400'
                : 'opacity-0 group-hover:opacity-100 text-stone-500 hover:text-amber-400'
            }`}
            title={
              isImportant
                ? 'Pinned in context working set memory. Click to unpin. (Does not affect context status)'
                : 'Pin as important working set memory in this context. (Does not affect context status)'
            }
            aria-label={
              isImportant
                ? 'Unpin item from context memory'
                : 'Pin item in context memory'
            }
          >
            <Star size={11} className={isImportant ? 'fill-current' : ''} />
          </button>
        )}

        {/* Quick Shortcut: Remove from Context */}
        {onExecuteItemAction && (
          <button
            type="button"
            disabled={isExecuting}
            onClick={(e) => {
              e.stopPropagation();
              onExecuteItemAction('item.remove_from_context', contextId, item.id);
            }}
            className="p-1 rounded hover:bg-white/10 opacity-0 group-hover:opacity-100 text-stone-500 hover:text-red-400 transition-all cursor-pointer"
            title="Remove item from this context workset without deleting the item"
            aria-label="Remove item from context"
          >
            <X size={11} />
          </button>
        )}

        {/* Item Secondary Actions Menu Trigger */}
        {onExecuteItemAction && (
          <button
            id={`launcher-item-menu-btn-${item.id}`}
            ref={triggerRef}
            data-menu-trigger="true"
            type="button"
            disabled={isExecuting}
            onClick={(e) => {
              e.stopPropagation();
              setIsMenuOpen((prev) => !prev);
            }}
            className={`p-1 rounded hover:bg-white/10 transition-all cursor-pointer ${
              isMenuOpen
                ? 'opacity-100 bg-white/10 text-stone-100'
                : 'opacity-0 group-hover:opacity-100 text-stone-500 hover:text-stone-200'
            }`}
            title="More item actions"
            aria-label={`More actions for item ${item.title}`}
            aria-haspopup="menu"
            aria-expanded={isMenuOpen}
            aria-controls={`launcher-item-menu-${item.id}`}
          >
            <MoreHorizontal size={11} />
          </button>
        )}

        {/* Compact Item Secondary Actions Popover */}
        {isMenuOpen && onExecuteItemAction && (
          <div
            id={`launcher-item-menu-${item.id}`}
            role="menu"
            aria-label={`Item actions for ${item.title}`}
            onKeyDown={handleMenuKeyDown}
            tabIndex={-1}
            className="absolute right-0 bottom-full mb-1 w-64 bg-stone-900 border border-white/10 rounded-lg shadow-2xl p-1.5 z-40 animate-in fade-in zoom-in-95 duration-100 focus:outline-none"
          >
            <div className="px-2 py-1 border-b border-white/5 mb-1">
              <div className="flex items-center justify-between gap-1">
                <span className="text-[10.5px] font-medium text-stone-300 truncate">
                  {item.title}
                </span>
                <span className="text-[9.5px] px-1.5 py-0.2 rounded bg-white/5 text-stone-400">
                  {isImportant ? 'Pinned Memory' : 'Context Item'}
                </span>
              </div>
              <p className="text-[9.5px] text-stone-500 mt-0.5">
                Item memory status does not affect parent context status ({contextState || 'active'}).
              </p>
            </div>

            <div className="space-y-0.5">
              {itemActions.map((action, idx) => {
                const isEnabled = action.isSupported;
                const isFocused = focusedMenuIndex === idx;
                return (
                  <button
                    key={action.id}
                    ref={(el) => {
                      itemMenuItemRefs.current[idx] = el;
                    }}
                    role="menuitem"
                    type="button"
                    tabIndex={isFocused ? 0 : -1}
                    disabled={!isEnabled || isExecuting}
                    aria-disabled={!isEnabled || isExecuting}
                    onClick={(e) => {
                      e.stopPropagation();
                      if (!isEnabled || isExecuting) return;
                      setIsMenuOpen(false);
                      onExecuteItemAction(action.id, contextId, item.id);
                    }}
                    title={!isEnabled ? action.unsupportedReason : action.description}
                    className={`w-full flex items-start gap-2.5 px-2 py-1.5 rounded-md text-xs text-left transition-colors ${
                      !isEnabled
                        ? 'opacity-40 cursor-not-allowed text-stone-500'
                        : isFocused
                        ? 'bg-white/10 text-stone-100 cursor-pointer'
                        : 'text-stone-300 hover:text-stone-100 hover:bg-white/10 cursor-pointer'
                    }`}
                  >
                    <div className="mt-0.5 shrink-0">
                      {(action.id === 'item.mark_important' || action.id === 'item.unmark_important') && (
                        <Star
                          size={12}
                          className={action.id === 'item.unmark_important' ? 'fill-amber-400 text-amber-400' : ''}
                        />
                      )}
                      {action.id === 'item.remove_from_context' && (
                        <X size={12} />
                      )}
                      {action.id === 'item.delete' && (
                        <Trash2 size={12} />
                      )}
                    </div>
                    <div className="flex-1 min-w-0">
                      <span className="truncate font-medium block">{action.label}</span>
                      <p className="text-[10px] text-stone-500 line-clamp-1 leading-tight mt-0.5">
                        {!isEnabled && action.unsupportedReason
                          ? action.unsupportedReason
                          : action.description}
                      </p>
                    </div>
                  </button>
                );
              })}
            </div>
          </div>
        )}
      </div>
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
