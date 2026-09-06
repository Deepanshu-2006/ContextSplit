import { useState, useRef, useEffect, KeyboardEvent } from 'react';
import { createPortal } from 'react-dom';
import {
  Plus,
  BoxSelect,
  Sparkles,
  X,
  Search,
  Check,
  FolderPlus,
  Loader2,
} from 'lucide-react';
import { db, Workspace } from '../../services/db';
import { useWorkspaceStore } from '../../store/workspaceStore';

export interface QuickContextAssignmentMenuProps {
  itemId: string;
  suggestedWorkspaceId?: string | null;
  suggestedConfidence?: number | null;
  workspaces: Workspace[];
  isOpen: boolean;
  position: { top: number; left?: number; right?: number } | null;
  onClose: () => void;
  onAssignSuccess?: (workspaceId: string) => void;
}

export function QuickContextAssignmentMenu({
  itemId,
  suggestedWorkspaceId,
  suggestedConfidence,
  workspaces,
  isOpen,
  position,
  onClose,
  onAssignSuccess,
}: QuickContextAssignmentMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const [searchQuery, setSearchQuery] = useState('');
  const [isCreatingNew, setIsCreatingNew] = useState(false);
  const [newWsName, setNewWsName] = useState('');
  const [isLoading, setIsLoading] = useState(false);

  // Focus new workspace input when creating
  useEffect(() => {
    if (isCreatingNew && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isCreatingNew]);

  // Click outside to close
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        onClose();
      }
    }
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [isOpen, onClose]);

  if (!isOpen || !position) return null;

  const handleSelectWorkspace = async (targetWsId: string) => {
    try {
      setIsLoading(true);
      // 1. Create confirmed ContextMembership
      await db.confirmContextMembership(itemId, targetWsId);

      // 2. Update Inbox state to 'organized'
      try {
        await db.transitionInboxItemState(
          itemId,
          'organized',
          `Assigned to workspace: ${targetWsId}`
        );
      } catch (e) {}

      // 3. Record user feedback signal
      try {
        await db.createContextSignal(
          targetWsId,
          itemId,
          'user_action',
          null,
          JSON.stringify({ action: 'context_assigned', workspace_id: targetWsId })
        );
      } catch (e) {}

      // 4. Move item to workspace (preserves signals and relationships)
      await useWorkspaceStore.getState().moveItemToWorkspace(itemId, targetWsId);

      onAssignSuccess?.(targetWsId);
      onClose();
    } catch (err) {
      console.error('Failed to assign context:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleCreateAndAssignWorkspace = async () => {
    if (!newWsName.trim()) return;
    try {
      setIsLoading(true);
      // Create new workspace using existing DB & store pattern
      const newWs = await db.createWorkspace(newWsName.trim());
      useWorkspaceStore.setState((s) => ({
        workspaces: [newWs, ...s.workspaces],
      }));

      // Directly assign the item to this newly created workspace
      await handleSelectWorkspace(newWs.id);
    } catch (err) {
      console.error('Failed to create new workspace:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      handleCreateAndAssignWorkspace();
    } else if (e.key === 'Escape') {
      setIsCreatingNew(false);
      setNewWsName('');
    }
  };

  const filteredWorkspaces = workspaces.filter((ws) =>
    ws.name.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const suggestedWs = workspaces.find((w) => w.id === suggestedWorkspaceId);

  return createPortal(
    <div
      ref={menuRef}
      style={{
        top: position.top,
        left:
          position.left !== undefined
            ? Math.max(12, Math.min(position.left, window.innerWidth - 272))
            : undefined,
        right: position.right !== undefined ? Math.max(12, position.right) : undefined,
      }}
      className="fixed w-64 bg-white rounded-xl shadow-2xl border border-gray-200/95 py-2 z-[9999] overflow-hidden flex flex-col gap-1 select-none animate-in fade-in zoom-in-95 duration-100"
    >
      {/* Menu Header */}
      <div className="flex items-center justify-between px-3 pt-1 pb-1.5 border-b border-black/4">
        <div className="flex items-center gap-1.5 text-[11px] font-semibold text-gray-700">
          <FolderPlus size={13} className="text-indigo-600" />
          <span>Add to Context</span>
        </div>
        <button
          onClick={onClose}
          className="p-1 text-gray-400 hover:text-gray-700 hover:bg-black/5 rounded transition-colors cursor-pointer"
          title="Cancel"
        >
          <X size={12} />
        </button>
      </div>

      {/* Suggested Context Quick Pick (if present) */}
      {suggestedWs && (
        <div className="px-2 pt-1">
          <div className="text-[10px] font-bold text-indigo-800 uppercase tracking-wider px-2 py-0.5">
            Recommended Context
          </div>
          <button
            disabled={isLoading}
            onClick={() => handleSelectWorkspace(suggestedWs.id)}
            className="w-full text-left px-2.5 py-1.5 bg-indigo-50/80 hover:bg-indigo-100/90 text-indigo-950 rounded-lg flex items-center justify-between border border-indigo-200/80 transition-colors cursor-pointer group"
          >
            <div className="flex items-center gap-2 min-w-0">
              <Sparkles size={13} className="text-indigo-600 shrink-0" />
              <span className="text-[12px] font-semibold truncate">{suggestedWs.name}</span>
            </div>
            {suggestedConfidence !== undefined && suggestedConfidence !== null && (
              <span className="text-[10px] font-bold text-indigo-700 bg-white/90 px-1.5 py-0.5 rounded shrink-0">
                {Math.round(suggestedConfidence * 100)}%
              </span>
            )}
          </button>
        </div>
      )}

      {/* Search Filter for Workspaces */}
      {workspaces.length > 4 && (
        <div className="px-2.5 pt-1">
          <div className="relative flex items-center">
            <Search size={11} className="absolute left-2 text-gray-400 pointer-events-none" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search contexts..."
              className="w-full bg-gray-100/70 hover:bg-gray-100 border border-black/5 rounded-md py-1 pl-6 pr-2 text-[11px] placeholder-gray-400 focus:outline-none focus:bg-white focus:ring-1 focus:ring-black/10"
            />
          </div>
        </div>
      )}

      {/* Available Workspaces List */}
      <div className="px-1.5 max-h-48 overflow-y-auto pt-1">
        <div className="text-[10px] font-semibold text-gray-400 uppercase tracking-wider px-2 py-0.5">
          All Contexts
        </div>

        {filteredWorkspaces.map((ws) => (
          <button
            key={ws.id}
            disabled={isLoading}
            onClick={() => handleSelectWorkspace(ws.id)}
            className="w-full text-left px-2 py-1.5 text-[12px] text-gray-700 hover:bg-gray-100/80 hover:text-gray-900 rounded-md flex items-center gap-2 transition-colors cursor-pointer"
          >
            <BoxSelect size={13} className="text-gray-400 shrink-0" />
            <span className="truncate flex-1">{ws.name}</span>
          </button>
        ))}

        {filteredWorkspaces.length === 0 && !isCreatingNew && (
          <p className="px-2 py-1.5 text-[11px] text-gray-400">No matching context.</p>
        )}
      </div>

      {/* Create New Workspace Form */}
      <div className="border-t border-black/4 px-2 pt-1.5 pb-1">
        {isCreatingNew ? (
          <div className="flex flex-col gap-1.5">
            <input
              ref={inputRef}
              type="text"
              value={newWsName}
              onChange={(e) => setNewWsName(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="New context name..."
              className="w-full bg-gray-50 border border-indigo-200 rounded-md py-1 px-2 text-[12px] text-gray-900 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            />
            <div className="flex items-center gap-1.5">
              <button
                disabled={isLoading || !newWsName.trim()}
                onClick={handleCreateAndAssignWorkspace}
                className="flex-1 flex items-center justify-center gap-1 py-1 bg-indigo-600 hover:bg-indigo-700 text-white rounded text-[11px] font-semibold transition-colors cursor-pointer disabled:opacity-50"
              >
                {isLoading ? <Loader2 size={11} className="animate-spin" /> : <Check size={11} />}
                <span>Create & Add</span>
              </button>
              <button
                onClick={() => {
                  setIsCreatingNew(false);
                  setNewWsName('');
                }}
                className="px-2 py-1 text-gray-500 hover:text-gray-800 hover:bg-black/5 rounded text-[11px] transition-colors cursor-pointer"
              >
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <button
            disabled={isLoading}
            onClick={() => setIsCreatingNew(true)}
            className="w-full text-left px-2 py-1.5 text-[12px] font-medium text-indigo-700 hover:bg-indigo-50 rounded-md flex items-center gap-2 transition-colors cursor-pointer"
          >
            <Plus size={13} className="text-indigo-600" />
            <span>Create New Context</span>
          </button>
        )}
      </div>
    </div>,
    document.body
  );
}
