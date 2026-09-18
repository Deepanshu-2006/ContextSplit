import { useState, useRef, useEffect } from 'react';
import {
  Filter,
  Check,
  FileText,
  Image as ImageIcon,
  Link2,
  FileCode,
  Folder,
  RotateCcw,
} from 'lucide-react';
import { useInboxStore } from '../../store/inboxStore';
import { Workspace } from '../../services/db';

interface InboxFilterPopoverProps {
  workspaces?: Workspace[];
}

export function InboxFilterPopover({ workspaces = [] }: InboxFilterPopoverProps) {
  const [isOpen, setIsOpen] = useState(false);
  const popoverRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const {
    filterItemType,
    filterInboxState,
    filterWorkspaceId,
    setFilterItemType,
    setFilterInboxState,
    setFilterWorkspaceId,
    resetAllFilters,
    getActiveFilterCount,
  } = useInboxStore();

  const activeCount = getActiveFilterCount();

  // Close when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (
        popoverRef.current &&
        !popoverRef.current.contains(event.target as Node) &&
        buttonRef.current &&
        !buttonRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    }
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [isOpen]);

  // Close on Escape key
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape' && isOpen) {
        setIsOpen(false);
      }
    }
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen]);

  const toggleFilterState = (state: string | null) => {
    setFilterInboxState(filterInboxState === state ? null : state);
  };

  const toggleItemType = (type: string | null) => {
    setFilterItemType(filterItemType === type ? null : type);
  };

  const toggleWorkspace = (wsId: string) => {
    setFilterWorkspaceId(filterWorkspaceId === wsId ? null : wsId);
  };

  return (
    <div className="relative inline-block text-left select-none">
      <button
        ref={buttonRef}
        onClick={() => setIsOpen(!isOpen)}
        className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-[12px] font-medium transition-all cursor-pointer border ${
          activeCount > 0
            ? 'bg-zinc-900 hover:bg-black text-white border-zinc-900 shadow-2xs'
            : 'bg-white hover:bg-zinc-50 text-zinc-700 hover:text-zinc-900 border-zinc-200/90 shadow-2xs'
        }`}
        title="Filter inbox captures"
      >
        <Filter
          size={13}
          className={activeCount > 0 ? 'text-zinc-200' : 'text-zinc-400'}
        />
        <span>Filter</span>
        {activeCount > 0 && (
          <span className="ml-0.5 px-1.5 py-0.2 text-[10px] font-medium bg-white/20 text-white rounded-full leading-tight">
            {activeCount}
          </span>
        )}
      </button>

      {isOpen && (
        <div
          ref={popoverRef}
          className="absolute right-0 mt-1.5 w-60 bg-white border border-zinc-200/90 rounded-xl shadow-xl py-1.5 z-40 flex flex-col text-[12px] animate-in fade-in-0 zoom-in-95 duration-100"
        >
          {/* Top Bar Header with Clear Action */}
          <div className="px-3 py-1.5 flex items-center justify-between border-b border-zinc-100 mb-1">
            <span className="text-[11px] font-medium text-zinc-400">
              Filter by
            </span>
            {activeCount > 0 && (
              <button
                onClick={() => {
                  resetAllFilters();
                  setIsOpen(false);
                }}
                className="flex items-center gap-1 text-[11px] text-zinc-500 hover:text-zinc-900 transition-colors cursor-pointer"
              >
                <RotateCcw size={10} />
                <span>Reset</span>
              </button>
            )}
          </div>

          {/* Status Section */}
          <div className="px-3 py-1 text-[10px] font-semibold text-zinc-400 uppercase tracking-wider">
            Status
          </div>
          <div className="flex flex-col px-1 gap-0.5">
            <button
              onClick={() => toggleFilterState('needs_review')}
              className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg text-left transition-colors cursor-pointer ${
                filterInboxState === 'needs_review'
                  ? 'bg-zinc-100 font-medium text-zinc-900'
                  : 'text-zinc-700 hover:bg-zinc-50'
              }`}
            >
              <div className="flex items-center gap-2">
                <span className="w-1.5 h-1.5 rounded-full bg-amber-500 shrink-0" />
                <span>Needs Review</span>
              </div>
              {filterInboxState === 'needs_review' && (
                <Check size={13} className="text-zinc-700" />
              )}
            </button>

            <button
              onClick={() => toggleFilterState('suggested')}
              className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg text-left transition-colors cursor-pointer ${
                filterInboxState === 'suggested'
                  ? 'bg-zinc-100 font-medium text-zinc-900'
                  : 'text-zinc-700 hover:bg-zinc-50'
              }`}
            >
              <div className="flex items-center gap-2">
                <span className="w-1.5 h-1.5 rounded-full bg-indigo-500 shrink-0" />
                <span>Suggested</span>
              </div>
              {filterInboxState === 'suggested' && (
                <Check size={13} className="text-zinc-700" />
              )}
            </button>

            <button
              onClick={() => toggleFilterState('understood')}
              className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg text-left transition-colors cursor-pointer ${
                filterInboxState === 'understood'
                  ? 'bg-zinc-100 font-medium text-zinc-900'
                  : 'text-zinc-700 hover:bg-zinc-50'
              }`}
            >
              <div className="flex items-center gap-2">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 shrink-0" />
                <span>Understood</span>
              </div>
              {filterInboxState === 'understood' && (
                <Check size={13} className="text-zinc-700" />
              )}
            </button>
          </div>

          <div className="my-1.5 mx-2 h-px bg-zinc-100" />

          {/* Type Section */}
          <div className="px-3 py-1 text-[10px] font-semibold text-zinc-400 uppercase tracking-wider">
            Type
          </div>
          <div className="flex flex-col px-1 gap-0.5">
            <button
              onClick={() => toggleItemType('text')}
              className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg text-left transition-colors cursor-pointer ${
                filterItemType === 'text'
                  ? 'bg-zinc-100 font-medium text-zinc-900'
                  : 'text-zinc-700 hover:bg-zinc-50'
              }`}
            >
              <div className="flex items-center gap-2">
                <FileText size={13} className="text-zinc-400 shrink-0" />
                <span>Text & Notes</span>
              </div>
              {filterItemType === 'text' && (
                <Check size={13} className="text-zinc-700" />
              )}
            </button>

            <button
              onClick={() => toggleItemType('image')}
              className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg text-left transition-colors cursor-pointer ${
                filterItemType === 'image'
                  ? 'bg-zinc-100 font-medium text-zinc-900'
                  : 'text-zinc-700 hover:bg-zinc-50'
              }`}
            >
              <div className="flex items-center gap-2">
                <ImageIcon size={13} className="text-zinc-400 shrink-0" />
                <span>Images</span>
              </div>
              {filterItemType === 'image' && (
                <Check size={13} className="text-zinc-700" />
              )}
            </button>

            <button
              onClick={() => toggleItemType('url')}
              className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg text-left transition-colors cursor-pointer ${
                filterItemType === 'url'
                  ? 'bg-zinc-100 font-medium text-zinc-900'
                  : 'text-zinc-700 hover:bg-zinc-50'
              }`}
            >
              <div className="flex items-center gap-2">
                <Link2 size={13} className="text-zinc-400 shrink-0" />
                <span>Links</span>
              </div>
              {filterItemType === 'url' && (
                <Check size={13} className="text-zinc-700" />
              )}
            </button>

            <button
              onClick={() => toggleItemType('file')}
              className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg text-left transition-colors cursor-pointer ${
                filterItemType === 'file'
                  ? 'bg-zinc-100 font-medium text-zinc-900'
                  : 'text-zinc-700 hover:bg-zinc-50'
              }`}
            >
              <div className="flex items-center gap-2">
                <FileCode size={13} className="text-zinc-400 shrink-0" />
                <span>Files</span>
              </div>
              {filterItemType === 'file' && (
                <Check size={13} className="text-zinc-700" />
              )}
            </button>
          </div>

          {/* Workspaces Section */}
          {workspaces.length > 0 && (
            <>
              <div className="my-1.5 mx-2 h-px bg-zinc-100" />
              <div className="px-3 py-1 text-[10px] font-semibold text-zinc-400 uppercase tracking-wider">
                Workspace
              </div>
              <div className="flex flex-col px-1 gap-0.5 max-h-36 overflow-y-auto">
                {workspaces.map((ws) => (
                  <button
                    key={ws.id}
                    onClick={() => toggleWorkspace(ws.id)}
                    className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg text-left transition-colors cursor-pointer ${
                      filterWorkspaceId === ws.id
                        ? 'bg-zinc-100 font-medium text-zinc-900'
                        : 'text-zinc-700 hover:bg-zinc-50'
                    }`}
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <Folder size={13} className="text-zinc-400 shrink-0" />
                      <span className="truncate">{ws.name}</span>
                    </div>
                    {filterWorkspaceId === ws.id && (
                      <Check size={13} className="text-zinc-700 shrink-0 ml-2" />
                    )}
                  </button>
                ))}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
