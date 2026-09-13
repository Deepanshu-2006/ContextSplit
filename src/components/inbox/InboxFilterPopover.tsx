import { useState, useRef, useEffect } from 'react';
import { Filter, Check } from 'lucide-react';
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

  const toggleFilterState = (state: string | null) => {
    setFilterInboxState(filterInboxState === state ? null : state);
  };

  const toggleItemType = (type: string | null) => {
    setFilterItemType(filterItemType === type ? null : type);
  };

  return (
    <div className="relative inline-block">
      <button
        ref={buttonRef}
        onClick={() => setIsOpen(!isOpen)}
        className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-[12px] font-medium transition-colors cursor-pointer border ${
          activeCount > 0
            ? 'bg-black/5 text-gray-900 border-black/15 font-semibold'
            : 'bg-white hover:bg-gray-50 text-gray-600 hover:text-gray-900 border-black/8'
        }`}
        title="Filter items"
      >
        <Filter size={13} className={activeCount > 0 ? 'text-gray-900' : 'text-gray-400'} />
        <span>Filter</span>
        {activeCount > 0 && (
          <span className="w-1.5 h-1.5 rounded-full bg-indigo-600 shrink-0" />
        )}
      </button>

      {isOpen && (
        <div
          ref={popoverRef}
          className="absolute right-0 mt-1.5 w-56 bg-white rounded-xl shadow-xl border border-black/10 py-2 z-30 flex flex-col gap-1 text-[12px] animate-in fade-in-0 duration-100"
        >
          {/* Header */}
          <div className="px-3 py-1 flex items-center justify-between border-b border-black/5 pb-1.5">
            <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
              Filters
            </span>
            {activeCount > 0 && (
              <button
                onClick={() => {
                  resetAllFilters();
                  setIsOpen(false);
                }}
                className="text-[11px] text-gray-400 hover:text-gray-800 transition-colors cursor-pointer"
              >
                Reset
              </button>
            )}
          </div>

          {/* Status Filters */}
          <div className="px-3 pt-1 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
            Status
          </div>
          <button
            onClick={() => toggleFilterState('needs_review')}
            className="flex items-center justify-between px-3 py-1 text-gray-700 hover:bg-black/5 transition-colors cursor-pointer text-left"
          >
            <span>Needs Review</span>
            {filterInboxState === 'needs_review' && <Check size={12} className="text-indigo-600" />}
          </button>
          <button
            onClick={() => toggleFilterState('suggested')}
            className="flex items-center justify-between px-3 py-1 text-gray-700 hover:bg-black/5 transition-colors cursor-pointer text-left"
          >
            <span>Suggested</span>
            {filterInboxState === 'suggested' && <Check size={12} className="text-indigo-600" />}
          </button>
          <button
            onClick={() => toggleFilterState('understood')}
            className="flex items-center justify-between px-3 py-1 text-gray-700 hover:bg-black/5 transition-colors cursor-pointer text-left"
          >
            <span>Understood</span>
            {filterInboxState === 'understood' && <Check size={12} className="text-indigo-600" />}
          </button>

          <div className="my-1 border-t border-black/5" />

          {/* Type Filters */}
          <div className="px-3 pt-0.5 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
            Type
          </div>
          <button
            onClick={() => toggleItemType('text')}
            className="flex items-center justify-between px-3 py-1 text-gray-700 hover:bg-black/5 transition-colors cursor-pointer text-left"
          >
            <span>Text & Notes</span>
            {filterItemType === 'text' && <Check size={12} className="text-indigo-600" />}
          </button>
          <button
            onClick={() => toggleItemType('image')}
            className="flex items-center justify-between px-3 py-1 text-gray-700 hover:bg-black/5 transition-colors cursor-pointer text-left"
          >
            <span>Images</span>
            {filterItemType === 'image' && <Check size={12} className="text-indigo-600" />}
          </button>
          <button
            onClick={() => toggleItemType('url')}
            className="flex items-center justify-between px-3 py-1 text-gray-700 hover:bg-black/5 transition-colors cursor-pointer text-left"
          >
            <span>Links</span>
            {filterItemType === 'url' && <Check size={12} className="text-indigo-600" />}
          </button>
          <button
            onClick={() => toggleItemType('file')}
            className="flex items-center justify-between px-3 py-1 text-gray-700 hover:bg-black/5 transition-colors cursor-pointer text-left"
          >
            <span>Files</span>
            {filterItemType === 'file' && <Check size={12} className="text-indigo-600" />}
          </button>

          {workspaces.length > 0 && (
            <>
              <div className="my-1 border-t border-black/5" />
              <div className="px-3 pt-0.5 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
                Workspace
              </div>
              {workspaces.map((ws) => (
                <button
                  key={ws.id}
                  onClick={() =>
                    setFilterWorkspaceId(filterWorkspaceId === ws.id ? null : ws.id)
                  }
                  className="flex items-center justify-between px-3 py-1 text-gray-700 hover:bg-black/5 transition-colors cursor-pointer text-left"
                >
                  <span className="truncate">{ws.name}</span>
                  {filterWorkspaceId === ws.id && (
                    <Check size={12} className="text-indigo-600 shrink-0" />
                  )}
                </button>
              ))}
            </>
          )}
        </div>
      )}
    </div>
  );
}
