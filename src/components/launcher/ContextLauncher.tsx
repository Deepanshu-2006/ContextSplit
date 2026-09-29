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
} from 'lucide-react';
import { useLauncherStore, LauncherContextItem } from '../../store/launcherStore';

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
      return nameMatch || taskMatch;
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
              className="p-1 rounded-md text-stone-400 hover:text-white hover:bg-white/10 transition-colors"
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
              className="p-1 rounded-md text-stone-400 hover:text-white hover:bg-white/10 transition-colors"
              title="Clear search"
            >
              <X size={14} />
            </button>
          )}
        </div>

        {/* Content Body */}
        <div
          ref={listRef}
          className="max-h-[380px] overflow-y-auto p-3 space-y-4 focus:outline-none"
          tabIndex={-1}
        >
          {/* 6. Loading State */}
          {isLoading && (
            <div className="py-12 flex flex-col items-center justify-center text-center space-y-3">
              <RefreshCw className="w-5 h-5 text-orange-400 animate-spin" />
              <p className="text-xs text-stone-400">Loading context memory from engine...</p>
            </div>
          )}

          {/* 8. Error State */}
          {!isLoading && error && (
            <div className="p-4 rounded-xl bg-red-950/40 border border-red-500/20 text-red-200 flex items-start gap-3">
              <AlertCircle className="w-5 h-5 text-red-400 shrink-0 mt-0.5" />
              <div className="flex-1 min-w-0">
                <p className="text-xs font-semibold text-red-300">Failed to connect to Context Engine</p>
                <p className="text-xs text-red-400/80 mt-0.5">{error}</p>
                <button
                  onClick={fetchLauncherData}
                  className="mt-2.5 inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-red-900/50 hover:bg-red-900 text-xs font-medium text-red-200 border border-red-700/40 transition-colors"
                >
                  <RefreshCw size={12} />
                  Retry
                </button>
              </div>
            </div>
          )}

          {/* 5. Empty State */}
          {!isLoading && !error && filteredList.length === 0 && (
            <div className="py-12 flex flex-col items-center justify-center text-center space-y-2">
              <div className="w-10 h-10 rounded-full bg-stone-800 flex items-center justify-center text-stone-400">
                <Layers size={18} />
              </div>
              <p className="text-sm font-medium text-stone-300">
                {searchQuery ? `No contexts matching "${searchQuery}"` : 'No contexts available'}
              </p>
              <p className="text-xs text-stone-500 max-w-xs">
                {searchQuery
                  ? 'Try searching with a different keyword or create a new context from the sidebar.'
                  : 'Contexts will appear here as you work in ContextSplit.'}
              </p>
            </div>
          )}

          {/* Contexts List */}
          {!isLoading && !error && filteredList.length > 0 && (
            <div className="space-y-1">
              {/* 3. Active Context Section */}
              {activeContext && (!searchQuery || filteredList.some((c) => c.id === activeContext.id)) && (
                <div className="mb-2">
                  <div className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-stone-400 flex items-center gap-1.5">
                    <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                    Active Context
                  </div>
                  {(() => {
                    const idx = filteredList.findIndex((c) => c.id === activeContext.id);
                    if (idx === -1) return null;
                    const isSelected = selectedIndex === idx;
                    return (
                      <ContextLauncherItem
                        key={activeContext.id}
                        item={activeContext}
                        isSelected={isSelected}
                        dataIndex={idx}
                        onSelect={() => setSelectedIndex(idx)}
                        onResume={() => resumeContextById(activeContext.id)}
                      />
                    );
                  })()}
                </div>
              )}

              {/* 4. Recent Contexts Section */}
              {filteredList.filter((c) => !activeContext || c.id !== activeContext.id).length > 0 && (
                <div>
                  <div className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-stone-400 flex items-center gap-1.5">
                    <Clock size={11} className="text-stone-400" />
                    Recent Contexts
                  </div>
                  {filteredList.map((item, idx) => {
                    if (activeContext && item.id === activeContext.id) return null;
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
              )}
            </div>
          )}
        </div>

        {/* Footer / Status Bar */}
        <div className="px-5 py-2.5 bg-stone-950/60 border-t border-white/5 flex items-center justify-between text-[11px] text-stone-500">
          <div className="flex items-center gap-1.5">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500/80" />
            <span>Context Engine Single Source of Truth</span>
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
      className={`group relative flex items-center justify-between px-3 py-2.5 rounded-xl cursor-pointer transition-all ${
        isSelected
          ? 'bg-orange-500/20 text-white border border-orange-500/40 shadow-sm'
          : 'hover:bg-white/5 text-stone-200 border border-transparent'
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
              ? 'bg-orange-500/30 text-orange-200'
              : 'bg-stone-800 text-stone-400'
          }`}
        >
          {item.isActive ? <CheckCircle2 size={16} /> : <FolderOpen size={16} />}
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-xs font-semibold truncate text-stone-100">
              {item.name}
            </span>
            {item.isActive && (
              <span className="px-1.5 py-0.2 rounded text-[9px] font-medium bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">
                Active
              </span>
            )}
            {item.confidence && (
              <span className="text-[10px] text-stone-400 capitalize">
                • {item.confidence} confidence
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
              <span>Ready to resume</span>
            )}
            {item.itemCount !== undefined && (
              <span className="text-stone-500">• {item.itemCount} items</span>
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
