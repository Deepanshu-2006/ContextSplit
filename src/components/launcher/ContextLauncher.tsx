import { useEffect, useRef, useMemo } from 'react';
import { createPortal } from 'react-dom';
import {
  Search,
  Clock,
  Layers,
  X,
  AlertCircle,
  RefreshCw,
  CornerDownLeft,
  CheckCircle2,
  FolderOpen,
  Compass,
  Activity,
  Sparkles,
  Zap,
  Archive,
  FileText,
  Link2,
  Globe,
  FileCode,
  HelpCircle,
  Play,
  Star,
} from 'lucide-react';
import {
  useLauncherStore,
  LauncherContextItem,
  LauncherPreviewModel,
  LauncherPreviewItem,
  ResumeActionStatus,
  matchContext,
  sortContextMatches,
} from '../../store/launcherStore';
import { useWorkspaceStore } from '../../store/workspaceStore';

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
    closeLauncher,
    setSearchQuery,
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

  // Filter contexts based on search query using deterministic structured matching
  const filteredList = useMemo(() => {
    const all: LauncherContextItem[] = [];
    if (activeContext) {
      all.push(activeContext);
    }
    for (const ctx of recentContexts) {
      if (activeContext && ctx.id === activeContext.id) continue;
      all.push(ctx);
    }

    if (!searchQuery.trim()) return all;

    const matched = all.filter((ctx) => matchContext(ctx, searchQuery));
    return sortContextMatches(matched, searchQuery);
  }, [activeContext, recentContexts, searchQuery]);

  // When selection index changes, fetch preview for that selected context
  useEffect(() => {
    if (filteredList.length > 0 && selectedIndex >= 0 && selectedIndex < filteredList.length) {
      const current = filteredList[selectedIndex];
      if (current && current.id !== selectedContextId) {
        selectContextById(current.id);
      }
    }
  }, [selectedIndex, filteredList, selectedContextId, selectContextById]);

  // Mac-native Keyboard Navigation
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // 1. Escape: dismiss prompt -> clear search -> close launcher
      if (e.key === 'Escape') {
        e.preventDefault();
        if (notReadyReason || resumeError) {
          clearResumeState();
        } else if (searchQuery) {
          setSearchQuery('');
          inputRef.current?.focus();
        } else {
          closeLauncher();
        }
        return;
      }

      // 2. Focus Search: Cmd+F, Ctrl+F, or '/' (when not typing in an input)
      if (
        ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') ||
        (e.key === '/' && document.activeElement !== inputRef.current)
      ) {
        e.preventDefault();
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

      // 5. Resume Action: Enter / Return
      if (e.key === 'Enter') {
        e.preventDefault();
        if (notReadyReason) {
          // If prompt was already showing, Enter triggers forced resume
          if (selectedContextId) {
            resumeContextById(selectedContextId, true);
          }
        } else if (filteredList.length > 0 && selectedIndex >= 0 && selectedIndex < filteredList.length) {
          const target = filteredList[selectedIndex];
          resumeContextById(target.id);
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
    notReadyReason,
    resumeError,
    searchQuery,
    closeLauncher,
    setSearchQuery,
    setSelectedIndex,
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
        className="w-full max-w-4xl bg-stone-900/95 backdrop-blur-2xl border border-white/10 rounded-2xl shadow-2xl overflow-hidden flex flex-col text-stone-100 animate-in fade-in zoom-in-95 duration-150 focus-within:ring-1 focus-within:ring-white/15"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 1. Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-white/10 bg-white/[0.02]">
          <div className="flex items-center gap-2.5">
            <div className="w-6 h-6 rounded-lg bg-orange-500/20 border border-orange-500/30 flex items-center justify-center text-orange-400">
              <Compass size={14} />
            </div>
            <span className="text-xs font-semibold tracking-wide uppercase text-stone-300">
              Context Launcher
            </span>
          </div>

          <div className="flex items-center gap-3">
            <div className="hidden sm:flex items-center gap-1.5 text-[11px] text-stone-400">
              <span className="px-1.5 py-0.5 rounded bg-white/10 text-stone-300 font-mono text-[10px]">↑↓ / ⌃N⌃P</span>
              <span>Navigate</span>
              <span className="px-1.5 py-0.5 rounded bg-white/10 text-stone-300 font-mono text-[10px] ml-1">↵</span>
              <span>Resume</span>
              <span className="px-1.5 py-0.5 rounded bg-white/10 text-stone-300 font-mono text-[10px] ml-1">Esc</span>
              <span>Close</span>
            </div>
            <button
              onClick={closeLauncher}
              className="p-1 rounded-md text-stone-400 hover:text-white hover:bg-white/10 focus-visible:ring-2 focus-visible:ring-orange-400 focus-visible:outline-none transition-colors cursor-pointer"
              aria-label="Close launcher"
            >
              <X size={15} />
            </button>
          </div>
        </div>

        {/* 2. Search Input */}
        <div className="relative flex items-center px-5 py-3.5 border-b border-white/10 bg-black/20 focus-within:bg-black/40 transition-colors">
          <Search size={18} className="text-stone-400 shrink-0 mr-3" aria-hidden="true" />
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
          {searchQuery && (
            <button
              type="button"
              onClick={() => {
                setSearchQuery('');
                inputRef.current?.focus();
              }}
              className="p-1 rounded-md text-stone-400 hover:text-white hover:bg-white/10 focus-visible:ring-2 focus-visible:ring-orange-400 focus-visible:outline-none transition-colors cursor-pointer"
              title="Clear search"
              aria-label="Clear search"
            >
              <X size={14} />
            </button>
          )}
        </div>

        {/* 3. Main Split View: Left List | Right Preview */}
        <div className="grid grid-cols-1 md:grid-cols-12 min-h-[400px] max-h-[480px] divide-y md:divide-y-0 md:divide-x divide-white/10">
          {/* Left Column: Contexts List */}
          <div
            id="launcher-context-list"
            role="listbox"
            aria-label="Context List"
            aria-activedescendant={selectedContextId ? `launcher-item-${selectedContextId}` : undefined}
            ref={listRef}
            className="md:col-span-6 lg:col-span-5 overflow-y-auto p-3.5 space-y-3.5 focus:outline-none"
            tabIndex={-1}
          >
            {/* Loading Skeleton State */}
            {isLoading && (
              <div className="space-y-2 py-1 animate-pulse">
                <div className="px-1 h-3 w-20 bg-stone-800 rounded text-[10px]" />
                <div className="p-3 rounded-xl bg-stone-850/50 border border-white/5 space-y-2">
                  <div className="h-4 w-32 bg-stone-800 rounded" />
                  <div className="h-3 w-48 bg-stone-800/60 rounded" />
                </div>
                <div className="px-1 h-3 w-24 bg-stone-800 rounded text-[10px] mt-2" />
                <div className="p-3 rounded-xl bg-stone-850/50 border border-white/5 space-y-2">
                  <div className="h-4 w-28 bg-stone-800 rounded" />
                  <div className="h-3 w-40 bg-stone-800/60 rounded" />
                </div>
                <div className="p-3 rounded-xl bg-stone-850/50 border border-white/5 space-y-2">
                  <div className="h-4 w-36 bg-stone-800 rounded" />
                  <div className="h-3 w-32 bg-stone-800/60 rounded" />
                </div>
              </div>
            )}

            {/* Error State */}
            {!isLoading && error && (
              <div className="p-3.5 rounded-xl bg-red-950/40 border border-red-500/20 text-red-200 flex items-start gap-2.5">
                <AlertCircle className="w-4 h-4 text-red-400 shrink-0 mt-0.5" />
                <div className="flex-1 min-w-0">
                  <p className="text-xs font-semibold text-red-300">Connection Error</p>
                  <p className="text-xs text-red-400/80 mt-0.5">{error}</p>
                  <button
                    onClick={fetchLauncherData}
                    className="mt-2 inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md bg-red-900/50 hover:bg-red-900 text-[11px] font-medium text-red-200 border border-red-700/40 transition-colors cursor-pointer"
                  >
                    <RefreshCw size={11} />
                    Retry
                  </button>
                </div>
              </div>
            )}

            {/* Active Context Section */}
            {!isLoading && !error && !searchQuery && (
              <div className="space-y-1">
                <div className="px-1 text-[10px] font-semibold uppercase tracking-wider text-stone-400 flex items-center justify-between">
                  <div className="flex items-center gap-1.5">
                    <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                    <span>Current Active</span>
                  </div>
                  {activeContext && (
                    <span className="text-[10px] text-stone-400 font-normal">
                      {formatGentleTime(activeContext.lastActiveAt, activeContext.durationEstimate)}
                    </span>
                  )}
                </div>

                {activeContext ? (
                  <ContextLauncherItem
                    item={activeContext}
                    isSelected={selectedIndex === 0}
                    dataIndex={0}
                    onSelect={() => setSelectedIndex(0)}
                    onResume={() => resumeContextById(activeContext.id)}
                  />
                ) : (
                  <div className="p-3 rounded-xl bg-stone-850/40 border border-white/5 flex items-start gap-2.5 text-stone-300">
                    <div className="w-6 h-6 rounded-lg bg-stone-800 flex items-center justify-center text-stone-400 shrink-0 mt-0.5">
                      <Sparkles size={13} className="text-stone-400" />
                    </div>
                    <div className="flex-1 min-w-0">
                      <span className="text-xs font-semibold text-stone-200">No active context</span>
                      <p className="text-[11px] text-stone-400 mt-0.5 leading-relaxed">
                        Start working and ContextSplit will build your context.
                      </p>
                    </div>
                  </div>
                )}
              </div>
            )}

            {/* Empty Search State */}
            {!isLoading && !error && searchQuery && filteredList.length === 0 && (
              <div className="py-12 flex flex-col items-center justify-center text-center space-y-2">
                <div className="w-9 h-9 rounded-full bg-stone-800 flex items-center justify-center text-stone-400">
                  <Layers size={16} />
                </div>
                <p className="text-xs font-medium text-stone-300">
                  No contexts matching "{searchQuery}"
                </p>
                <p className="text-[11px] text-stone-500 max-w-xs">
                  Try searching with a different keyword or create a new context.
                </p>
              </div>
            )}

            {/* Recent Contexts Section */}
            {!isLoading && !error && !searchQuery && (
              <div className="space-y-1 pt-1">
                <div className="px-1 py-1 text-[10px] font-semibold uppercase tracking-wider text-stone-400 flex items-center justify-between">
                  <div className="flex items-center gap-1.5">
                    <Clock size={11} className="text-stone-400" />
                    <span>Recent Contexts</span>
                  </div>
                  {recentContexts.length > 0 && (
                    <span className="text-[10px] text-stone-400 font-normal">
                      {recentContexts.length}
                    </span>
                  )}
                </div>

                {recentContexts.length === 0 ? (
                  <div className="p-3.5 rounded-xl bg-stone-850/30 border border-white/5 text-center text-stone-400 space-y-0.5">
                    <p className="text-xs font-medium text-stone-300">No recent contexts</p>
                    <p className="text-[11px] text-stone-500">
                      Recently active projects will appear here.
                    </p>
                  </div>
                ) : (
                  <div className="space-y-1">
                    {recentContexts.map((item, relIdx) => {
                      const globalIdx = activeContext ? relIdx + 1 : relIdx;
                      const isSelected = selectedIndex === globalIdx;
                      return (
                        <ContextLauncherItem
                          key={item.id}
                          item={item}
                          isSelected={isSelected}
                          dataIndex={globalIdx}
                          onSelect={() => setSelectedIndex(globalIdx)}
                          onResume={() => resumeContextById(item.id)}
                        />
                      );
                    })}
                  </div>
                )}
              </div>
            )}

            {/* Search Results List */}
            {!isLoading && !error && searchQuery && filteredList.length > 0 && (
              <div className="space-y-1">
                <div className="px-1 py-1 text-[10px] font-semibold uppercase tracking-wider text-stone-400 flex items-center gap-1.5">
                  <Search size={11} className="text-stone-400" />
                  <span>Matching Contexts ({filteredList.length})</span>
                </div>
                <div className="space-y-1">
                  {filteredList.map((item, idx) => {
                    const isSelected = selectedIndex === idx;
                    return (
                      <ContextLauncherItem
                        key={item.id}
                        item={item}
                        isSelected={isSelected}
                        dataIndex={idx}
                        onSelect={() => setSelectedIndex(idx)}
                        onResume={() => resumeContextById(item.id)}
                      />
                    );
                  })}
                </div>
              </div>
            )}
          </div>

          {/* Right Column: Context Preview Pane */}
          <div className="md:col-span-6 lg:col-span-7 bg-stone-950/40 p-4 overflow-y-auto flex flex-col justify-between">
            <ContextPreviewPane
              preview={activePreview}
              isLoading={isLoadingPreview}
              isResuming={isResuming}
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
              onClose={closeLauncher}
            />
          </div>
        </div>

        {/* Footer / Status Bar */}
        <div className="px-5 py-2.5 bg-stone-950/80 border-t border-white/5 flex items-center justify-between text-[11px] text-stone-500">
          <div className="flex items-center gap-2">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500/80" />
            <span>Answers: What will I return to if I resume?</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="flex items-center gap-1">
              <span>Press</span>
              <kbd className="px-1 py-0.5 rounded bg-white/10 text-stone-300 font-mono text-[10px]">↵</kbd>
              <span>to resume</span>
              <span className="mx-1 text-stone-600">•</span>
              <kbd className="px-1 py-0.5 rounded bg-white/10 text-stone-300 font-mono text-[10px]">Esc</kbd>
              <span>to close</span>
            </div>
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
  onClose,
}: ContextPreviewPaneProps) {
  if (isLoading) {
    return (
      <div className="space-y-4 animate-pulse p-1">
        <div className="flex items-center justify-between">
          <div className="h-5 w-40 bg-stone-800 rounded-md" />
          <div className="h-5 w-16 bg-stone-800 rounded-full" />
        </div>
        <div className="h-20 bg-stone-850/60 border border-white/5 rounded-xl" />
        <div className="space-y-2">
          <div className="h-3 w-28 bg-stone-800 rounded" />
          <div className="h-10 bg-stone-850/60 border border-white/5 rounded-lg" />
          <div className="h-10 bg-stone-850/60 border border-white/5 rounded-lg" />
        </div>
        <div className="h-9 w-full bg-stone-800 rounded-xl mt-4" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-center p-6 space-y-2.5">
        <div className="w-10 h-10 rounded-full bg-red-950/50 border border-red-500/30 flex items-center justify-center text-red-400">
          <AlertCircle size={18} />
        </div>
        <p className="text-xs font-semibold text-stone-300">Context Preview Unavailable</p>
        <p className="text-[11px] text-stone-500 max-w-xs">{error}</p>
        <button
          onClick={onClose}
          className="mt-2 px-3 py-1 text-xs text-stone-400 hover:text-white rounded bg-white/5 hover:bg-white/10 cursor-pointer"
        >
          Close
        </button>
      </div>
    );
  }

  if (!preview) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-center p-6 space-y-2 text-stone-500">
        <div className="w-10 h-10 rounded-full bg-stone-850 border border-white/5 flex items-center justify-center text-stone-400">
          <Compass size={18} />
        </div>
        <p className="text-xs font-medium text-stone-300">No context selected</p>
        <p className="text-[11px] text-stone-500 max-w-xs">
          Select a context from the list to preview where you left off, active workset, and resume readiness.
        </p>
      </div>
    );
  }

  const isReady = preview.readinessBand === 'ready' || preview.readinessBand === 'mostly_ready';

  return (
    <div className="space-y-3.5 flex flex-col justify-between h-full">
      <div className="space-y-3">
        {/* Preview Header: Name, Last Active Time, and State/Readiness */}
        <div className="flex items-start justify-between gap-2 border-b border-white/5 pb-2.5">
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-stone-100 truncate">
              {preview.name}
            </h3>
            <p className="text-[11px] text-stone-400 mt-0.5">
              {formatGentleTime(preview.lastActiveAt, preview.durationEstimate)}
            </p>
          </div>

          <div className="flex items-center gap-1.5 shrink-0 flex-wrap justify-end">
            {preview.isAmbiguous && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-amber-950/60 text-amber-300 border border-amber-500/30">
                <HelpCircle size={10} />
                <span>Clarifying State</span>
              </span>
            )}

            {preview.isStale && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-stone-800 text-stone-400 border border-white/5">
                <Archive size={10} />
                <span>Dormant</span>
              </span>
            )}

            <span
              className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium border ${
                isReady
                  ? 'bg-emerald-950/60 text-emerald-300 border-emerald-500/30'
                  : 'bg-amber-950/60 text-amber-300 border-amber-500/30'
              }`}
            >
              <Zap size={10} />
              <span className="capitalize">{preview.readinessBand.replace('_', ' ')}</span>
            </span>
          </div>
        </div>

        {/* Where You Left Off / Active Task Card */}
        <div className="p-3 rounded-xl bg-stone-850/60 border border-white/5 space-y-1.5">
          <div className="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wider text-stone-400">
            <Activity size={11} className="text-orange-400" />
            <span>Where you left off</span>
          </div>

          {preview.taskTitle ? (
            <p className="text-xs font-medium text-stone-200 leading-snug">
              Working on <span className="text-orange-300 font-semibold">{preview.taskTitle}</span>
            </p>
          ) : preview.activitySummary ? (
            <p className="text-xs text-stone-300 leading-snug">{preview.activitySummary}</p>
          ) : (
            <p className="text-xs text-stone-400 italic">No specific task recorded yet.</p>
          )}

          {preview.readinessExplanation && (
            <p className="text-[11px] text-stone-400 pt-0.5 border-t border-white/5 mt-1">
              {preview.readinessExplanation}
            </p>
          )}
        </div>

        {/* Not Ready Explanation Notice */}
        {notReadyReason && (
          <div className="p-3 rounded-xl bg-amber-950/40 border border-amber-500/30 text-amber-200 space-y-2">
            <div className="flex items-start gap-2">
              <AlertCircle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
              <div className="min-w-0 flex-1">
                <p className="text-xs font-semibold text-amber-300">Context Not Fully Ready</p>
                <p className="text-[11px] text-amber-400/90 mt-0.5 leading-relaxed">
                  {notReadyReason}
                </p>
              </div>
            </div>
            <div className="flex items-center gap-2 pt-1 border-t border-amber-500/20">
              <button
                onClick={() => onResume(true)}
                disabled={isResuming}
                className="px-2.5 py-1 rounded-lg bg-amber-500 hover:bg-amber-600 disabled:opacity-50 text-stone-950 font-semibold text-[11px] shadow-sm transition-colors cursor-pointer"
              >
                {isResuming ? 'Resuming...' : 'Resume Anyway'}
              </button>
              <button
                onClick={onClearResumeState}
                disabled={isResuming}
                className="px-2.5 py-1 rounded-lg bg-white/5 hover:bg-white/10 text-stone-300 text-[11px] font-medium transition-colors cursor-pointer"
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {/* Resume Error Alert */}
        {resumeError && (
          <div className="p-3 rounded-xl bg-red-950/40 border border-red-500/30 text-red-200 flex items-start gap-2 text-xs">
            <AlertCircle className="w-4 h-4 text-red-400 shrink-0 mt-0.5" />
            <div className="flex-1 min-w-0">
              <p className="font-semibold text-red-300">Resume Failed</p>
              <p className="text-[11px] text-red-400/90 mt-0.5">{resumeError}</p>
            </div>
            <button
              onClick={onClearResumeState}
              className="text-stone-400 hover:text-white p-0.5 cursor-pointer"
              aria-label="Dismiss error"
            >
              <X size={12} />
            </button>
          </div>
        )}

        {/* Important Items Section */}
        {preview.importantItems && preview.importantItems.length > 0 && (
          <div className="space-y-1.5">
            <div className="flex items-center gap-1 text-[10px] font-semibold uppercase tracking-wider text-amber-400 px-0.5">
              <Star size={10} className="fill-amber-400" />
              <span>Important Items</span>
            </div>
            <div className="space-y-1">
              {preview.importantItems.map((item) => (
                <WorksetPreviewRow key={`imp-${item.id}`} item={item} isImportant />
              ))}
            </div>
          </div>
        )}

        {/* Workset Items Preview */}
        <div className="space-y-1.5">
          <div className="flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-stone-400 px-0.5">
            <span>Working Set Summary</span>
            <span>{preview.itemCount} items</span>
          </div>

          {preview.worksetItems.length === 0 ? (
            <div className="p-2.5 rounded-lg bg-stone-850/30 border border-white/5 text-[11px] text-stone-500 text-center">
              No items in active workset
            </div>
          ) : (
            <div className="space-y-1">
              {preview.worksetItems.map((item) => (
                <WorksetPreviewRow key={item.id} item={item} />
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Action Buttons: Primary (Resume) + Secondary (Close) */}
      <div className="pt-3 border-t border-white/5 flex items-center gap-2">
        <button
          onClick={() => onResume(false)}
          disabled={isResuming}
          className="flex-1 flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-orange-500 hover:bg-orange-600 disabled:opacity-50 disabled:cursor-not-allowed text-stone-950 font-semibold text-xs shadow-lg shadow-orange-500/20 transition-all cursor-pointer"
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

        <button
          onClick={onClose}
          disabled={isResuming}
          className="px-3.5 py-2 rounded-xl bg-white/5 hover:bg-white/10 disabled:opacity-50 text-stone-400 hover:text-stone-200 text-xs font-medium transition-all border border-white/5 cursor-pointer"
        >
          <span>Close</span>
          <kbd className="ml-1.5 px-1 py-0.5 text-[9px] font-mono bg-white/10 text-stone-400 rounded">
            Esc
          </kbd>
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
  isImportant = false,
}: {
  item: LauncherPreviewItem;
  isImportant?: boolean;
}) {
  const getIcon = () => {
    switch (item.itemType) {
      case 'url':
      case 'link':
        return <Globe size={12} className="text-blue-400 shrink-0" />;
      case 'code':
      case 'snippet':
        return <FileCode size={12} className="text-amber-400 shrink-0" />;
      case 'file':
        return <FileText size={12} className="text-emerald-400 shrink-0" />;
      default:
        return <Link2 size={12} className="text-stone-400 shrink-0" />;
    }
  };

  return (
    <div
      className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg border text-xs gap-2 ${
        isImportant
          ? 'bg-amber-950/20 border-amber-500/30 text-amber-100'
          : 'bg-stone-850/40 border-white/5 text-stone-300'
      }`}
    >
      <div className="flex items-center gap-2 min-w-0 flex-1">
        {getIcon()}
        <span className="truncate text-[11.5px] font-medium">{item.title}</span>
      </div>
      {item.appName && (
        <span className="text-[10px] text-stone-500 shrink-0 font-mono bg-stone-800/80 px-1.5 py-0.5 rounded">
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
  dataIndex: number;
  onSelect: () => void;
  onResume: () => void;
}

function ContextLauncherItem({
  item,
  isSelected,
  dataIndex,
  onSelect,
  onResume,
}: ContextLauncherItemProps) {
  const isDormant = item.state === 'dormant';
  const isArchived = item.state === 'archived';

  return (
    <div
      id={`launcher-item-${item.id}`}
      data-index={dataIndex}
      onClick={onSelect}
      onDoubleClick={onResume}
      onMouseEnter={onSelect}
      className={`group relative flex items-center justify-between px-3 py-2.5 rounded-xl cursor-pointer transition-all border ${
        isSelected
          ? 'bg-orange-500/20 text-white border-orange-500/40 shadow-sm ring-1 ring-orange-500/30'
          : 'hover:bg-white/5 text-stone-200 border-transparent'
      }`}
      role="option"
      aria-selected={isSelected}
    >
      <div className="flex items-center gap-2.5 min-w-0 flex-1 mr-2">
        <div
          className={`w-7 h-7 rounded-lg flex items-center justify-center shrink-0 transition-colors ${
            item.isActive
              ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/30'
              : isSelected
              ? 'bg-orange-500/30 text-orange-200 border border-orange-500/40'
              : isDormant || isArchived
              ? 'bg-stone-850 text-stone-500 border border-white/5'
              : 'bg-stone-800 text-stone-400 border border-white/5'
          }`}
        >
          {item.isActive ? (
            <CheckCircle2 size={15} />
          ) : isArchived ? (
            <Archive size={14} />
          ) : (
            <FolderOpen size={14} />
          )}
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span
              className={`text-xs font-semibold truncate ${
                isDormant || isArchived ? 'text-stone-300' : 'text-stone-100'
              }`}
            >
              {item.name}
            </span>
            {item.isActive && (
              <span className="px-1.5 py-0.2 rounded text-[8.5px] font-medium bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">
                Active
              </span>
            )}
            {isDormant && (
              <span className="px-1.5 py-0.2 rounded text-[8.5px] font-medium bg-stone-800 text-stone-400 border border-white/5">
                Dormant
              </span>
            )}
          </div>

          <div className="flex items-center gap-1.5 mt-0.5 text-[10.5px] text-stone-400 truncate">
            {item.taskTitle ? (
              <span className="truncate text-stone-300">
                Task: <span className="text-stone-200">{item.taskTitle}</span>
              </span>
            ) : item.durationEstimate ? (
              <span>{item.durationEstimate}</span>
            ) : (
              <span>{formatGentleTime(item.lastActiveAt)}</span>
            )}
          </div>
        </div>
      </div>

      <div className="flex items-center gap-1 shrink-0">
        <span
          className={`text-[9px] font-medium px-2 py-0.5 rounded-full flex items-center gap-1 transition-opacity ${
            isSelected
              ? 'opacity-100 bg-orange-500/30 text-orange-200 border border-orange-400/30'
              : 'opacity-0 text-stone-400'
          }`}
        >
          <span>Select</span>
          <CornerDownLeft size={9} />
        </span>
      </div>
    </div>
  );
}
