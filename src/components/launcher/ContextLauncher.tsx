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
} from 'lucide-react';
import { useLauncherStore, LauncherContextItem } from '../../store/launcherStore';
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

    if (diffMin < 2) return 'Active just now';
    if (diffMin < 60) return `Active ${diffMin}m ago`;
    if (diffHour < 24) return `Active ${diffHour}h ago`;
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
    isLoading,
    error,
    activeContext,
    recentContexts,
    closeLauncher,
    setSearchQuery,
    setSelectedIndex,
    fetchLauncherData,
    resumeContextById,
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

  // Filter contexts based on search query
  const filteredList = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    const all: LauncherContextItem[] = [];
    if (activeContext) {
      all.push(activeContext);
    }
    for (const ctx of recentContexts) {
      if (activeContext && ctx.id === activeContext.id) continue;
      all.push(ctx);
    }

    if (!query) return all;

    return all.filter((ctx) => {
      const nameMatch = ctx.name.toLowerCase().includes(query);
      const taskMatch = ctx.taskTitle ? ctx.taskTitle.toLowerCase().includes(query) : false;
      const activityMatch = ctx.recentActivitySummary
        ? ctx.recentActivitySummary.toLowerCase().includes(query)
        : false;
      return nameMatch || taskMatch || activityMatch;
    });
  }, [activeContext, recentContexts, searchQuery]);

  // Keyboard navigation
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeLauncher();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        if (filteredList.length > 0) {
          setSelectedIndex((selectedIndex + 1) % filteredList.length);
        }
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        if (filteredList.length > 0) {
          setSelectedIndex((selectedIndex - 1 + filteredList.length) % filteredList.length);
        }
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (filteredList.length > 0 && selectedIndex >= 0 && selectedIndex < filteredList.length) {
          const target = filteredList[selectedIndex];
          resumeContextById(target.id);
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, filteredList, selectedIndex, closeLauncher, setSelectedIndex, resumeContextById]);

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
      className="fixed inset-0 z-50 flex items-start justify-center pt-24 pb-12 px-4 bg-black/60 backdrop-blur-sm transition-all"
      onClick={(e) => {
        if (e.target === e.currentTarget) closeLauncher();
      }}
      role="dialog"
      aria-modal="true"
      aria-label="Context Launcher"
    >
      <div
        className="w-full max-w-2xl bg-stone-900/95 backdrop-blur-2xl border border-white/10 rounded-2xl shadow-2xl overflow-hidden flex flex-col text-stone-100 animate-in fade-in zoom-in-95 duration-150"
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
              <span className="px-1.5 py-0.5 rounded bg-white/10 text-stone-300 font-mono text-[10px]">↑↓</span>
              <span>Navigate</span>
              <span className="px-1.5 py-0.5 rounded bg-white/10 text-stone-300 font-mono text-[10px] ml-1">↵</span>
              <span>Resume</span>
              <span className="px-1.5 py-0.5 rounded bg-white/10 text-stone-300 font-mono text-[10px] ml-1">Esc</span>
              <span>Close</span>
            </div>
            <button
              onClick={closeLauncher}
              className="p-1 rounded-md text-stone-400 hover:text-white hover:bg-white/10 transition-colors cursor-pointer"
              aria-label="Close launcher"
            >
              <X size={15} />
            </button>
          </div>
        </div>

        {/* 2. Search Input */}
        <div className="relative flex items-center px-5 py-3.5 border-b border-white/10 bg-black/20">
          <Search size={18} className="text-stone-400 shrink-0 mr-3" />
          <input
            ref={inputRef}
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search or switch contexts..."
            className="w-full bg-transparent text-sm text-stone-100 placeholder-stone-500 focus:outline-none"
            aria-label="Search contexts"
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery('')}
              className="p-1 rounded-md text-stone-400 hover:text-white hover:bg-white/10 transition-colors cursor-pointer"
              title="Clear search"
            >
              <X size={14} />
            </button>
          )}
        </div>

        {/* Content Body */}
        <div
          ref={listRef}
          className="max-h-[420px] overflow-y-auto p-4 space-y-4 focus:outline-none"
          tabIndex={-1}
        >
          {/* Loading State */}
          {isLoading && (
            <div className="py-12 flex flex-col items-center justify-center text-center space-y-3">
              <RefreshCw className="w-5 h-5 text-orange-400 animate-spin" />
              <p className="text-xs text-stone-400">Loading context memory from engine...</p>
            </div>
          )}

          {/* Error State */}
          {!isLoading && error && (
            <div className="p-4 rounded-xl bg-red-950/40 border border-red-500/20 text-red-200 flex items-start gap-3">
              <AlertCircle className="w-5 h-5 text-red-400 shrink-0 mt-0.5" />
              <div className="flex-1 min-w-0">
                <p className="text-xs font-semibold text-red-300">Failed to connect to Context Engine</p>
                <p className="text-xs text-red-400/80 mt-0.5">{error}</p>
                <button
                  onClick={fetchLauncherData}
                  className="mt-2.5 inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-red-900/50 hover:bg-red-900 text-xs font-medium text-red-200 border border-red-700/40 transition-colors cursor-pointer"
                >
                  <RefreshCw size={12} />
                  Retry
                </button>
              </div>
            </div>
          )}

          {/* 3. Active Context Section */}
          {!isLoading && !error && !searchQuery && (
            <div className="space-y-1.5">
              <div className="px-1 text-[10px] font-semibold uppercase tracking-wider text-stone-400 flex items-center justify-between">
                <div className="flex items-center gap-1.5">
                  <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                  <span>Current Active Context</span>
                </div>
                {activeContext && (
                  <span className="text-[10px] text-stone-400 font-normal lowercase">
                    {formatGentleTime(activeContext.lastActiveAt, activeContext.durationEstimate)}
                  </span>
                )}
              </div>

              {activeContext ? (
                <ActiveContextCard
                  item={activeContext}
                  isSelected={selectedIndex === 0}
                  dataIndex={0}
                  onSelect={() => setSelectedIndex(0)}
                  onResume={() => resumeContextById(activeContext.id)}
                />
              ) : (
                <div className="p-3.5 rounded-xl bg-stone-850/40 border border-white/5 flex items-start gap-3 text-stone-300">
                  <div className="w-7 h-7 rounded-lg bg-stone-800 flex items-center justify-center text-stone-400 shrink-0 mt-0.5">
                    <Sparkles size={14} className="text-stone-400" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-semibold text-stone-200">No active context</span>
                      <span className="px-1.5 py-0.2 rounded text-[9px] font-medium bg-stone-800 text-stone-400 border border-white/5">
                        Idle
                      </span>
                    </div>
                    <p className="text-xs text-stone-400 mt-1 leading-relaxed">
                      Start working and ContextSplit will build your context.
                    </p>
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Search No Matches State */}
          {!isLoading && !error && searchQuery && filteredList.length === 0 && (
            <div className="py-12 flex flex-col items-center justify-center text-center space-y-2">
              <div className="w-10 h-10 rounded-full bg-stone-800 flex items-center justify-center text-stone-400">
                <Layers size={18} />
              </div>
              <p className="text-sm font-medium text-stone-300">
                No contexts matching "{searchQuery}"
              </p>
              <p className="text-xs text-stone-500 max-w-xs">
                Try searching with a different keyword or create a new context from the sidebar.
              </p>
            </div>
          )}

          {/* 4. Recent Contexts Section */}
          {!isLoading &&
            !error &&
            filteredList.filter((c) => !activeContext || c.id !== activeContext.id || !!searchQuery).length > 0 && (
              <div className="space-y-1 pt-1">
                <div className="px-1 py-1 text-[10px] font-semibold uppercase tracking-wider text-stone-400 flex items-center gap-1.5">
                  <Clock size={11} className="text-stone-400" />
                  <span>{searchQuery ? 'Matching Contexts' : 'Recent Contexts'}</span>
                </div>

                <div className="space-y-1">
                  {filteredList.map((item, idx) => {
                    // Avoid duplicating activeContext in recent list unless searching
                    if (!searchQuery && activeContext && item.id === activeContext.id) return null;
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

        {/* Footer / Status Bar */}
        <div className="px-5 py-2.5 bg-stone-950/60 border-t border-white/5 flex items-center justify-between text-[11px] text-stone-500">
          <div className="flex items-center gap-1.5">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500/80" />
            <span>Context Engine Authoritative</span>
          </div>
          <div className="flex items-center gap-1">
            <span>Press</span>
            <kbd className="px-1 py-0.5 rounded bg-white/10 text-stone-300 font-mono text-[10px]">↵</kbd>
            <span>to resume</span>
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
}

/**
 * Dedicated Active Context Card
 */
interface ActiveContextCardProps {
  item: LauncherContextItem;
  isSelected: boolean;
  dataIndex: number;
  onSelect: () => void;
  onResume: () => void;
}

function ActiveContextCard({
  item,
  isSelected,
  dataIndex,
  onSelect,
  onResume,
}: ActiveContextCardProps) {
  const readiness = item.resumeReadiness;
  const isReady = readiness?.readiness === 'ready' || readiness?.readiness === 'mostly_ready';

  return (
    <div
      data-index={dataIndex}
      onClick={onResume}
      onMouseEnter={onSelect}
      className={`group relative p-3.5 rounded-xl cursor-pointer transition-all border ${
        isSelected
          ? 'bg-emerald-950/30 border-emerald-500/50 shadow-md ring-1 ring-emerald-500/30'
          : 'bg-emerald-950/20 hover:bg-emerald-950/30 border-emerald-500/20'
      }`}
      role="option"
      aria-selected={isSelected}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-3 min-w-0 flex-1">
          <div className="w-8 h-8 rounded-lg bg-emerald-500/20 border border-emerald-500/40 flex items-center justify-center text-emerald-400 shrink-0 mt-0.5">
            <CheckCircle2 size={16} />
          </div>

          <div className="min-w-0 flex-1 space-y-1">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-sm font-semibold text-stone-100 truncate">
                {item.name}
              </span>
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                Active
              </span>
              {readiness && (
                <span
                  className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium border ${
                    isReady
                      ? 'bg-emerald-900/40 text-emerald-300 border-emerald-700/40'
                      : 'bg-amber-900/40 text-amber-300 border-amber-700/40'
                  }`}
                  title={readiness.formattedExplanation || readiness.summary}
                >
                  <Zap size={10} />
                  <span className="capitalize">{readiness.readiness.replace('_', ' ')}</span>
                </span>
              )}
            </div>

            {/* Recent Activity / Task Statement */}
            <div className="flex items-center gap-1.5 text-xs text-stone-300">
              {item.taskTitle ? (
                <span className="truncate">
                  Task: <span className="font-medium text-emerald-200">{item.taskTitle}</span>
                </span>
              ) : item.recentActivitySummary ? (
                <span className="truncate flex items-center gap-1 text-stone-300">
                  <Activity size={12} className="text-emerald-400 shrink-0" />
                  <span className="truncate">{item.recentActivitySummary}</span>
                </span>
              ) : (
                <span className="text-stone-400">Context active and tracking working state</span>
              )}
              {item.itemCount !== undefined && (
                <span className="text-stone-400 text-[11px] shrink-0">• {item.itemCount} items</span>
              )}
            </div>
          </div>
        </div>

        {/* Quick Action Button */}
        <div className="flex items-center gap-2 shrink-0 pt-0.5">
          <span
            className={`text-xs font-medium px-2.5 py-1 rounded-lg flex items-center gap-1.5 transition-all ${
              isSelected
                ? 'bg-emerald-500 text-stone-950 shadow-sm font-semibold'
                : 'bg-emerald-500/20 text-emerald-300 group-hover:bg-emerald-500/30'
            }`}
          >
            <span>Resume</span>
            <CornerDownLeft size={12} />
          </span>
        </div>
      </div>
    </div>
  );
}

/**
 * Recent Context List Item
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
  return (
    <div
      data-index={dataIndex}
      onClick={onResume}
      onMouseEnter={onSelect}
      className={`group relative flex items-center justify-between px-3.5 py-2.5 rounded-xl cursor-pointer transition-all border ${
        isSelected
          ? 'bg-orange-500/20 text-white border-orange-500/40 shadow-sm'
          : 'hover:bg-white/5 text-stone-200 border-transparent'
      }`}
      role="option"
      aria-selected={isSelected}
    >
      <div className="flex items-center gap-3 min-w-0 flex-1 mr-3">
        <div
          className={`w-8 h-8 rounded-lg flex items-center justify-center shrink-0 transition-colors ${
            item.isActive
              ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/30'
              : isSelected
              ? 'bg-orange-500/30 text-orange-200 border border-orange-500/40'
              : 'bg-stone-800 text-stone-400 border border-white/5'
          }`}
        >
          {item.isActive ? <CheckCircle2 size={16} /> : <FolderOpen size={16} />}
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-xs font-semibold truncate text-stone-100">
              {item.name}
            </span>
            {item.confidence && (
              <span className="text-[10px] text-stone-400 capitalize">
                • {item.confidence}
              </span>
            )}
          </div>

          <div className="flex items-center gap-2 mt-0.5 text-[11px] text-stone-400 truncate">
            {item.taskTitle ? (
              <span className="truncate text-stone-300">
                Task: <span className="text-stone-200">{item.taskTitle}</span>
              </span>
            ) : item.durationEstimate ? (
              <span>{item.durationEstimate}</span>
            ) : (
              <span>{formatGentleTime(item.lastActiveAt)}</span>
            )}
            {item.itemCount !== undefined && (
              <span className="text-stone-400">• {item.itemCount} items</span>
            )}
          </div>
        </div>
      </div>

      <div className="flex items-center gap-2 shrink-0">
        <span
          className={`text-[10px] font-medium px-2 py-0.5 rounded-full flex items-center gap-1 transition-opacity ${
            isSelected
              ? 'opacity-100 bg-orange-500/30 text-orange-200 border border-orange-400/30'
              : 'opacity-0 group-hover:opacity-100 text-stone-400 bg-white/5'
          }`}
        >
          <span>Resume</span>
          <CornerDownLeft size={10} />
        </span>
      </div>
    </div>
  );
}
