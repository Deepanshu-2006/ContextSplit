import { useEffect, useRef, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  Search,
  X,
  AlertCircle,
  RefreshCw,
  Play,
  FileText,
  FileCode,
  Link2,
  MoreHorizontal,
  Pause,
  Archive,
  Star,
  ExternalLink,
  Trash2,
  Pencil,
  Pin,
  RotateCcw,
  CheckSquare,
  Activity,
  FolderOpen,
} from 'lucide-react';
import { toast } from 'sonner';
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
  sortContextList,
  groupLauncherContexts,
  getVisibleLauncherSlice,
  ActionErrorInfo,
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
import {
  getDuplicateNameSet,
  resolveContextDisplayIdentity,
  formatGentleTime,
} from '../../models/contextIdentification';
import { validateContextName } from '../../models/contextOrganization';

const FILTER_OPTIONS: { id: ContextFilterType; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'active', label: 'Active' },
  { id: 'recent', label: 'Recent' },
  { id: 'paused', label: 'Paused' },
  { id: 'archived', label: 'Archived' },
];

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
    executingActionId,
    executingTargetId,
    resumeStatus,
    resumeError,
    actionError,
    notReadyReason,
    error,
    previewError,
    activeContext,
    recentContexts,
    isPreviewOpen,
    activeFilter,
    activeSort,
    setIsPreviewOpen,
    closeLauncher,
    setSearchQuery,
    setActiveFilter,
    setActiveSort,
    toggleActiveSort,
    setSelectedIndex,
    selectContextById,
    fetchLauncherData,
    resumeContextById,
    clearResumeState,
    clearActionError,
    executeContextAction,
    executeItemAction,
  } = useLauncherStore();

  const { activeWorkspaceId } = useWorkspaceStore();

  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const modalRef = useRef<HTMLDivElement>(null);

  // Focus trap inside the modal dialog
  const handleModalKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Tab') {
      const focusable = modalRef.current?.querySelectorAll<HTMLElement>(
        'input:not([disabled]), button:not([disabled]):not([aria-disabled="true"]):not([tabindex="-1"]), [role="button"]:not([tabindex="-1"]), [tabindex="0"]'
      );
      if (!focusable || focusable.length === 0) return;

      const firstEl = focusable[0];
      const lastEl = focusable[focusable.length - 1];

      if (e.shiftKey) {
        if (document.activeElement === firstEl) {
          e.preventDefault();
          lastEl.focus();
        }
      } else {
        if (document.activeElement === lastEl) {
          e.preventDefault();
          firstEl.focus();
        }
      }
    }
  };

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


  // Filter, sort, and rank contexts based on search query, active lifecycle filter, and sort order
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

    // 2. If no search query, sort deterministically according to activeSort (recent or name)
    // Pinned contexts are always elevated at the top without duplication
    if (!searchQuery.trim()) {
      return sortContextList(stateFiltered, activeSort);
    }

    // 3. Match search query terms against structured metadata within state-filtered contexts
    const matched = stateFiltered.filter((ctx) => matchContext(ctx, searchQuery));
    return sortContextMatches(matched, searchQuery, activeSort);
  }, [activeContext, recentContexts, activeFilter, searchQuery, activeSort]);

  // Detect duplicate names across all available contexts for deterministic disambiguation
  const duplicateNamesSet = useMemo(() => {
    const allContexts = [
      ...(activeContext ? [activeContext] : []),
      ...recentContexts,
    ];
    return getDuplicateNameSet(allContexts);
  }, [activeContext, recentContexts]);

  // Windowed visible slice for DOM rendering performance:
  // Guarantees rendering cost stays < 2ms during fast typing even with 5,000+ contexts
  // Dynamically expands if user navigates selection via keyboard past initial visible threshold
  const { visibleItems: visibleFilteredList, hasMore: hasMoreFiltered } = useMemo(() => {
    return getVisibleLauncherSlice(filteredList, selectedIndex);
  }, [filteredList, selectedIndex]);

  // Compute structured groups grounded in actual data for organized presentation
  const contextGroups = useMemo(() => {
    return groupLauncherContexts(visibleFilteredList, {
      searchQuery,
      activeFilter,
      activeSort,
      activeContextId: activeContext?.id,
    });
  }, [visibleFilteredList, searchQuery, activeFilter, activeSort, activeContext?.id]);

  // Preserve stable selection: when filteredList order changes (e.g. sort toggled),
  // ensure selectedIndex follows the currently selectedContextId
  useEffect(() => {
    if (filteredList.length === 0) {
      if (selectedContextId !== null) {
        useLauncherStore.setState({ selectedContextId: null, activePreview: null });
      }
    } else if (selectedContextId) {
      const foundIdx = filteredList.findIndex((item) => item.id === selectedContextId);
      if (foundIdx >= 0) {
        if (foundIdx !== selectedIndex) {
          setSelectedIndex(foundIdx);
        }
      } else {
        // Selected context is no longer in filtered list (e.g. filtered out by lifecycle filter)
        setSelectedIndex(0);
        selectContextById(filteredList[0].id);
      }
    } else {
      setSelectedIndex(0);
      selectContextById(filteredList[0].id);
    }
  }, [filteredList, selectedContextId, selectContextById, setSelectedIndex]);

  // Mac-native Keyboard Navigation with two-stage inspection & resume
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      const activeEl = document.activeElement as HTMLElement | null;

      // If focus is currently inside an open menu, let the menu manage its own keyboard navigation
      if (activeEl?.closest('[role="menu"]')) {
        return;
      }

      // 1. Escape: dismiss error/prompt -> close preview -> clear search -> clear filter -> close launcher
      if (e.key === 'Escape') {
        e.preventDefault();
        if (actionError || notReadyReason || resumeError) {
          clearActionError();
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
        // If focused in interactive controls, do not hijack Home
        if (activeEl?.tagName === 'BUTTON' || activeEl?.closest('[data-menu-trigger="true"]')) {
          return;
        }
        e.preventDefault();
        if (filteredList.length > 0) {
          setSelectedIndex(0);
          selectContextById(filteredList[0].id);
        }
        return;
      }

      if (
        e.key === 'End' ||
        ((e.metaKey || e.ctrlKey) && e.key === 'ArrowDown')
      ) {
        // If focused in interactive controls, do not hijack End
        if (activeEl?.tagName === 'BUTTON' || activeEl?.closest('[data-menu-trigger="true"]')) {
          return;
        }
        e.preventDefault();
        if (filteredList.length > 0) {
          const lastIdx = filteredList.length - 1;
          setSelectedIndex(lastIdx);
          selectContextById(filteredList[lastIdx].id);
        }
        return;
      }

      // 4. Sequential Navigation: ArrowDown / Ctrl+N / ArrowUp / Ctrl+P
      const isNext = e.key === 'ArrowDown' || (e.ctrlKey && e.key.toLowerCase() === 'n');
      const isPrev = e.key === 'ArrowUp' || (e.ctrlKey && e.key.toLowerCase() === 'p');

      if (isNext || isPrev) {
        // If focused inside preview action buttons, menu triggers, or workset item rows, let the element handle arrow navigation
        if (
          activeEl?.closest('[data-menu-trigger="true"]') ||
          activeEl?.id === 'launcher-primary-action-btn' ||
          activeEl?.closest('[role="menu"]') ||
          activeEl?.closest('[data-workset-row="true"]')
        ) {
          return;
        }

        if (isNext) {
          e.preventDefault();
          if (filteredList.length > 0) {
            const nextIndex = (selectedIndex + 1) % filteredList.length;
            setSelectedIndex(nextIndex);
            const target = filteredList[nextIndex];
            if (target) {
              selectContextById(target.id);
            }
          }
          return;
        }

        if (isPrev) {
          e.preventDefault();
          if (filteredList.length > 0) {
            const prevIndex = (selectedIndex - 1 + filteredList.length) % filteredList.length;
            setSelectedIndex(prevIndex);
            const target = filteredList[prevIndex];
            if (target) {
              selectContextById(target.id);
            }
          }
          return;
        }
      }

      // 5. Context Quick Actions Shortcuts (disabled during text editing):
      const isTextInput = activeEl?.tagName === 'INPUT' || activeEl?.tagName === 'TEXTAREA';
      if (!isTextInput) {
        // Cmd+O / Ctrl+O: Open Context Details
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'o') {
          e.preventDefault();
          const targetId = selectedContextId || filteredList[selectedIndex]?.id;
          if (targetId) {
            executeContextAction('context.open_details', targetId);
          }
          return;
        }

        // Alt+P / Option+P: Toggle Pin on Selected Context
        if (e.altKey && e.key.toLowerCase() === 'p') {
          e.preventDefault();
          const targetId = selectedContextId || filteredList[selectedIndex]?.id;
          if (targetId) {
            const isCurrentlyPinned = Boolean(
              activePreview?.contextId === targetId
                ? activePreview.isPinned
                : filteredList.find((c) => c.id === targetId)?.isPinned
            );
            executeContextAction(isCurrentlyPinned ? 'context.unpin' : 'context.pin', targetId);
          }
          return;
        }

        // Alt+M / Option+M: Toggle Secondary Actions Menu for selected context
        if (e.altKey && e.key.toLowerCase() === 'm') {
          e.preventDefault();
          const moreActionsBtn = document.getElementById('launcher-more-actions-btn');
          if (moreActionsBtn) {
            moreActionsBtn.click();
            moreActionsBtn.focus();
          }
          return;
        }

        // Alt+R / Option+R: Rename Selected Context
        if (e.altKey && e.key.toLowerCase() === 'r') {
          e.preventDefault();
          const renameBtn = document.getElementById('launcher-rename-context-btn');
          if (renameBtn) {
            renameBtn.click();
          }
          return;
        }

        // Alt+S / Option+S: Toggle Context Sort Order (recent <-> name)
        if (e.altKey && e.key.toLowerCase() === 's') {
          e.preventDefault();
          toggleActiveSort();
          return;
        }

        // Alt+A / Option+A: Archive or Restore Selected Context
        if (e.altKey && e.key.toLowerCase() === 'a') {
          e.preventDefault();
          const targetId = selectedContextId || filteredList[selectedIndex]?.id;
          if (targetId) {
            const targetContext = filteredList.find((c) => c.id === targetId);
            const currentState = resolveContextLifecycleState({
              lifecycleState: targetContext?.state,
              isActive: targetContext?.isActive,
            });
            const actionToRun = currentState === 'archived' ? 'context.restore' : 'context.archive';
            executeContextAction(actionToRun, targetId);
          }
          return;
        }
      }

      // 6. Enter Action: Resume from preview or list
      if (e.key === 'Enter') {
        // If an active button, menu item, or workset row is focused, let the focused element handle its own native/custom click.
        // This avoids accidental duplicate executions or executing context.resume when activating a menu trigger or button.
        if (
          activeEl?.tagName === 'BUTTON' ||
          activeEl?.closest('[role="menu"]') ||
          activeEl?.closest('[data-menu-trigger="true"]') ||
          activeEl?.closest('[data-workset-row="true"]')
        ) {
          return;
        }

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

        const target = filteredList[selectedIndex];
        if (target) {
          setIsPreviewOpen(true);
          executeContextAction('context.resume', target.id);
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
    actionError,
    searchQuery,
    activeFilter,
    closeLauncher,
    setSearchQuery,
    setActiveFilter,
    setSelectedIndex,
    setIsPreviewOpen,
    resumeContextById,
    clearResumeState,
    clearActionError,
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
        ref={modalRef}
        onKeyDown={handleModalKeyDown}
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
              className="p-1 rounded text-stone-400 hover:text-white active:text-stone-300 transition-colors duration-75 cursor-pointer mr-1.5 focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20 active:scale-95"
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

        {/* Filter Tabs & Minimal Sort Segmented Control */}
        <div className="flex items-center justify-between px-3.5 py-1.5 border-b border-white/5 bg-stone-950/20 text-xs gap-2">
          <div
            role="tablist"
            aria-label="Filter contexts by lifecycle state"
            className="flex items-center gap-1 overflow-x-auto no-scrollbar py-0.5"
          >
            {FILTER_OPTIONS.map((f) => {
              const isSelected = activeFilter === f.id;
              return (
                <button
                  key={f.id}
                  role="tab"
                  id={`launcher-filter-tab-${f.id}`}
                  aria-selected={isSelected}
                  tabIndex={isSelected ? 0 : -1}
                  onClick={() => {
                    setActiveFilter(f.id);
                    inputRef.current?.focus();
                  }}
                  className={`px-2.5 py-1 rounded-md text-xs transition-all duration-75 shrink-0 focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20 active:bg-white/10 select-none ${
                    isSelected
                      ? 'bg-white/10 text-stone-100 font-medium shadow-xs cursor-default'
                      : 'text-stone-400 hover:text-stone-200 hover:bg-white/[0.04] cursor-pointer'
                  }`}
                >
                  {f.label}
                </button>
              );
            })}
          </div>

          {/* Minimal Native-Feel Sort Segmented Control */}
          <div
            className="flex items-center shrink-0"
            role="group"
            aria-label="Sort contexts"
          >
            <div className="flex items-center p-0.5 rounded-md bg-stone-900/80 border border-white/5">
              <button
                id="launcher-sort-recent-btn"
                type="button"
                aria-pressed={activeSort === 'recent'}
                onClick={() => {
                  setActiveSort('recent');
                  inputRef.current?.focus();
                }}
                className={`px-2 py-0.5 rounded text-[11px] transition-all duration-75 select-none focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20 active:bg-white/10 ${
                  activeSort === 'recent'
                    ? 'bg-white/10 text-stone-100 font-medium shadow-xs cursor-default'
                    : 'text-stone-400 hover:text-stone-200 hover:bg-white/[0.04] cursor-pointer'
                }`}
                title="Sort by recent activity (⌥S)"
              >
                Recent
              </button>
              <button
                id="launcher-sort-name-btn"
                type="button"
                aria-pressed={activeSort === 'name'}
                onClick={() => {
                  setActiveSort('name');
                  inputRef.current?.focus();
                }}
                className={`px-2 py-0.5 rounded text-[11px] transition-all duration-75 select-none focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20 active:bg-white/10 ${
                  activeSort === 'name'
                    ? 'bg-white/10 text-stone-100 font-medium shadow-xs cursor-default'
                    : 'text-stone-400 hover:text-stone-200 hover:bg-white/[0.04] cursor-pointer'
                }`}
                title="Sort alphabetically A-Z (⌥S)"
              >
                Name
              </button>
            </div>
          </div>
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
                    className="mt-2 inline-flex items-center gap-1 px-2 py-0.5 rounded bg-red-900/40 hover:bg-red-900/60 active:bg-red-900/80 text-[11px] text-red-200 transition-colors duration-75 cursor-pointer focus:outline-none focus-visible:ring-1 focus-visible:ring-red-400 active:scale-95"
                  >
                    <RefreshCw size={10} />
                    Retry
                  </button>
                </div>
              </div>
            )}

            {/* Empty state */}
            {!isLoading && !error && filteredList.length === 0 && (
              <div className="py-8 text-center space-y-1">
                <p className="text-xs font-medium text-stone-300">
                  {searchQuery ? 'No matching contexts' : 'No contexts available'}
                </p>
                <p className="text-xs text-stone-500">
                  {searchQuery
                    ? 'Try a different name or keyword.'
                    : activeFilter !== 'all'
                    ? `No contexts in the ${activeFilter} state.`
                    : 'Create or activate a context to get started.'}
                </p>
              </div>
            )}

            {/* Context Groups List */}
            {!isLoading && !error && filteredList.length > 0 && (
              <div className="space-y-3">
                {contextGroups.map((group) => (
                  <div key={group.id} className="space-y-1">
                    <div className="px-2 py-1 text-[10px] font-semibold tracking-wider uppercase text-stone-400 flex items-center justify-between select-none">
                      <div className="flex items-center gap-1.5">
                        {group.id === 'pinned' && (
                          <Pin className="w-2.5 h-2.5 text-amber-400 fill-amber-400/20 shrink-0" />
                        )}
                        {group.id === 'active' && (
                          <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 shrink-0 shadow-[0_0_6px_rgba(52,211,153,0.4)]" />
                        )}
                        <span>{group.title}</span>
                      </div>
                      {group.id !== 'results' && (
                        <span className="text-[10px] font-mono text-stone-500">
                          {group.items.length}
                        </span>
                      )}
                    </div>

                    <div className="space-y-0.5">
                      {group.items.map((item, relIdx) => {
                        const globalIdx = group.startIndex + relIdx;
                        const isSelected = selectedIndex === globalIdx;
                        return (
                          <ContextLauncherItem
                            key={item.id}
                            item={item}
                            isSelected={isSelected}
                            isPreviewOpen={isSelected && isPreviewOpen}
                            isResuming={isResuming && selectedContextId === item.id}
                            isDuplicate={duplicateNamesSet.has(item.name.trim().toLowerCase())}
                            dataIndex={globalIdx}
                            onSelect={() => {
                              setSelectedIndex(globalIdx);
                              selectContextById(item.id);
                              setIsPreviewOpen(true);
                            }}
                            onResume={() => resumeContextById(item.id)}
                          />
                        );
                      })}
                    </div>
                  </div>
                ))}

                {hasMoreFiltered && (
                  <div className="px-3 py-1.5 text-center text-[10.5px] text-stone-500">
                    Showing top {visibleFilteredList.length} of {filteredList.length} contexts
                  </div>
                )}
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
              preview={activePreview?.contextId === selectedContextId ? activePreview : null}
              isLoading={isLoadingPreview}
              isResuming={isResuming}
              isActionExecuting={isActionExecuting}
              executingActionId={executingActionId}
              executingTargetId={executingTargetId}
              isPreviewOpen={isPreviewOpen}
              resumeStatus={resumeStatus}
              resumeError={resumeError}
              actionError={actionError}
              notReadyReason={notReadyReason}
              error={previewError}
              isActiveContext={Boolean(
                activeContext &&
                  activePreview &&
                  activePreview.contextId === activeContext.id &&
                  activePreview.state !== 'archived' &&
                  activePreview.state !== 'inactive'
              )}
              isDuplicate={Boolean(
                activePreview && duplicateNamesSet.has(activePreview.name.trim().toLowerCase())
              )}
              onResume={(force) => {
                if (activePreview) {
                  executeContextAction('context.resume', activePreview.contextId, { force });
                }
              }}
              onExecuteContextAction={(actionId, options) => {
                if (activePreview) {
                  return executeContextAction(actionId, activePreview.contextId, options);
                }
              }}
              onExecuteItemAction={(actionId, contextId, itemId) => {
                executeItemAction(actionId, contextId, itemId);
              }}
              onClearResumeState={clearResumeState}
              onClearActionError={clearActionError}
              onClose={() => {
                setIsPreviewOpen(false);
                inputRef.current?.focus();
              }}
            />
          </div>
        </div>

        {/* Footer: Minimal 1-line shortcuts */}
        <div className="px-4 py-2 bg-stone-950/60 border-t border-white/5 flex items-center justify-between text-[11px] text-stone-500 select-none">
          <span className="font-medium text-stone-400">ContextSplit</span>
          <div className="flex items-center gap-2.5 sm:gap-3 flex-wrap">
            <span><kbd className="font-mono text-[10px] text-stone-400">↑↓</kbd> Navigate</span>
            <span><kbd className="font-mono text-[10px] text-stone-400">↵</kbd> Resume</span>
            <span><kbd className="font-mono text-[10px] text-stone-400">⌥S</kbd> Sort</span>
            <span className="hidden sm:inline"><kbd className="font-mono text-[10px] text-stone-400">⌥P</kbd> Pin</span>
            <span className="hidden sm:inline"><kbd className="font-mono text-[10px] text-stone-400">⌥R</kbd> Rename</span>
            <span className="hidden md:inline"><kbd className="font-mono text-[10px] text-stone-400">⌥A</kbd> Archive</span>
            <span className="hidden lg:inline"><kbd className="font-mono text-[10px] text-stone-400">⌘O</kbd> Details</span>
            <span><kbd className="font-mono text-[10px] text-stone-400">⌥M</kbd> Actions</span>
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
  executingActionId?: ContextActionId | ItemActionId | null;
  executingTargetId?: string | null;
  isPreviewOpen?: boolean;
  resumeStatus: ResumeActionStatus;
  resumeError: string | null;
  actionError?: ActionErrorInfo | null;
  notReadyReason: string | null;
  error: string | null;
  isActiveContext?: boolean;
  isDuplicate?: boolean;
  onResume: (force?: boolean) => void;
  onExecuteContextAction?: (actionId: ContextActionId, options?: { force?: boolean; newName?: string }) => Promise<any> | any;
  onExecuteItemAction?: (actionId: ItemActionId, contextId: string, itemId: string) => void;
  onClearResumeState: () => void;
  onClearActionError?: () => void;
  onClose: () => void;
}

function ContextPreviewPane({
  preview,
  isLoading,
  isResuming,
  isActionExecuting = false,
  executingActionId,
  executingTargetId,
  resumeError,
  actionError,
  notReadyReason,
  error,
  isActiveContext = false,
  isDuplicate = false,
  onResume,
  onExecuteContextAction,
  onExecuteItemAction,
  onClearResumeState,
  onClearActionError,
}: ContextPreviewPaneProps) {
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const [focusedMenuIndex, setFocusedMenuIndex] = useState(-1);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const [renameValidationError, setRenameValidationError] = useState<string | null>(null);
  const [isSubmittingRename, setIsSubmittingRename] = useState(false);
  const renameInputRef = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuItemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  // Streamlined activity summary removing duplicate context names
  const cleanActivitySummary = useMemo(() => {
    if (!preview?.activitySummary) return null;
    const prefix = new RegExp(`^Active in\\s+${preview.name}\\s+with\\s+`, 'i');
    if (prefix.test(preview.activitySummary)) {
      const streamlined = preview.activitySummary.replace(prefix, '');
      return streamlined.charAt(0).toUpperCase() + streamlined.slice(1);
    }
    return preview.activitySummary;
  }, [preview?.activitySummary, preview?.name]);

  // Close menu and reset rename when selected context changes
  useEffect(() => {
    setIsMenuOpen(false);
    setFocusedMenuIndex(-1);
    setIsRenaming(false);
    setRenameValue('');
    setRenameValidationError(null);
    setIsSubmittingRename(false);
  }, [preview?.contextId]);

  const handleStartRename = () => {
    if (!preview) return;
    setIsMenuOpen(false);
    setIsRenaming(true);
    setRenameValue(preview.name);
    setRenameValidationError(null);
    setTimeout(() => {
      renameInputRef.current?.focus();
      renameInputRef.current?.select();
    }, 40);
  };

  const handleCancelRename = () => {
    setIsRenaming(false);
    setRenameValue('');
    setRenameValidationError(null);
    setIsSubmittingRename(false);
  };

  const handleRenameSubmit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    if (isSubmittingRename || isActionExecuting || !preview) return;

    const trimmed = renameValue.trim();
    const validation = validateContextName(trimmed);
    if (!validation.isValid) {
      setRenameValidationError(validation.error || 'Invalid name');
      renameInputRef.current?.focus();
      return;
    }

    if (trimmed === preview.name.trim()) {
      handleCancelRename();
      return;
    }

    setIsSubmittingRename(true);
    setRenameValidationError(null);

    try {
      if (onExecuteContextAction) {
        const res = await onExecuteContextAction('context.rename', { newName: trimmed });
        if (res && typeof res === 'object' && 'success' in res && !res.success) {
          setRenameValidationError(res.error || res.message || 'Failed to rename context');
          renameInputRef.current?.focus();
          return;
        }
      }
      setIsRenaming(false);
      setRenameValue('');
      setRenameValidationError(null);
    } catch (err) {
      const errMsg = err instanceof Error ? err.message : String(err);
      setRenameValidationError(errMsg);
      renameInputRef.current?.focus();
    } finally {
      setIsSubmittingRename(false);
    }
  };

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
      isPinned: preview.isPinned,
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
    if (secondaryActions.length === 0) {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        setIsMenuOpen(false);
        triggerRef.current?.focus();
      }
      return;
    }

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
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      e.stopPropagation();
      const targetAction = secondaryActions[focusedMenuIndex];
      if (targetAction && targetAction.isSupported && !isResuming && !isActionExecuting) {
        setIsMenuOpen(false);
        if (targetAction.id === 'context.rename') {
          handleStartRename();
        } else {
          if (onExecuteContextAction) {
            onExecuteContextAction(targetAction.id);
          }
          triggerRef.current?.focus();
        }
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      setIsMenuOpen(false);
      triggerRef.current?.focus();
    } else if (e.key === 'Tab') {
      setIsMenuOpen(false);
      triggerRef.current?.focus();
    }
  };

  const isPrimaryExecuting = Boolean(
    isResuming || (isActionExecuting && executingActionId === 'context.resume')
  );

  const primaryButtonLabel = isPrimaryExecuting
    ? primaryActionDescriptor.inProgressLabel
    : primaryActionDescriptor.label === 'Resume Context'
    ? 'Resume'
    : primaryActionDescriptor.label;

  const isContextSecondaryExecuting = Boolean(
    isActionExecuting &&
      preview &&
      executingTargetId === preview.contextId &&
      executingActionId &&
      executingActionId !== 'context.resume'
  );

  if (isLoading && !preview) {
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

  if (error && !preview) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-center p-4 space-y-2">
        <AlertCircle size={20} className="text-red-400" />
        <p className="text-xs font-semibold text-stone-200">Context Preview Unavailable</p>
        <p className="text-[11px] text-stone-400 max-w-xs">{error}</p>
      </div>
    );
  }

  if (!preview) {
    if (actionError) {
      return (
        <div className="h-full flex flex-col items-center justify-center text-center p-4 space-y-2">
          <AlertCircle size={20} className="text-red-400" />
          <p className="text-xs font-semibold text-stone-200">{actionError.title}</p>
          <p className="text-[11px] text-stone-400 max-w-xs">{actionError.message}</p>
          <button
            onClick={() => {
              onClearActionError?.();
              onClearResumeState();
            }}
            className="mt-2 px-3 py-1 rounded bg-white/10 hover:bg-white/15 text-stone-200 text-xs transition-colors cursor-pointer"
          >
            Dismiss
          </button>
        </div>
      );
    }
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
      <div className="space-y-3.5">
        {/* 4. Selected Context: Header */}
        <div className="border-b border-white/5 pb-3">
          {isRenaming ? (
            <form onSubmit={handleRenameSubmit} className="space-y-1.5" id="launcher-rename-context-form">
              <div className="flex items-center gap-1.5">
                <input
                  ref={renameInputRef}
                  id="launcher-rename-context-input"
                  type="text"
                  value={renameValue}
                  onChange={(e) => {
                    setRenameValue(e.target.value);
                    if (renameValidationError) setRenameValidationError(null);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Escape') {
                      e.preventDefault();
                      e.stopPropagation();
                      handleCancelRename();
                    }
                  }}
                  disabled={isSubmittingRename}
                  maxLength={100}
                  placeholder="Context name"
                  aria-label="Rename context"
                  className="flex-1 bg-stone-950 border border-white/15 focus:border-orange-500/80 focus:ring-1 focus:ring-orange-500/40 focus:outline-none rounded-md px-2.5 py-1 text-xs text-stone-100 placeholder-stone-500 font-medium transition-all"
                />
                <button
                  type="submit"
                  id="launcher-rename-save-btn"
                  disabled={isSubmittingRename || !renameValue.trim()}
                  className="px-2.5 py-1 bg-orange-500 hover:bg-orange-600 active:bg-orange-700 active:scale-[0.98] disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none text-stone-950 font-semibold text-xs rounded-md transition-all duration-75 cursor-pointer flex items-center gap-1 shrink-0 shadow-xs focus:outline-none focus-visible:ring-2 focus-visible:ring-orange-400"
                >
                  {isSubmittingRename ? (
                    <RefreshCw size={11} className="animate-spin" />
                  ) : null}
                  <span>Save</span>
                </button>
                <button
                  type="button"
                  id="launcher-rename-cancel-btn"
                  onClick={handleCancelRename}
                  disabled={isSubmittingRename}
                  className="px-2.5 py-1 bg-white/5 hover:bg-white/10 active:bg-white/15 active:scale-[0.98] text-stone-300 hover:text-stone-100 text-xs rounded-md transition-all duration-75 disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none cursor-pointer shrink-0 focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20"
                >
                  Cancel
                </button>
              </div>
              {renameValidationError && (
                <p id="launcher-rename-error-text" className="text-[11px] text-red-400 flex items-center gap-1">
                  <AlertCircle size={11} className="shrink-0" />
                  <span>{renameValidationError}</span>
                </p>
              )}
            </form>
          ) : (
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 group">
                  <h3 className="text-sm font-semibold text-stone-100 truncate">
                    {preview.name}
                  </h3>
                  <button
                    id="launcher-rename-context-btn"
                    type="button"
                    onClick={handleStartRename}
                    disabled={isResuming || isActionExecuting}
                    className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100 w-6 h-6 flex items-center justify-center text-stone-400 hover:text-stone-100 hover:bg-white/10 active:bg-white/20 active:scale-95 rounded-md transition-all duration-75 cursor-pointer focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20 disabled:opacity-40 disabled:pointer-events-none"
                    title="Rename Context (⌥R)"
                    aria-label="Rename Context"
                  >
                    <Pencil size={12} />
                  </button>
                  <button
                    id="launcher-pin-context-btn"
                    type="button"
                    onClick={() => {
                      if (onExecuteContextAction && preview) {
                        onExecuteContextAction(preview.isPinned ? 'context.unpin' : 'context.pin');
                      }
                    }}
                    disabled={isResuming || isActionExecuting}
                    className={`w-6 h-6 flex items-center justify-center rounded-md transition-all duration-75 cursor-pointer focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20 active:scale-95 disabled:opacity-40 disabled:pointer-events-none ${
                      preview.isPinned
                        ? 'text-amber-400 hover:text-amber-300 hover:bg-amber-400/10 active:bg-amber-400/20'
                        : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100 text-stone-400 hover:text-stone-100 hover:bg-white/10 active:bg-white/20'
                    }`}
                    title={preview.isPinned ? 'Unpin Context (⌥P)' : 'Pin Context (⌥P)'}
                    aria-label={preview.isPinned ? 'Unpin Context' : 'Pin Context'}
                    aria-pressed={preview.isPinned}
                  >
                    <Pin size={12} className={preview.isPinned ? 'fill-current' : ''} />
                  </button>
                  {isDuplicate && (
                    <span
                      className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-300/90 border border-amber-500/20 shrink-0"
                      title="Multiple contexts share this name"
                    >
                      Duplicate
                    </span>
                  )}
                </div>
              </div>
              {isArchived ? (
                <span className="text-[10px] font-medium px-2 py-0.5 rounded bg-stone-800 text-stone-400 border border-white/5 shrink-0">
                  Archived
                </span>
              ) : preview.state === 'paused' ? (
                <span className="text-[10px] font-medium px-2 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20 shrink-0">
                  Paused
                </span>
              ) : (
                <span
                  className={`text-[10px] font-medium px-2 py-0.5 rounded capitalize shrink-0 border ${
                    isReady
                      ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
                      : 'bg-amber-500/10 text-amber-400 border-amber-500/20'
                  }`}
                >
                  {preview.readinessBand.replace('_', ' ')}
                </span>
              )}
            </div>
          )}

          <div className="flex items-center gap-2 mt-1 text-[11px] text-stone-400 flex-wrap">
            {preview.projectIdentifier && (
              <>
                <span className="font-mono text-stone-300">
                  {preview.projectIdentifier}
                </span>
                <span className="text-stone-600">•</span>
              </>
            )}
            <span>{formatGentleTime(preview.lastActiveAt, preview.durationEstimate)}</span>
          </div>
        </div>

        {/* Context Description (when available and distinct from task) */}
        {preview.description && preview.description !== preview.taskTitle && (
          <p className="text-xs text-stone-400 line-clamp-2 leading-relaxed px-0.5">
            {preview.description}
          </p>
        )}

        {/* Next Action / Active Task (Requirement 6 - Genuine Data Only) */}
        {preview.taskTitle && (
          <div className="flex items-start gap-2 px-2.5 py-1.5 rounded-md bg-white/[0.03] border border-white/5 shadow-xs">
            <CheckSquare size={13} className="text-orange-400 shrink-0 mt-0.5" />
            <div className="min-w-0 flex-1">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-stone-500 mr-1.5">
                Task:
              </span>
              <span className="text-xs font-medium text-stone-200 line-clamp-2">
                {preview.taskTitle}
              </span>
            </div>
          </div>
        )}

        {/* Recent Activity (Requirement 5 - Genuine Data Only, without duplicate context label) */}
        {cleanActivitySummary && (
          <div className="flex items-center gap-2 px-1 text-[11px] text-stone-400">
            <Activity size={12} className="text-stone-500 shrink-0" />
            <span className="truncate flex-1">{cleanActivitySummary}</span>
          </div>
        )}

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
                className="px-2 py-0.5 rounded bg-amber-500 hover:bg-amber-600 active:bg-amber-700 active:scale-95 disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none text-stone-950 font-medium text-[11px] transition-all duration-75 cursor-pointer focus:outline-none focus-visible:ring-1 focus-visible:ring-amber-300"
              >
                {isResuming ? 'Resuming...' : 'Resume Anyway'}
              </button>
              <button
                onClick={onClearResumeState}
                disabled={isResuming || isActionExecuting}
                className="px-2 py-0.5 rounded bg-white/5 hover:bg-white/10 active:bg-white/15 active:scale-95 text-stone-300 text-[11px] transition-all duration-75 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20"
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {actionError ? (
          <div
            id="launcher-action-error-banner"
            role="alert"
            className="px-2.5 py-1.5 rounded-md bg-red-950/40 border border-red-500/20 text-red-200 text-xs flex items-center justify-between gap-2 shadow-xs"
          >
            <div className="flex items-center gap-2 min-w-0 flex-1">
              <AlertCircle className="w-3.5 h-3.5 text-red-400 shrink-0" />
              <div className="min-w-0 flex-1 flex items-baseline gap-1.5">
                <p className="font-semibold text-red-300 text-[11px] shrink-0">{actionError.title}</p>
                <p className="text-[11px] text-red-400/90 truncate">{actionError.message}</p>
              </div>
            </div>
            <div className="flex items-center gap-1.5 shrink-0">
              {actionError.isRetryable && (
                <button
                  onClick={() => {
                    const actionId = actionError.actionId;
                    const targetId = actionError.targetId;
                    onClearActionError?.();
                    onClearResumeState();
                    if (actionId.startsWith('item.') && targetId && preview) {
                      onExecuteItemAction?.(actionId as ItemActionId, preview.contextId, targetId);
                    } else if (actionId.startsWith('context.') && preview) {
                      onExecuteContextAction?.(actionId as ContextActionId);
                    }
                  }}
                  disabled={isResuming || isActionExecuting}
                  className="px-1.5 py-0.5 rounded bg-red-500/20 hover:bg-red-500/30 active:bg-red-500/40 active:scale-95 text-red-300 border border-red-500/30 font-medium text-[10px] transition-all duration-75 cursor-pointer flex items-center gap-1 disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none focus:outline-none focus-visible:ring-1 focus-visible:ring-red-400"
                >
                  <RefreshCw size={9} />
                  <span>Retry</span>
                </button>
              )}
              <button
                onClick={() => {
                  onClearActionError?.();
                  onClearResumeState();
                }}
                className="text-stone-400 hover:text-white active:text-stone-300 p-0.5 cursor-pointer rounded transition-colors duration-75 focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20 active:scale-95"
                aria-label="Dismiss error"
              >
                <X size={12} />
              </button>
            </div>
          </div>
        ) : resumeError ? (
          <div className="px-2.5 py-1.5 rounded-md bg-red-950/40 border border-red-500/20 text-red-200 flex items-center justify-between gap-2 text-xs">
            <div className="flex items-center gap-2 min-w-0 flex-1">
              <AlertCircle className="w-3.5 h-3.5 text-red-400 shrink-0" />
              <div className="min-w-0 flex-1 flex items-baseline gap-1.5">
                <p className="font-semibold text-red-300 text-[11px] shrink-0">Resume Failed</p>
                <p className="text-[11px] text-red-400/90 truncate">{resumeError}</p>
              </div>
            </div>
            <button
              onClick={onClearResumeState}
              className="text-stone-400 hover:text-white active:text-stone-300 p-0.5 cursor-pointer rounded transition-colors duration-75 focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20 active:scale-95"
              aria-label="Dismiss error"
            >
              <X size={12} />
            </button>
          </div>
        ) : null}

        {error && (
          <div
            id="launcher-refresh-error-banner"
            data-testid="workset-refresh-error-banner"
            role="alert"
            className="px-2.5 py-1.5 rounded-md bg-amber-950/30 border border-amber-500/20 text-amber-200 flex items-center justify-between gap-2 text-xs"
          >
            <div className="flex items-center gap-2 min-w-0 flex-1">
              <AlertCircle className="w-3.5 h-3.5 text-amber-400 shrink-0" />
              <div className="min-w-0 flex-1 flex items-baseline gap-1.5">
                <p className="font-semibold text-amber-300 text-[11px] shrink-0">Notice</p>
                <p className="text-[11px] text-amber-400/90 truncate">{error}</p>
              </div>
            </div>
            <button
              onClick={() => {
                useLauncherStore.setState({ previewError: null });
              }}
              className="text-stone-400 hover:text-white active:text-stone-300 p-0.5 cursor-pointer rounded transition-colors duration-75 focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20 active:scale-95"
              aria-label="Dismiss error"
            >
              <X size={12} />
            </button>
          </div>
        )}

        {/* Working Set Section (Requirements 2, 3, 4, 7, 8, 9, 10, 11) */}
        <div className="space-y-1.5 flex-1 min-h-0">
          <div className="flex items-center justify-between text-[11px] font-medium text-stone-400">
            <div className="flex items-center gap-1.5">
              <span>Working Set</span>
              {isLoading && (
                <RefreshCw
                  size={10}
                  className="animate-spin text-stone-500"
                  data-testid="workset-loading-indicator"
                />
              )}
            </div>
            <span className="text-[10px] text-stone-500 font-mono">
              {preview.itemCount} {preview.itemCount === 1 ? 'item' : 'items'}
            </span>
          </div>

          {preview.worksetItems.length === 0 ? (
            <div className="py-4 px-3 text-center rounded-lg border border-dashed border-white/10 bg-white/[0.01] flex flex-col items-center justify-center gap-1.5">
              <FolderOpen size={16} className="text-stone-600 mb-0.5" />
              <p className="text-xs font-medium text-stone-300">No items in working set</p>
              <p className="text-[11px] text-stone-500 max-w-[260px] leading-relaxed">
                Resume this workspace to begin capturing files, web links, or notes.
              </p>
            </div>
          ) : (
            <div className="space-y-1 pr-0.5" data-testid="workset-items-list">
              {preview.worksetItems.slice(0, 5).map((item) => (
                <WorksetPreviewRow
                  key={item.id}
                  item={item}
                  contextId={preview.contextId}
                  contextState={preview.state}
                  isExecuting={isActionExecuting}
                  executingActionId={executingActionId}
                  executingTargetId={executingTargetId}
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
          onKeyDown={(e) => {
            if (e.key === 'ArrowRight') {
              e.preventDefault();
              triggerRef.current?.focus();
            }
          }}
          disabled={isResuming || isActionExecuting}
          className="flex-1 py-2.5 px-4 rounded-lg bg-orange-500 hover:bg-orange-600 active:bg-orange-700 active:scale-[0.99] disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none text-stone-950 font-semibold text-xs transition-all duration-75 flex items-center justify-center gap-2 cursor-pointer shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-orange-400 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-900"
          aria-label={primaryButtonLabel}
        >
          {isPrimaryExecuting ? (
            <>
              <RefreshCw size={13} className="animate-spin" />
              <span>{primaryButtonLabel}</span>
            </>
          ) : (
            <>
              <Play size={13} className="fill-current" />
              <span>{primaryButtonLabel}</span>
              <kbd className="ml-1 px-1.5 py-0.5 text-[10px] font-mono bg-stone-950/20 text-stone-950 rounded leading-none font-semibold">
                ↵
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
            onKeyDown={(e) => {
              if (e.key === 'ArrowLeft') {
                e.preventDefault();
                document.getElementById('launcher-primary-action-btn')?.focus();
              } else if (e.key === 'ArrowDown' || e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                e.stopPropagation();
                setIsMenuOpen(true);
              }
            }}
            disabled={isResuming || isActionExecuting}
            className={`p-2.5 rounded-lg border border-white/10 transition-all duration-75 cursor-pointer flex items-center justify-center focus:outline-none focus-visible:ring-2 focus-visible:ring-orange-400/70 focus-visible:ring-offset-1 focus-visible:ring-offset-stone-900 active:scale-[0.98] disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none ${
              isMenuOpen
                ? 'bg-stone-800 text-stone-100 ring-1 ring-white/20'
                : 'bg-stone-900/60 hover:bg-stone-800 active:bg-stone-800/80 text-stone-400 hover:text-stone-200'
            }`}
            title={isContextSecondaryExecuting ? 'Executing action...' : 'More context actions (⌥M)'}
            aria-label={isContextSecondaryExecuting ? 'Executing action' : 'More actions for selected context'}
            aria-haspopup="menu"
            aria-expanded={isMenuOpen}
            aria-controls="launcher-context-secondary-menu"
          >
            {isContextSecondaryExecuting ? (
              <RefreshCw size={14} className="animate-spin text-stone-200" />
            ) : (
              <MoreHorizontal size={14} />
            )}
          </button>

          {/* Compact Popover Menu */}
          {isMenuOpen && (
            <div
              id="launcher-context-secondary-menu"
              role="menu"
              aria-label="Context secondary actions"
              onKeyDown={handleMenuKeyDown}
              tabIndex={-1}
              className="absolute bottom-full mb-1.5 right-0 w-60 bg-stone-900/95 backdrop-blur-md border border-white/10 rounded-lg shadow-2xl p-1 z-30 animate-in fade-in zoom-in-95 duration-100 focus:outline-none"
            >
              <div className="px-2.5 py-1 text-[10px] font-medium text-stone-500 uppercase tracking-wider">
                Context Actions
              </div>
              {secondaryActions.length === 0 ? (
                <div className="px-3 py-2 text-[11px] text-stone-500 italic text-center">
                  No secondary actions available
                </div>
              ) : (
                <div className="space-y-0.5">
                  {secondaryActions.map((action, idx) => {
                    const isEnabled = action.isSupported;
                    const isFocused = focusedMenuIndex === idx;
                    const isThisActionExecuting =
                      isActionExecuting && executingActionId === action.id;
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
                          if (action.id === 'context.rename') {
                            handleStartRename();
                            return;
                          }
                          if (onExecuteContextAction) {
                            onExecuteContextAction(action.id);
                          }
                          triggerRef.current?.focus();
                        }}
                        title={!isEnabled ? action.unsupportedReason : action.description}
                        className={`w-full flex items-center justify-between gap-2.5 px-2.5 py-1.5 rounded-md text-xs text-left transition-colors duration-75 focus:outline-none ${
                          !isEnabled
                            ? isFocused
                              ? 'opacity-40 cursor-not-allowed text-stone-500 bg-white/5 ring-1 ring-white/10 pointer-events-none'
                              : 'opacity-40 cursor-not-allowed text-stone-500 pointer-events-none'
                            : isFocused
                            ? 'bg-white/10 text-stone-100 cursor-pointer ring-1 ring-white/20'
                            : 'text-stone-300 hover:text-stone-100 hover:bg-white/10 active:bg-white/15 cursor-pointer'
                        }`}
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <span className="shrink-0 text-stone-400">
                            {isThisActionExecuting ? (
                              <RefreshCw size={13} className="animate-spin text-stone-200" />
                            ) : (
                              <>
                                {action.id === 'context.rename' && (
                                  <Pencil size={13} />
                                )}
                                {(action.id === 'context.pin' || action.id === 'context.unpin') && (
                                  <Pin
                                    size={13}
                                    className={action.id === 'context.unpin' ? 'fill-current text-amber-400' : ''}
                                  />
                                )}
                                {action.id === 'context.open_details' && (
                                  <ExternalLink size={13} />
                                )}
                                {action.id === 'context.pause' && (
                                  <Pause size={13} />
                                )}
                                {action.id === 'context.archive' && (
                                  <Archive size={13} />
                                )}
                                {action.id === 'context.restore' && (
                                  <RotateCcw size={13} />
                                )}
                                {action.id === 'context.delete' && (
                                  <Trash2 size={13} />
                                )}
                              </>
                            )}
                          </span>
                          <div className="min-w-0">
                            <span className="truncate font-medium block">{action.label}</span>
                            {!isEnabled && action.unsupportedReason && (
                              <span className="text-[10px] text-stone-500 block truncate leading-tight">
                                {action.unsupportedReason}
                              </span>
                            )}
                          </div>
                        </div>
                        {action.shortcut && (
                          <kbd className="text-[10px] font-mono text-stone-500 bg-white/5 px-1.5 py-0.5 rounded shrink-0">
                            {action.shortcut}
                          </kbd>
                        )}
                      </button>
                    );
                  })}
                </div>
              )}
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
  executingActionId?: ContextActionId | ItemActionId | null;
  executingTargetId?: string | null;
  onExecuteItemAction?: (actionId: ItemActionId, contextId: string, itemId: string) => void;
}

function WorksetPreviewRow({
  item,
  contextId,
  contextState: _contextState,
  isExecuting = false,
  executingActionId,
  executingTargetId,
  onExecuteItemAction,
}: WorksetPreviewRowProps) {
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const [focusedMenuIndex, setFocusedMenuIndex] = useState(-1);
  const rowRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const itemMenuItemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const isLink = item.itemType === 'url' || item.itemType === 'link';
  const isFile = item.itemType === 'file';
  const isImportant = Boolean(item.isImportant);

  const isItemExecuting = Boolean(isExecuting && executingTargetId === item.id);
  const isOpenExecuting =
    isItemExecuting && (executingActionId === 'item.open' || executingActionId === 'item.reveal');
  const isPinExecuting =
    isItemExecuting &&
    (executingActionId === 'item.mark_important' || executingActionId === 'item.unmark_important');

  const itemActions = useMemo(() => {
    return getAvailableItemActions({
      contextId,
      itemId: item.id,
      itemType: item.itemType,
      isImportant,
      status: item.status,
    });
  }, [contextId, item.id, item.itemType, isImportant, item.status]);

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

  const handleOpenOrReveal = (e?: React.MouseEvent | React.KeyboardEvent) => {
    if (e) {
      e.stopPropagation();
    }

    // Concurrency guard: Prevent duplicate action execution (Requirement 9)
    if (isExecuting || isItemExecuting) {
      return;
    }

    if (item.status === 'deleted') {
      toast.error('Item has been deleted and cannot be opened.');
      return;
    }
    if (item.status === 'missing') {
      toast.error('Item is missing or unavailable.');
      return;
    }

    // Dispatch supported open/reveal action through existing item service (Requirements 1, 2, 3, 4, 5, 6, 7)
    if (onExecuteItemAction) {
      onExecuteItemAction('item.open', contextId, item.id);
    }
  };

  const handleRowKeyDown = (e: React.KeyboardEvent) => {
    if (e.target === rowRef.current) {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        e.stopPropagation();
        handleOpenOrReveal();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        const next = rowRef.current?.nextElementSibling as HTMLElement | null;
        next?.focus();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        const prev = rowRef.current?.previousElementSibling as HTMLElement | null;
        prev?.focus();
      }
    }
  };

  const handleMenuKeyDown = (e: React.KeyboardEvent) => {
    if (itemActions.length === 0) {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        setIsMenuOpen(false);
        triggerRef.current?.focus();
      }
      return;
    }

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
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      e.stopPropagation();
      const targetAction = itemActions[focusedMenuIndex];
      if (targetAction && targetAction.isSupported && !isExecuting) {
        setIsMenuOpen(false);
        if (onExecuteItemAction) {
          onExecuteItemAction(targetAction.id, contextId, item.id);
        }
        triggerRef.current?.focus();
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      setIsMenuOpen(false);
      triggerRef.current?.focus();
    } else if (e.key === 'Tab') {
      setIsMenuOpen(false);
      triggerRef.current?.focus();
    }
  };

  const displayTitle = item.title?.trim() || 'Untitled Item';

  return (
    <div
      ref={rowRef}
      role="button"
      tabIndex={0}
      data-workset-row="true"
      onClick={handleOpenOrReveal}
      onKeyDown={handleRowKeyDown}
      aria-label={`Open ${displayTitle}`}
      aria-busy={isOpenExecuting}
      className={`group relative flex items-center justify-between py-1.5 px-2 rounded-md hover:bg-white/[0.04] focus:bg-white/[0.04] active:bg-white/[0.07] focus:outline-none focus-visible:ring-1 focus-visible:ring-orange-400/80 text-xs gap-2 text-stone-300 transition-colors duration-75 select-none cursor-pointer ${
        item.status === 'deleted' ? 'opacity-60' : ''
      }`}
    >
      <div className="flex items-center gap-2 min-w-0 flex-1">
        {isOpenExecuting ? (
          <RefreshCw
            size={12}
            className="text-orange-400 animate-spin shrink-0"
            data-testid={`item-loading-spinner-${item.id}`}
          />
        ) : isLink ? (
          <Link2 size={12} className="text-blue-400/90 shrink-0" />
        ) : isFile ? (
          <FileCode size={12} className="text-emerald-400/90 shrink-0" />
        ) : (
          <FileText size={12} className="text-stone-400 shrink-0" />
        )}
        <span
          className={`truncate text-[11.5px] ${
            item.status === 'deleted'
              ? 'line-through text-stone-500'
              : item.status === 'missing'
              ? 'italic text-stone-400'
              : 'text-stone-200 font-medium'
          }`}
        >
          {displayTitle}
        </span>
        {item.domain ? (
          <span className="text-[10px] text-stone-500 font-mono shrink-0 truncate max-w-[85px]">
            {item.domain}
          </span>
        ) : item.appName ? (
          <span className="text-[10px] text-stone-500 font-mono shrink-0 truncate max-w-[80px]">
            {item.appName}
          </span>
        ) : null}
      </div>

      <div className="flex items-center gap-1 shrink-0 relative" ref={menuRef}>
        {item.isSuggestedAddition && (
          <span
            className="text-[9px] px-1 py-0.5 rounded bg-amber-500/10 text-amber-300 border border-amber-500/20 font-mono shrink-0 font-medium"
            title="Suggested addition inferred from recent activity"
            data-testid={`badge-suggested-${item.id}`}
          >
            Suggested
          </span>
        )}
        {!item.isSuggestedAddition && item.provenance === 'inferred' && (
          <span
            className="text-[9px] px-1 py-0.5 rounded bg-sky-500/10 text-sky-300 border border-sky-500/20 font-mono shrink-0 font-medium"
            title="Inferred contextual relationship"
            data-testid={`badge-inferred-${item.id}`}
          >
            Inferred
          </span>
        )}
        {item.status === 'deleted' && (
          <span
            className="text-[9px] px-1 py-0.5 rounded bg-red-500/10 text-red-300 border border-red-500/20 font-mono shrink-0 font-medium"
            title="Item has been deleted"
            data-testid={`badge-deleted-${item.id}`}
          >
            Deleted
          </span>
        )}
        {item.status === 'missing' && (
          <span
            className="text-[9px] px-1 py-0.5 rounded bg-stone-500/10 text-stone-400 border border-stone-500/20 font-mono shrink-0 font-medium"
            title="Item is unavailable"
            data-testid={`badge-unavailable-${item.id}`}
          >
            Unavailable
          </span>
        )}

        {/* Action Cue (Compact indicator on hover) */}
        {(!item.status || (item.status !== 'deleted' && item.status !== 'missing')) && (
          <span
            className="opacity-0 group-hover:opacity-100 transition-opacity text-stone-500 hover:text-stone-300 shrink-0 px-0.5"
            title={isLink ? 'Open link in browser' : isFile ? 'Reveal file in folder' : 'Open item'}
            data-testid={`action-cue-${item.id}`}
          >
            {isLink ? <ExternalLink size={11} /> : isFile ? <FolderOpen size={11} /> : null}
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
            className={`p-1 rounded hover:bg-white/10 active:bg-white/15 transition-all duration-75 cursor-pointer focus-visible:opacity-100 focus:outline-none focus-visible:ring-1 focus-visible:ring-orange-400 active:scale-95 disabled:opacity-40 disabled:pointer-events-none ${
              isPinExecuting
                ? 'opacity-100 text-amber-400'
                : isImportant
                ? 'opacity-100 text-amber-400'
                : 'opacity-0 group-hover:opacity-100 text-stone-500 hover:text-amber-400'
            }`}
            title={
              isPinExecuting
                ? 'Updating memory retention...'
                : isImportant
                ? 'Pinned in context memory. Click to unpin.'
                : 'Pin item in context memory.'
            }
            aria-label={
              isPinExecuting
                ? 'Updating memory retention'
                : isImportant
                ? 'Unpin item from context memory'
                : 'Pin item in context memory'
            }
          >
            {isPinExecuting ? (
              <RefreshCw size={11} className="animate-spin text-amber-400" />
            ) : (
              <Star size={11} className={isImportant ? 'fill-current' : ''} />
            )}
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
            onKeyDown={(e) => {
              if (e.key === 'ArrowDown' || e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                e.stopPropagation();
                setIsMenuOpen(true);
              }
            }}
            className={`p-1 rounded hover:bg-white/10 active:bg-white/15 transition-all duration-75 cursor-pointer focus-visible:opacity-100 focus:outline-none focus-visible:ring-1 focus-visible:ring-orange-400 active:scale-95 disabled:opacity-40 disabled:pointer-events-none ${
              isMenuOpen
                ? 'opacity-100 bg-white/10 text-stone-100 ring-1 ring-white/20'
                : isItemExecuting && !isPinExecuting && !isOpenExecuting
                ? 'opacity-100 text-stone-200'
                : 'opacity-0 group-hover:opacity-100 text-stone-500 hover:text-stone-200'
            }`}
            title={isItemExecuting ? 'Action executing...' : 'More item actions'}
            aria-label={`More actions for item ${item.title}`}
            aria-haspopup="menu"
            aria-expanded={isMenuOpen}
            aria-controls={`launcher-item-menu-${item.id}`}
          >
            {isItemExecuting && !isPinExecuting && !isOpenExecuting ? (
              <RefreshCw size={11} className="animate-spin text-stone-300" />
            ) : (
              <MoreHorizontal size={11} />
            )}
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
            className="absolute right-0 bottom-full mb-1 w-56 bg-stone-900/95 backdrop-blur-md border border-white/10 rounded-lg shadow-2xl p-1 z-40 animate-in fade-in zoom-in-95 duration-100 focus:outline-none"
          >
            <div className="px-2 py-1 border-b border-white/5 mb-1">
              <span className="text-[10.5px] font-medium text-stone-300 truncate block">
                {item.title}
              </span>
            </div>

            {itemActions.length === 0 ? (
              <div className="px-3 py-2 text-[10.5px] text-stone-500 italic text-center">
                No actions available
              </div>
            ) : (
              <div className="space-y-0.5">
                {itemActions.map((action, idx) => {
                  const isEnabled = action.isSupported;
                  const isFocused = focusedMenuIndex === idx;
                  const isThisItemActionExecuting =
                    isItemExecuting && executingActionId === action.id;
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
                        triggerRef.current?.focus();
                      }}
                      title={!isEnabled ? action.unsupportedReason : action.description}
                      className={`w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-xs text-left transition-colors duration-75 focus:outline-none ${
                        !isEnabled
                          ? isFocused
                            ? 'opacity-40 cursor-not-allowed text-stone-500 bg-white/5 ring-1 ring-white/10 pointer-events-none'
                            : 'opacity-40 cursor-not-allowed text-stone-500 pointer-events-none'
                          : isFocused
                          ? 'bg-white/10 text-stone-100 cursor-pointer ring-1 ring-white/20'
                          : 'text-stone-300 hover:text-stone-100 hover:bg-white/10 active:bg-white/15 cursor-pointer'
                      }`}
                    >
                      <span className="shrink-0 text-stone-400">
                        {isThisItemActionExecuting ? (
                          <RefreshCw size={12} className="animate-spin text-stone-300" />
                        ) : (
                          <>
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
                          </>
                        )}
                      </span>
                      <div className="flex-1 min-w-0">
                        <span className="truncate font-medium block">{action.label}</span>
                        {!isEnabled && action.unsupportedReason && (
                          <span className="text-[10px] text-stone-500 block truncate leading-tight">
                            {action.unsupportedReason}
                          </span>
                        )}
                      </div>
                    </button>
                  );
                })}
              </div>
            )}
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
  isDuplicate?: boolean;
  dataIndex: number;
  onSelect: () => void;
  onResume: () => void;
}

function ContextLauncherItem({
  item,
  isSelected,
  isResuming = false,
  isDuplicate = false,
  dataIndex,
  onSelect,
  onResume,
}: ContextLauncherItemProps) {
  const lifecycle = resolveContextLifecycleState({
    lifecycleState: item.state,
    isActive: item.isActive,
  });
  const isPaused = lifecycle === 'paused';
  const isArchived = lifecycle === 'archived';

  const identity = resolveContextDisplayIdentity(item, isDuplicate);

  return (
    <div
      id={`launcher-item-${item.id}`}
      data-index={dataIndex}
      onClick={onSelect}
      onDoubleClick={onResume}
      className={`group flex items-center justify-between px-2.5 py-2 rounded-lg cursor-pointer transition-all duration-75 select-none focus:outline-none focus-visible:ring-1 focus-visible:ring-white/20 active:scale-[0.99] ${
        isSelected
          ? 'bg-stone-800 hover:bg-stone-800/90 text-stone-100 ring-1 ring-white/10 shadow-xs'
          : 'hover:bg-white/[0.04] active:bg-white/[0.07] text-stone-300'
      }`}
      role="option"
      aria-selected={isSelected}
    >
      <div className="flex items-center gap-2 min-w-0 flex-1 mr-2">
        {/* Minimal status indicator */}
        {item.isActive && !isArchived ? (
          <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 shrink-0 shadow-[0_0_6px_rgba(52,211,153,0.4)]" title="Active" />
        ) : isPaused ? (
          <span className="w-1.5 h-1.5 rounded-full bg-amber-400 shrink-0" title="Paused" />
        ) : isArchived ? (
          <span className="w-1.5 h-1.5 rounded-full bg-stone-600 shrink-0" title="Archived" />
        ) : (
          <span className="w-1.5 h-1.5 rounded-full bg-transparent shrink-0" />
        )}

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className={`text-xs font-medium truncate ${isArchived ? 'text-stone-400' : 'text-stone-100'}`}>
              {identity.name}
            </span>
            {identity.disambiguationTag ? (
              <span
                className="text-[10px] px-1.5 py-0.5 rounded bg-white/5 text-stone-400 font-mono shrink-0 border border-white/5"
                title={`Disambiguation: ${identity.disambiguationTag}`}
              >
                {identity.disambiguationTag}
              </span>
            ) : item.projectIdentifier ? (
              <span className="text-[10px] text-stone-500 font-mono shrink-0">
                {item.projectIdentifier}
              </span>
            ) : null}
            {item.isPinned && (
              <span
                title="Pinned Context"
                aria-label="Pinned Context"
                className="text-amber-400/90 shrink-0 flex items-center ml-0.5"
              >
                <Pin size={10} className="fill-current" />
              </span>
            )}
            {isPaused && (
              <span className="text-[10px] text-amber-400/90 shrink-0 font-medium">
                Paused
              </span>
            )}
            {isArchived && (
              <span className="text-[10px] text-stone-400/90 shrink-0 font-medium px-1.5 py-0.5 rounded bg-white/5 border border-white/5">
                Archived
              </span>
            )}
          </div>

          <div className="flex items-center gap-1.5 text-[11px] text-stone-400 truncate mt-0.5">
            {identity.subtitle && (
              <span className="truncate">{identity.subtitle}</span>
            )}
            {identity.subtitle && <span className="text-stone-600">•</span>}
            <span className="shrink-0">{identity.timeStr}</span>
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
