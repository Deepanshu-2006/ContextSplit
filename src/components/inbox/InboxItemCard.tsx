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
} from 'lucide-react';
import { db, Workspace } from '../../services/db';
import { DerivedInboxItem, useInboxStore } from '../../store/inboxStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { toast } from 'sonner';

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
  const { item, attention, candidateInfo, inboxState } = derived;
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
    try {
      await db.deleteItem(item.id);
      const currentActive = useWorkspaceStore.getState().activeItems;
      useWorkspaceStore.setState({
        activeItems: currentActive.filter((i) => i.id !== item.id),
        inboxCount: Math.max(0, useWorkspaceStore.getState().inboxCount - 1),
      });
      toast.success('Item deleted');
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
      if (diffMins < 60) return `${diffMins} min ago`;
      if (diffHours < 24) return `${diffHours} hr ago`;

      // If older, show time of day if today, otherwise date
      const isToday = date.toDateString() === now.toDateString();
      if (isToday) {
        return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
      }
      return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
    } catch {
      return '';
    }
  };

  // Generate clean snippet and title
  let displayTitle = item.title;
  let snippet = '';

  if (item.item_type === 'url') {
    try {
      const url = new URL(item.content);
      displayTitle = item.title || `Link from ${url.hostname.replace(/^www\./, '')}`;
      snippet = url.hostname + url.pathname;
    } catch {
      displayTitle = item.title || 'Link';
      snippet = item.content;
    }
  } else if (item.item_type === 'image') {
    displayTitle = item.title || 'Image capture';
    try {
      const meta = JSON.parse(item.metadata || '{}');
      snippet = meta.file_size ? `Image · ${meta.file_size}` : 'Image';
    } catch {
      snippet = 'Image';
    }
  } else {
    // Text / Note
    const lines = item.content.trim().split('\n');
    if (!displayTitle) {
      displayTitle = lines[0] || 'Note';
      snippet = lines.slice(1).join(' ').trim() || lines[0];
    } else {
      snippet = lines.join(' ').trim();
    }
  }

  // Determine ONE subtle context pill
  const suggestedWsId = attention?.factors?.suggested_workspace_id;
  const suggestedWs = workspaces.find((w) => w.id === suggestedWsId);
  const currentWs = workspaces.find((w) => w.id === item.workspace_id);

  const isNeedsReview = Boolean(
    attention?.factors?.is_ambiguous || derived.section === 'needs_attention' || inboxState?.state === 'needs_review'
  );

  const renderContextPill = () => {
    if (currentWs) {
      return (
        <span className="inline-flex items-center gap-1 text-[11px] font-medium text-gray-700 bg-gray-100/90 px-2 py-0.5 rounded-md shrink-0 border border-black/5">
          {currentWs.name}
        </span>
      );
    }
    if (isNeedsReview) {
      return (
        <span className="inline-flex items-center gap-1 text-[11px] font-medium text-amber-800 bg-amber-50 px-2 py-0.5 rounded-md shrink-0 border border-amber-200/80">
          <span className="w-1.5 h-1.5 rounded-full bg-amber-500 shrink-0" />
          <span>Needs review</span>
        </span>
      );
    }
    if (suggestedWs) {
      return (
        <span className="inline-flex items-center gap-1 text-[11px] font-medium text-gray-700 bg-gray-100/90 px-2 py-0.5 rounded-md shrink-0 border border-black/5">
          <span className="text-gray-400">Suggested:</span>
          <span className="font-semibold text-gray-800">{suggestedWs.name}</span>
        </span>
      );
    }
    if (candidateInfo) {
      return (
        <span className="inline-flex items-center gap-1 text-[11px] font-medium text-cyan-800 bg-cyan-50 px-2 py-0.5 rounded-md shrink-0 border border-cyan-200/60">
          <span className="text-cyan-600 font-normal">Candidate:</span>
          <span className="font-semibold">{candidateInfo.candidateName}</span>
        </span>
      );
    }
    return (
      <span className="inline-flex items-center text-[11px] font-normal text-gray-400 bg-gray-50 px-2 py-0.5 rounded-md shrink-0">
        No context yet
      </span>
    );
  };

  const getItemIcon = () => {
    switch (item.item_type) {
      case 'image':
        return <ImageIcon size={14} className="text-purple-500 shrink-0" />;
      case 'url':
        return <Link2 size={14} className="text-blue-500 shrink-0" />;
      case 'file':
        return <FileCode size={14} className="text-gray-500 shrink-0" />;
      default:
        return <FileText size={14} className="text-gray-500 shrink-0" />;
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
      className={`group relative flex items-center justify-between px-4 py-3 bg-white hover:bg-gray-50/70 border-b border-black/5 transition-all duration-100 select-none cursor-pointer ${
        isSelected
          ? 'bg-blue-50/30'
          : isFocused
          ? 'bg-gray-50/80 ring-1 ring-black/10'
          : ''
      }`}
    >
      {/* Left Column: Selection Checkbox + Type Icon + Title & Snippet */}
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
              ? 'bg-gray-900 border-gray-900 text-white'
              : 'border-gray-300 hover:border-gray-500 bg-white opacity-0 group-hover:opacity-100'
          } ${isMultiSelectActive ? '!opacity-100' : ''}`}
          title={isSelected ? 'Deselect' : 'Select (X)'}
        >
          {isSelected && <Check size={11} strokeWidth={3} />}
        </button>

        {/* Content Icon */}
        <div className="w-5 flex items-center justify-center shrink-0">
          {getItemIcon()}
        </div>

        {/* Text Details & Context Pill */}
        <div className="flex flex-col min-w-0 flex-1 gap-0.5">
          <div className="flex items-center gap-2 min-w-0">
            <span className="text-[13px] font-semibold text-gray-900 truncate">
              {displayTitle}
            </span>
            {renderContextPill()}
          </div>

          <p className="text-[12px] text-gray-500 truncate leading-normal">
            {snippet}
          </p>
        </div>
      </div>

      {/* Right Column: Relative Timestamp & Subtle Hover Menu */}
      <div className="flex items-center gap-2 shrink-0">
        <span className="text-[11px] text-gray-400 font-mono">
          {getRelativeTime(item.created_at)}
        </span>

        <button
          ref={menuBtnRef}
          onClick={openMenu}
          className="p-1 text-gray-400 hover:text-gray-700 hover:bg-black/5 rounded-md transition-colors cursor-pointer opacity-0 group-hover:opacity-100"
          title="More options"
        >
          <MoreVertical size={14} />
        </button>
      </div>

      {/* Options Menu Dropdown Portal */}
      {menuOpen && menuPos && createPortal(
        <div
          ref={menuRef}
          style={{ top: menuPos.top, right: menuPos.right }}
          className="fixed w-48 bg-white rounded-xl shadow-xl border border-black/10 py-1.5 z-50 text-[12px] overflow-hidden animate-in fade-in-0 duration-100"
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen(false);
              onSelectItem?.(item.id);
            }}
            className="w-full text-left px-3 py-1.5 text-gray-700 hover:bg-black/5 flex items-center gap-2 transition-colors cursor-pointer"
          >
            <FolderPlus size={13} className="text-gray-500" />
            <span>View Context Details</span>
          </button>

          <button
            onClick={handleCopy}
            className="w-full text-left px-3 py-1.5 text-gray-700 hover:bg-black/5 flex items-center gap-2 transition-colors cursor-pointer"
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
              className="w-full text-left px-3 py-1.5 text-gray-700 hover:bg-black/5 flex items-center gap-2 transition-colors cursor-pointer"
            >
              <ExternalLink size={13} className="text-gray-500" />
              <span>Open in Browser</span>
            </button>
          )}

          <div className="my-1 border-t border-black/5" />

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
              className="w-full text-left px-3 py-1.5 text-gray-700 hover:bg-black/5 flex items-center gap-2 transition-colors cursor-pointer"
            >
              <BoxSelect size={12} className="text-gray-400" />
              <span className="truncate">{ws.name}</span>
            </button>
          ))}

          <div className="my-1 border-t border-black/5" />

          <button
            onClick={handleDelete}
            className="w-full text-left px-3 py-1.5 text-red-600 hover:bg-red-50 flex items-center gap-2 transition-colors cursor-pointer"
          >
            <Trash2 size={13} className="text-red-500" />
            <span>Delete Item</span>
          </button>
        </div>,
        document.body
      )}
    </div>
  );
}

export const InboxItemCard = memo(InboxItemCardComponent);
