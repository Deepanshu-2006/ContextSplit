import { useState, useRef, useEffect } from 'react';
import {
  FolderPlus,
  XCircle,
  Trash2,
  X,
  ChevronDown,
  Plus,
  Loader2,
  BoxSelect,
  Search,
  Check,
  CheckCheck,
} from 'lucide-react';
import { db, Workspace } from '../../services/db';
import { useWorkspaceStore } from '../../store/workspaceStore';

interface BulkActionsBarProps {
  selectedCount: number;
  totalCount: number;
  workspaces: Workspace[];
  isProcessing: boolean;
  onSelectAll: () => void;
  onClearSelection: () => void;
  onBulkAssign: (workspaceId: string) => Promise<void>;
  onBulkDismiss: () => Promise<void>;
  onBulkClear: () => Promise<void>;
}

export function BulkActionsBar({
  selectedCount,
  totalCount,
  workspaces,
  isProcessing,
  onSelectAll,
  onClearSelection,
  onBulkAssign,
  onBulkDismiss,
  onBulkClear,
}: BulkActionsBarProps) {
  const [workspaceMenuOpen, setWorkspaceMenuOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [isCreatingNew, setIsCreatingNew] = useState(false);
  const [newWsName, setNewWsName] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Close workspace menu on click outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setWorkspaceMenuOpen(false);
        setIsCreatingNew(false);
      }
    }
    if (workspaceMenuOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [workspaceMenuOpen]);

  // Focus input when creating new workspace
  useEffect(() => {
    if (isCreatingNew && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isCreatingNew]);

  if (selectedCount === 0) return null;

  const handleAssignWorkspace = async (wsId: string) => {
    try {
      setIsSubmitting(true);
      await onBulkAssign(wsId);
      setWorkspaceMenuOpen(false);
    } catch (err) {
      console.error('Failed to assign items to workspace:', err);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleCreateAndAssign = async () => {
    if (!newWsName.trim()) return;
    try {
      setIsSubmitting(true);
      const newWs = await db.createWorkspace(newWsName.trim());
      useWorkspaceStore.setState((s) => ({
        workspaces: [newWs, ...s.workspaces],
      }));
      await onBulkAssign(newWs.id);
      setWorkspaceMenuOpen(false);
      setIsCreatingNew(false);
      setNewWsName('');
    } catch (err) {
      console.error('Failed to create and assign workspace:', err);
    } finally {
      setIsSubmitting(false);
    }
  };

  const filteredWorkspaces = workspaces.filter((ws) =>
    ws.name.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-[9990] flex items-center gap-2 bg-gray-950/90 backdrop-blur-2xl border border-white/10 text-white pl-3 pr-2 py-1.5 rounded-full shadow-[0_16px_40px_-6px_rgba(0,0,0,0.4)] animate-in fade-in slide-in-from-bottom-4 duration-200 select-none font-sans">
      {/* Selected Counter & Select All Button */}
      <div className="flex items-center gap-2 pr-2.5 border-r border-white/15">
        <div className="flex items-center gap-1.5 pl-0.5">
          <span className="flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded-full bg-white/15 text-[11px] font-semibold text-white leading-none">
            {selectedCount}
          </span>
          <span className="text-[12px] font-medium text-gray-300">selected</span>
        </div>

        {selectedCount < totalCount ? (
          <button
            onClick={onSelectAll}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-white/10 hover:bg-white/18 active:bg-white/25 border border-white/15 text-white text-[11.5px] font-medium transition-all shadow-2xs cursor-pointer group whitespace-nowrap"
          >
            <CheckCheck size={12} className="text-gray-400 group-hover:text-white transition-colors" />
            <span>Select all ({totalCount})</span>
          </button>
        ) : (
          <button
            onClick={onClearSelection}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-white/10 hover:bg-white/18 active:bg-white/25 border border-white/15 text-white text-[11.5px] font-medium transition-all shadow-2xs cursor-pointer group whitespace-nowrap"
          >
            <X size={12} className="text-gray-400 group-hover:text-white transition-colors" />
            <span>Deselect all</span>
          </button>
        )}
      </div>

      {/* Action Buttons Group */}
      <div className="flex items-center gap-1 relative" ref={menuRef}>
        {/* 1. Add to Workspace (Primary Pill) */}
        <button
          disabled={isProcessing || isSubmitting}
          onClick={() => setWorkspaceMenuOpen((v) => !v)}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-white text-gray-950 hover:bg-gray-100 rounded-full text-[12px] font-semibold shadow-xs transition-all cursor-pointer disabled:opacity-50 whitespace-nowrap"
        >
          {isProcessing || isSubmitting ? (
            <Loader2 size={12} className="animate-spin" />
          ) : (
            <FolderPlus size={13} className="text-gray-900" />
          )}
          <span>Add to Workspace</span>
          <ChevronDown size={11} className="text-gray-500" />
        </button>

        {/* Workspace Quick Dropdown Menu */}
        {workspaceMenuOpen && (
          <div className="absolute bottom-full left-0 mb-2.5 w-64 bg-white text-gray-900 rounded-2xl shadow-[0_16px_48px_-8px_rgba(0,0,0,0.25)] border border-gray-200/80 py-2 flex flex-col gap-1 select-none z-[9999] animate-in fade-in zoom-in-95 duration-150">
            <div className="px-3.5 pt-1 pb-1.5 border-b border-gray-100 flex items-center justify-between">
              <span className="text-[11.5px] font-semibold text-gray-700">Choose Workspace</span>
              <button
                onClick={() => setWorkspaceMenuOpen(false)}
                className="text-gray-400 hover:text-gray-700 p-0.5 rounded-md hover:bg-gray-100 transition-colors"
              >
                <X size={12} />
              </button>
            </div>

            {/* Search */}
            {workspaces.length > 4 && (
              <div className="px-2.5 pt-1">
                <div className="relative flex items-center">
                  <Search size={11} className="absolute left-2.5 text-gray-400" />
                  <input
                    type="text"
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    placeholder="Search workspaces..."
                    className="w-full bg-gray-50 border border-gray-200 rounded-lg py-1 pl-6 pr-2 text-[11.5px] focus:outline-none focus:bg-white focus:ring-1 focus:ring-indigo-500"
                  />
                </div>
              </div>
            )}

            {/* Workspaces List */}
            <div className="px-1.5 max-h-48 overflow-y-auto pt-1">
              {filteredWorkspaces.map((ws) => (
                <button
                  key={ws.id}
                  disabled={isSubmitting}
                  onClick={() => handleAssignWorkspace(ws.id)}
                  className="w-full text-left px-2.5 py-1.5 text-[12px] text-gray-700 hover:bg-gray-100 rounded-lg flex items-center gap-2 transition-colors cursor-pointer"
                >
                  <BoxSelect size={13} className="text-gray-400" />
                  <span className="truncate font-medium">{ws.name}</span>
                </button>
              ))}

              {filteredWorkspaces.length === 0 && !isCreatingNew && (
                <p className="px-3 py-2 text-[11.5px] text-gray-400">No matching workspace.</p>
              )}
            </div>

            {/* Create New Workspace */}
            <div className="border-t border-gray-100 px-2 pt-1.5 pb-0.5">
              {isCreatingNew ? (
                <div className="flex flex-col gap-1.5">
                  <input
                    ref={inputRef}
                    type="text"
                    value={newWsName}
                    onChange={(e) => setNewWsName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleCreateAndAssign();
                      if (e.key === 'Escape') setIsCreatingNew(false);
                    }}
                    placeholder="New workspace name..."
                    className="w-full bg-gray-50 border border-indigo-200 rounded-lg py-1 px-2 text-[12px] focus:outline-none focus:ring-1 focus:ring-indigo-500"
                  />
                  <div className="flex items-center gap-1.5">
                    <button
                      disabled={isSubmitting || !newWsName.trim()}
                      onClick={handleCreateAndAssign}
                      className="flex-1 flex items-center justify-center gap-1 py-1 bg-gray-900 hover:bg-black text-white rounded-lg text-[11px] font-medium transition-colors cursor-pointer disabled:opacity-50"
                    >
                      {isSubmitting ? (
                        <Loader2 size={11} className="animate-spin" />
                      ) : (
                        <Check size={11} />
                      )}
                      <span>Create & Assign</span>
                    </button>
                    <button
                      onClick={() => setIsCreatingNew(false)}
                      className="px-2 py-1 text-gray-500 hover:text-gray-800 rounded-lg text-[11px] hover:bg-gray-100 transition-colors"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              ) : (
                <button
                  onClick={() => setIsCreatingNew(true)}
                  className="w-full text-left px-2 py-1.5 text-[12px] font-medium text-indigo-600 hover:bg-indigo-50/60 rounded-lg flex items-center gap-2 transition-colors cursor-pointer"
                >
                  <Plus size={13} />
                  <span>Create New Workspace</span>
                </button>
              )}
            </div>
          </div>
        )}

        {/* 2. Dismiss Suggestions Button (Subtle Ghost Pill) */}
        <button
          disabled={isProcessing}
          onClick={onBulkDismiss}
          className="flex items-center gap-1.5 px-2.5 py-1.5 text-gray-300 hover:text-white hover:bg-white/10 rounded-full text-[12px] font-medium transition-colors cursor-pointer disabled:opacity-50 whitespace-nowrap"
          title="Dismiss suggestions for selected items"
        >
          <XCircle size={13} className="text-gray-400" />
          <span>Dismiss Suggestions</span>
        </button>

        {/* 3. Clear from Inbox Button (Subtle Danger Ghost Pill) */}
        <button
          disabled={isProcessing}
          onClick={onBulkClear}
          className="flex items-center gap-1.5 px-2.5 py-1.5 text-gray-300 hover:text-rose-300 hover:bg-rose-500/15 rounded-full text-[12px] font-medium transition-colors cursor-pointer disabled:opacity-50 whitespace-nowrap"
          title="Delete selected items from Inbox"
        >
          <Trash2 size={13} className="text-gray-400 hover:text-rose-300" />
          <span>Clear Items</span>
        </button>
      </div>

      {/* Dismiss Selection */}
      <button
        onClick={onClearSelection}
        className="p-1 text-gray-400 hover:text-white hover:bg-white/10 rounded-full transition-colors cursor-pointer ml-0.5"
        title="Clear Selection (Esc)"
      >
        <X size={13} />
      </button>
    </div>
  );
}
