import { useState, useRef, useEffect, memo } from 'react';
import { createPortal } from 'react-dom';
import {
  FileText,
  Image as ImageIcon,
  Link2,
  FileCode,
  MoreVertical,
  Check,
  ExternalLink,
  Copy,
  Trash2,
  FolderPlus,
  BoxSelect,
  Layers,
  Folder,
} from 'lucide-react';
import { db, Workspace } from '../../services/db';
import { DerivedInboxItem, useInboxStore } from '../../store/inboxStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { toast } from 'sonner';

import { showUndoToast } from './UndoToast';

interface InboxItemCardProps {
  derived: DerivedInboxItem;
  workspaces: Workspace[];
  onSelectItem?: (itemId: string) => void;
  onMoveToWorkspace: (itemId: string, workspaceId: string) => void;
}

function InboxItemCardComponent({
  derived,
  workspaces,
  onSelectItem,
  onMoveToWorkspace,
}: InboxItemCardProps) {
  const { item, attention, candidateInfo } = derived;
  const isSelected = useInboxStore((s) => s.selectedItemIds.includes(derived.item.id));
  const isMultiSelectActive = useInboxStore((s) => s.isMultiSelectActive);
  const isFocused = useInboxStore((s) => s.focusedItemId === derived.item.id);
  const toggleSelectItem = useInboxStore((s) => s.toggleSelectItem);
  const setFocusedItemId = useInboxStore((s) => s.setFocusedItemId);

  const setIsDraggingCard = useWorkspaceStore((s) => s.setIsDraggingCard);
  const setHoveredWorkspaceId = useWorkspaceStore((s) => s.setHoveredWorkspaceId);

  const [menuOpen, setMenuOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ top: number; right: number } | null>(null);
  const menuBtnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  // Close options dropdown on click outside
  useEffect(() => {
    function handleClickOutside(event: globalThis.MouseEvent) {
      if (
        menuRef.current &&
        !menuRef.current.contains(event.target as Node) &&
        menuBtnRef.current &&
        !menuBtnRef.current.contains(event.target as Node)
      ) {
        setMenuOpen(false);
      }
    }
    if (menuOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [menuOpen]);

  const openMenu = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (menuBtnRef.current) {
      const rect = menuBtnRef.current.getBoundingClientRect();
      setMenuPos({ top: rect.bottom + 4, right: window.innerWidth - rect.right });
    }
    setMenuOpen((v) => !v);
  };

  const handleCopy = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setMenuOpen(false);
    try {
      await navigator.clipboard.writeText(item.content);
      toast.success('Copied to clipboard');
    } catch {
      toast.error('Could not copy to clipboard');
    }
  };

  const handleDelete = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setMenuOpen(false);
    const itemToRestore = { ...item };
    try {
      await db.deleteItem(item.id);
      const currentActive = useWorkspaceStore.getState().activeItems;
      useWorkspaceStore.setState({
        activeItems: currentActive.filter((i) => i.id !== item.id),
        inboxCount: Math.max(0, useWorkspaceStore.getState().inboxCount - 1),
      });
      showUndoToast({
        message: 'Item deleted',
        onUndo: async () => {
          try {
            const restored = await db.createItem(
              itemToRestore.workspace_id,
              itemToRestore.item_type,
              itemToRestore.content,
              itemToRestore.title,
              itemToRestore.metadata
            );
            useWorkspaceStore.setState((s) => ({
              activeItems: [restored, ...s.activeItems],
              inboxCount: s.inboxCount + (itemToRestore.workspace_id ? 0 : 1),
            }));
            toast.success('Item restored');
          } catch {
            toast.error('Failed to restore item');
          }
        },
      });
    } catch {
      toast.error('Failed to delete item');
    }
  };

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (item.item_type === 'url') {
      window.open(item.content, '_blank');
    } else {
      onSelectItem?.(item.id);
    }
  };

  // Format relative timestamp
  const getRelativeTime = (dateStr: string) => {
    try {
      const date = new Date(dateStr);
      const now = new Date();
      const diffMs = now.getTime() - date.getTime();
      const diffMins = Math.floor(diffMs / (1000 * 60));
      const diffHours = Math.floor(diffMins / 60);

      if (diffMins < 1) return 'Just now';
      if (diffMins < 60) return `${diffMins}m ago`;
      if (diffHours < 24) return `${diffHours}h ago`;

      const isToday = date.toDateString() === now.toDateString();
      if (isToday) {
        return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
      }
      return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
    } catch {
      return '';
    }
  };

  // Generate clean snippet and title without clutter
  let displayTitle = item.title;
  let snippet = '';

  if (item.item_type === 'url') {
    try {
      const url = new URL(item.content);
      displayTitle = item.title || `Link from ${url.hostname.replace(/^www\./, '')}`;
      snippet = url.hostname + (url.pathname !== '/' ? url.pathname : '');
    } catch {
      displayTitle = item.title || 'Link';
      snippet = item.content;
    }
  } else if (item.item_type === 'image') {
    displayTitle = item.title || 'Image capture';
    try {
      const meta = JSON.parse(item.metadata || '{}');
      if (meta.file_size || meta.dimensions) {
        snippet = [meta.dimensions, meta.file_size].filter(Boolean).join(' · ');
      } else {
        snippet = '';
      }
    } catch {
      snippet = '';
    }
  } else {
    // Text / Note
    const lines = item.content.trim().split('\n');
    if (!displayTitle) {
      displayTitle = lines[0] || 'Note';
      snippet = lines.slice(1).join(' ').trim();
    } else {
      snippet = lines.join(' ').trim();
    }
  }

  // Refined context badge (neatly aligned and formatted)
  const suggestedWsId = attention?.factors?.suggested_workspace_id;
  const suggestedWs = workspaces.find((w) => w.id === suggestedWsId);
  const currentWs = workspaces.find((w) => w.id === item.workspace_id);

  const renderContextPill = () => {
    if (currentWs) {
      return (
        <span className="inline-flex items-center gap-1.5 text-[11px] font-medium text-gray-700 bg-gray-100/90 border border-gray-200/70 px-2.5 py-0.5 rounded-full shrink-0">
          <Folder size={11} className="text-gray-400" />
          <span className="truncate max-w-[140px]">{currentWs.name}</span>
        </span>
      );
    }
    if (suggestedWs) {
      return (
        <span className="inline-flex items-center gap-1.5 text-[11px] font-medium text-indigo-700 bg-indigo-50/80 border border-indigo-200/60 px-2.5 py-0.5 rounded-full shrink-0">
          <Layers size={11} className="text-indigo-500" />
          <span className="truncate max-w-[140px]">{suggestedWs.name}</span>
        </span>
      );
    }
    if (candidateInfo) {
      return (
        <span className="inline-flex items-center gap-1.5 text-[11px] font-medium text-gray-700 bg-gray-100/90 hover:bg-gray-200/70 border border-gray-200/60 px-2.5 py-0.5 rounded-full shrink-0 transition-colors">
          <Layers size={11} className="text-indigo-500" />
          <span className="truncate max-w-[140px]">{candidateInfo.candidateName}</span>
        </span>
      );
    }
    if (attention?.factors?.is_ambiguous) {
      return (
        <span className="inline-flex items-center gap-1.5 text-[11px] font-medium text-amber-800 bg-amber-50/90 border border-amber-200/60 px-2.5 py-0.5 rounded-full shrink-0">
          <span className="w-1.5 h-1.5 rounded-full bg-amber-500 shrink-0" />
          <span>Needs review</span>
        </span>
      );
    }
    return null;
  };

  const getItemIcon = () => {
    switch (item.item_type) {
      case 'image':
        return (
          <div className="w-7 h-7 rounded-lg bg-purple-50 text-purple-600 flex items-center justify-center shrink-0 border border-purple-100/60 group-hover:bg-purple-100/60 transition-colors">
            <ImageIcon size={13} />
          </div>
        );
      case 'url':
        return (
          <div className="w-7 h-7 rounded-lg bg-blue-50 text-blue-600 flex items-center justify-center shrink-0 border border-blue-100/60 group-hover:bg-blue-100/60 transition-colors">
            <Link2 size={13} />
          </div>
        );
      case 'file':
        return (
          <div className="w-7 h-7 rounded-lg bg-emerald-50 text-emerald-600 flex items-center justify-center shrink-0 border border-emerald-100/60 group-hover:bg-emerald-100/60 transition-colors">
            <FileCode size={13} />
          </div>
        );
      default:
        return (
          <div className="w-7 h-7 rounded-lg bg-gray-100 text-gray-600 flex items-center justify-center shrink-0 border border-gray-200/60 group-hover:bg-gray-200/60 transition-colors">
            <FileText size={13} />
          </div>
        );
    }
  };

  return (
    <div
      data-item-id={item.id}
      draggable
      onDragStart={(e) => {
        setIsDraggingCard(true);
        e.dataTransfer.setData('text/plain', item.id);
        e.dataTransfer.setData('application/contextsplit-item-id', item.id);
      }}
      onDragEnd={() => {
        setIsDraggingCard(false);
        setHoveredWorkspaceId(null);
      }}
      onClick={() => {
        setFocusedItemId(item.id);
        onSelectItem?.(item.id);
      }}
      onDoubleClick={handleDoubleClick}
      onContextMenu={(e) => openMenu(e)}
      className={`group relative flex items-center justify-between px-3.5 py-2.5 bg-white hover:bg-gray-50/80 border-b border-gray-100 transition-colors duration-100 select-none cursor-pointer ${
        isSelected
          ? 'bg-indigo-50/40'
          : isFocused
          ? 'bg-gray-50 ring-1 ring-black/10'
          : ''
      }`}
    >
      {/* Left Column: Selection Checkbox + Styled Type Icon Badge + Title & Snippet */}
      <div className="flex items-center gap-3 min-w-0 flex-1 pr-4">
        {/* Selection Checkbox */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            toggleSelectItem(item.id);
          }}
          className={`w-4 h-4 rounded border flex items-center justify-center transition-all cursor-pointer shrink-0 ${
            isSelected
              ? 'bg-gray-950 border-gray-950 text-white'
              : 'border-gray-200 hover:border-gray-400 bg-white opacity-0 group-hover:opacity-100'
          } ${isMultiSelectActive ? '!opacity-100' : ''}`}
          title={isSelected ? 'Deselect' : 'Select (X)'}
        >
          {isSelected && <Check size={10} strokeWidth={3} />}
        </button>

        {/* Content Icon Badge */}
        {getItemIcon()}

        {/* Title & Optional Snippet */}
        <div className="flex flex-col min-w-0 flex-1 justify-center">
          <span className="text-[13px] font-medium text-gray-900 group-hover:text-black truncate tracking-tight">
            {displayTitle}
          </span>

          {snippet ? (
            <p className="text-[11.5px] text-gray-400 truncate leading-normal mt-0.5">
              {snippet}
            </p>
          ) : null}
        </div>
      </div>

      {/* Center/Right: Neatly Aligned Context Chip */}
      <div className="flex items-center shrink-0 px-2">
        {renderContextPill()}
      </div>

      {/* Far Right: Relative Timestamp & Subtle Hover Menu */}
      <div className="flex items-center gap-2 shrink-0 pl-2">
        <span className="text-[11.5px] text-gray-400 font-normal tracking-tight w-14 text-right">
          {getRelativeTime(item.created_at)}
        </span>

        <button
          ref={menuBtnRef}
          onClick={openMenu}
          className={`p-1 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-md transition-all cursor-pointer ${
            menuOpen ? 'opacity-100 bg-gray-100 text-gray-700' : 'opacity-0 group-hover:opacity-100'
          }`}
          title="More options"
        >
          <MoreVertical size={13} />
        </button>
      </div>

      {/* Options Menu Dropdown Portal */}
      {menuOpen && menuPos && createPortal(
        <div
          ref={menuRef}
          style={{ top: menuPos.top, right: menuPos.right }}
          className="fixed w-48 bg-white rounded-xl shadow-xl border border-gray-200/80 py-1.5 z-50 text-[12px] overflow-hidden animate-in fade-in-0 duration-100 select-none"
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen(false);
              onSelectItem?.(item.id);
            }}
            className="w-full text-left px-3 py-1.5 text-gray-700 hover:bg-gray-100 flex items-center gap-2 transition-colors cursor-pointer"
          >
            <FolderPlus size={13} className="text-gray-500" />
            <span>View Context Details</span>
          </button>

          <button
            onClick={handleCopy}
            className="w-full text-left px-3 py-1.5 text-gray-700 hover:bg-gray-100 flex items-center gap-2 transition-colors cursor-pointer"
          >
            <Copy size={13} className="text-gray-500" />
            <span>Copy Content</span>
          </button>

          {item.item_type === 'url' && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                setMenuOpen(false);
                window.open(item.content, '_blank');
              }}
              className="w-full text-left px-3 py-1.5 text-gray-700 hover:bg-gray-100 flex items-center gap-2 transition-colors cursor-pointer"
            >
              <ExternalLink size={13} className="text-gray-500" />
              <span>Open in Browser</span>
            </button>
          )}

          <div className="my-1 border-t border-gray-100" />

          <div className="px-3 py-0.5 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
            Move to Workspace
          </div>
          {workspaces.map((ws) => (
            <button
              key={ws.id}
              onClick={(e) => {
                e.stopPropagation();
                setMenuOpen(false);
                onMoveToWorkspace(item.id, ws.id);
              }}
              className="w-full text-left px-3 py-1.5 text-gray-700 hover:bg-gray-100 flex items-center gap-2 transition-colors cursor-pointer"
            >
              <BoxSelect size={12} className="text-gray-400" />
              <span className="truncate">{ws.name}</span>
            </button>
          ))}

          <div className="my-1 border-t border-gray-100" />

          <button
            onClick={handleDelete}
            className="w-full text-left px-3 py-1.5 text-rose-600 hover:bg-rose-50 flex items-center gap-2 transition-colors cursor-pointer"
          >
            <Trash2 size={13} className="text-rose-500" />
            <span>Delete Item</span>
          </button>
        </div>,
        document.body
      )}
    </div>
  );
}

export const InboxItemCard = memo(InboxItemCardComponent);
