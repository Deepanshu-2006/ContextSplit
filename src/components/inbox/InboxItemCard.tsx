import { useState, useRef, useEffect, memo } from 'react';
import { createPortal } from 'react-dom';
import {
  FileText,
  Image as ImageIcon,
  Link2,
  FileCode,
  Sparkles,
  Plus,
  X,
  Search,
  MoreVertical,
  Check,
  BoxSelect,
  CheckCircle2,
  ExternalLink,
  FolderPlus,
} from 'lucide-react';
import { db, Workspace } from '../../services/db';
import { DerivedInboxItem, useInboxStore } from '../../store/inboxStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { useDiscoveryStore } from '../../store/discoveryStore';
import { ContextInspector } from '../workspace/ContextInspector';
import { ContextBadge } from '../workspace/ContextBadge';
import { QuickContextAssignmentMenu } from './QuickContextAssignmentMenu';

interface InboxItemCardProps {
  derived: DerivedInboxItem;
  workspaces: Workspace[];
  onMoveToWorkspace: (itemId: string, workspaceId: string) => void;
  showSectionBadge?: boolean;
}

function InboxItemCardComponent({
  derived,
  workspaces,
  onMoveToWorkspace,
  showSectionBadge = false,
}: InboxItemCardProps) {
  const { item, section, attention, candidateInfo } = derived;
  const {
    dismissItemSuggestion,
    selectedItemIds,
    toggleSelectItem,
    isMultiSelectActive,
    focusedItemId,
    setFocusedItemId,
  } = useInboxStore();
  const { reviewCandidate } = useDiscoveryStore();
  const { setIsDraggingCard, setHoveredWorkspaceId } = useWorkspaceStore();

  const isSelected = selectedItemIds.includes(item.id);
  const isFocused = focusedItemId === item.id;

  const [menuOpen, setMenuOpen] = useState(false);
  const [quickMenuOpen, setQuickMenuOpen] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ top: number; right: number } | null>(null);
  const [quickMenuPos, setQuickMenuPos] = useState<{ top: number; left?: number; right?: number } | null>(null);
  const menuBtnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [isProcessingAction, setIsProcessingAction] = useState(false);

  // Close options menu on click outside
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
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const openMenu = () => {
    if (menuBtnRef.current) {
      const rect = menuBtnRef.current.getBoundingClientRect();
      setMenuPos({ top: rect.bottom + 6, right: window.innerWidth - rect.right });
    }
    setMenuOpen((v) => !v);
  };

  const openQuickMenu = (e: React.MouseEvent<HTMLElement>) => {
    e.stopPropagation();
    const rect = e.currentTarget.getBoundingClientRect();
    setQuickMenuPos({ top: rect.bottom + 6, left: rect.left });
    setQuickMenuOpen(true);
    setMenuOpen(false);
  };

  const handleDismiss = async (wsId?: string) => {
    try {
      setIsProcessingAction(true);
      await dismissItemSuggestion(item.id, wsId);
    } catch (e) {
      console.error(e);
    } finally {
      setIsProcessingAction(false);
    }
  };

  const handleReviewCandidate = () => {
    if (candidateInfo?.candidateId) {
      reviewCandidate(candidateInfo.candidateId);
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
      const diffDays = Math.floor(diffHours / 24);

      if (diffMins < 1) return 'Just now';
      if (diffMins < 60) return `${diffMins}m ago`;
      if (diffHours < 24) return `${diffHours}h ago`;
      return `${diffDays}d ago`;
    } catch {
      return '';
    }
  };

  // Extract application name from metadata
  let appName: string | null = null;
  try {
    const meta = JSON.parse(item.metadata || '{}');
    appName = meta.app_name || meta.source_app || null;
  } catch {}

  // Render type-specific content preview
  const renderContent = () => {
    switch (item.item_type) {
      case 'text':
        return (
          <p className="text-[13px] text-gray-700 leading-relaxed line-clamp-4 font-mono select-text bg-gray-50/70 p-2.5 rounded-lg border border-black/3">
            {item.content}
          </p>
        );
      case 'image':
        return (
          <div className="rounded-lg overflow-hidden border border-black/5 bg-gray-50 flex items-center justify-center max-h-48">
            <img
              src={item.content}
              alt={item.title || 'Captured image'}
              className="w-full h-full object-contain max-h-44"
              loading="lazy"
            />
          </div>
        );
      case 'url':
        return (
          <div className="p-2.5 bg-blue-50/40 rounded-lg border border-blue-100 flex flex-col gap-1">
            <a
              href={item.content}
              target="_blank"
              rel="noopener noreferrer"
              className="text-[13px] font-medium text-blue-600 hover:text-blue-800 flex items-center gap-1.5 truncate"
            >
              <span className="truncate">{item.title || item.content}</span>
              <ExternalLink size={12} className="shrink-0 opacity-70" />
            </a>
            <span className="text-[11px] text-gray-400 font-mono truncate">{item.content}</span>
          </div>
        );
      case 'file':
      default:
        return (
          <div className="flex items-center gap-2 p-2.5 bg-gray-50 rounded-lg border border-black/4">
            <FileCode size={18} className="text-gray-500 shrink-0" />
            <div className="min-w-0 flex-1">
              <p className="text-[12px] font-medium text-gray-800 truncate">{item.title || item.content}</p>
              <p className="text-[10px] text-gray-400 uppercase tracking-wider">File Resource</p>
            </div>
          </div>
        );
    }
  };

  const getItemIcon = () => {
    switch (item.item_type) {
      case 'text':
        return <FileText size={14} className="text-gray-500" />;
      case 'image':
        return <ImageIcon size={14} className="text-purple-500" />;
      case 'url':
        return <Link2 size={14} className="text-blue-500" />;
      default:
        return <FileCode size={14} className="text-gray-500" />;
    }
  };

  const suggestedWsId = attention?.factors?.suggested_workspace_id;

  // Evaluate Contextual Action State
  const hasHighConfidenceSuggestion =
    Boolean(
      (attention?.factors?.has_suggested_membership ||
        (suggestedWsId != null && (attention?.factors?.contextual_confidence ?? 0) >= 0.70)) &&
        !attention?.factors?.is_ambiguous
    );

  const hasCandidate = Boolean(
    candidateInfo != null || attention?.factors?.has_new_candidate === true
  );

  const isUncertain = Boolean(
    attention?.factors?.is_ambiguous === true ||
      derived.section === 'needs_attention'
  );

  const isUnderstood = Boolean(
    derived.section === 'already_understood' ||
      derived.inboxState?.state === 'understood' ||
      ((attention?.factors?.contextual_confidence ?? 0) >= 0.70 && !attention?.factors?.is_ambiguous)
  );

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
      onClick={(e) => {
        setFocusedItemId(item.id);
        if (e.shiftKey || e.metaKey || e.ctrlKey || isMultiSelectActive) {
          const target = e.target as HTMLElement;
          if (!target.closest('button') && !target.closest('a') && !target.closest('input')) {
            toggleSelectItem(item.id);
          }
        }
      }}
      className={`group bg-white rounded-xl border transition-all duration-200 flex flex-col justify-between overflow-hidden shadow-2xs hover:shadow-md cursor-pointer ${
        isSelected
          ? 'ring-2 ring-indigo-600 border-indigo-400 bg-indigo-50/15 shadow-sm'
          : isFocused
          ? 'ring-2 ring-indigo-500/80 border-indigo-500 shadow-md ring-offset-1'
          : section === 'needs_attention'
          ? 'border-amber-200/90 hover:border-amber-400/80 bg-linear-to-b from-amber-50/20 to-white'
          : section === 'new_candidates'
          ? 'border-cyan-200/90 hover:border-cyan-400/80 bg-linear-to-b from-cyan-50/20 to-white'
          : section === 'context_suggestions'
          ? 'border-indigo-200/90 hover:border-indigo-400/80 bg-linear-to-b from-indigo-50/15 to-white'
          : section === 'already_understood'
          ? 'border-teal-200/90 hover:border-teal-400/80 bg-linear-to-b from-teal-50/15 to-white'
          : 'border-black/6 hover:border-black/15'
      }`}
    >
      {/* Top Card Header */}
      <div className="flex items-center justify-between px-3.5 pt-3 pb-2 border-b border-black/3">
        <div className="flex items-center gap-1.5 min-w-0">
          <button
            onClick={(e) => {
              e.stopPropagation();
              toggleSelectItem(item.id);
            }}
            className={`w-4 h-4 rounded border flex items-center justify-center transition-colors cursor-pointer ${
              isSelected
                ? 'bg-indigo-600 border-indigo-600 text-white'
                : 'border-gray-300 hover:border-gray-500 bg-white'
            }`}
            title={isSelected ? 'Deselect item' : 'Select item (X)'}
          >
            {isSelected && <Check size={11} strokeWidth={3} />}
          </button>

          <span className="text-gray-400">{getItemIcon()}</span>
          <span className="text-[11px] font-medium text-gray-500 capitalize truncate">
            {item.item_type}
          </span>
          {showSectionBadge && (
            <span
              className={`text-[9px] font-semibold px-1.5 py-0.2 rounded-full border ${
                section === 'needs_attention'
                  ? 'bg-amber-50 text-amber-700 border-amber-200'
                  : section === 'new_candidates'
                  ? 'bg-cyan-50 text-cyan-700 border-cyan-200'
                  : section === 'context_suggestions'
                  ? 'bg-indigo-50 text-indigo-700 border-indigo-200'
                  : section === 'already_understood'
                  ? 'bg-teal-50 text-teal-700 border-teal-200'
                  : 'bg-gray-100 text-gray-600 border-gray-200'
              }`}
            >
              {section.replace('_', ' ')}
            </span>
          )}
        </div>

        <div className="flex items-center gap-1 text-gray-400">
          <span className="text-[10px] text-gray-400 font-mono">
            {getRelativeTime(item.created_at)}
          </span>
          <button
            ref={menuBtnRef}
            onClick={(e) => {
              e.stopPropagation();
              openMenu();
            }}
            className="p-1 hover:text-gray-700 hover:bg-black/5 rounded transition-colors cursor-pointer opacity-80 group-hover:opacity-100"
            title="Item options"
          >
            <MoreVertical size={13} />
          </button>
        </div>
      </div>

      {/* Main Content Area */}
      <div className="p-3.5 flex-1 flex flex-col gap-2 min-w-0">
        {/* Title */}
        {item.title && (
          <h3 className="text-xs font-semibold text-gray-900 line-clamp-1 group-hover:text-indigo-600 transition-colors">
            {item.title}
          </h3>
        )}

        {/* Content Preview */}
        {renderContent()}

        {/* Source App / Domain Badge */}
        {appName && (
          <div className="flex items-center gap-1 text-[10px] text-gray-400 truncate">
            <span className="w-1.5 h-1.5 rounded-full bg-gray-300 shrink-0" />
            <span className="truncate">From {appName}</span>
          </div>
        )}

        {/* Match Highlight if Search Active */}
        {derived.searchResult && (
          <div className="mt-1 p-1.5 rounded-md bg-gray-50 border border-black/4 flex items-center justify-between text-[10px]">
            {derived.searchResult.semantic_score != null ? (
              <span className="flex items-center gap-1 text-indigo-700 font-medium">
                <Sparkles size={11} className="text-amber-500" />
                Semantic match
              </span>
            ) : (
              <span className="flex items-center gap-1 text-gray-500 font-medium">
                <Search size={11} className="text-gray-400" />
                Keyword match
              </span>
            )}
            {derived.searchResult.match_highlights && derived.searchResult.match_highlights.length > 0 && (
              <span className="text-[9px] text-gray-400 truncate max-w-[120px]" title={derived.searchResult.match_highlights[0]}>
                {derived.searchResult.match_highlights[0]}
              </span>
            )}
          </div>
        )}
      </div>

      {/* Context Status & Inline Actions Section */}
      <div className="px-3.5 pb-3 flex flex-col gap-2.5 border-t border-black/3 pt-2.5 bg-gray-50/40">
        {/* Contextual Badge (Clickable to Inspect ContextExplanation) */}
        <div className="flex items-center justify-between gap-2">
          <ContextBadge
            itemId={item.id}
            attention={attention}
            candidateInfo={candidateInfo}
            workspaces={workspaces}
            onOpenInspector={() => setInspectorOpen(true)}
          />
        </div>

        {/* ==================================================================== */}
        {/* INLINE CONTEXTUAL ACTIONS */}
        {/* ==================================================================== */}

        {/* 1. HIGH-CONFIDENCE SUGGESTED MEMBERSHIP: [Add to Context] */}
        {hasHighConfidenceSuggestion && (
          <div className="flex items-center gap-1.5 pt-0.5">
            <button
              onClick={openQuickMenu}
              disabled={isProcessingAction}
              className="flex-1 flex items-center justify-between px-3 py-1.5 bg-indigo-600 hover:bg-indigo-700 text-white rounded-lg text-[11px] font-semibold shadow-2xs transition-colors cursor-pointer group/btn"
              title="Add to Context (A)"
            >
              <div className="flex items-center gap-1.5">
                <Plus size={13} strokeWidth={2.5} />
                <span>Add to Context</span>
              </div>
              <kbd className="text-[9px] font-mono text-indigo-200 bg-indigo-800/50 px-1 py-0.2 rounded border border-indigo-400/30 opacity-80 group-hover/btn:opacity-100">
                A
              </kbd>
            </button>
            <button
              onClick={() => handleDismiss(suggestedWsId || undefined)}
              disabled={isProcessingAction}
              title="Dismiss suggestion (D)"
              className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-black/5 rounded-lg border border-black/5 transition-colors cursor-pointer flex items-center gap-1"
            >
              <X size={13} />
            </button>
          </div>
        )}

        {/* 2. NEW CONTEXT CANDIDATES: [Review Context] */}
        {!hasHighConfidenceSuggestion && hasCandidate && candidateInfo && (
          <div className="pt-0.5">
            <button
              onClick={handleReviewCandidate}
              className="w-full flex items-center justify-between px-3 py-1.5 bg-cyan-600 hover:bg-cyan-700 text-white rounded-lg text-[11px] font-semibold shadow-2xs transition-colors cursor-pointer group/btn"
              title="Review Context (R)"
            >
              <div className="flex items-center gap-1.5">
                <Sparkles size={13} />
                <span>Review Context</span>
              </div>
              <kbd className="text-[9px] font-mono text-cyan-200 bg-cyan-800/50 px-1 py-0.2 rounded border border-cyan-400/30 opacity-80 group-hover/btn:opacity-100">
                R
              </kbd>
            </button>
          </div>
        )}

        {/* 3. UNCERTAIN ITEMS: [Choose Context] */}
        {!hasHighConfidenceSuggestion && !hasCandidate && isUncertain && (
          <div className="flex items-center gap-1.5 pt-0.5 relative">
            <button
              onClick={openQuickMenu}
              disabled={isProcessingAction}
              className="flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 bg-amber-600 hover:bg-amber-700 text-white rounded-lg text-[11px] font-semibold shadow-2xs transition-colors cursor-pointer"
              title="Choose Context"
            >
              <FolderPlus size={13} />
              <span>Choose Context</span>
            </button>
            <button
              onClick={() => handleDismiss()}
              disabled={isProcessingAction}
              title="Dismiss prompt (D)"
              className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-black/5 rounded-lg border border-black/5 transition-colors cursor-pointer"
            >
              <X size={13} />
            </button>
          </div>
        )}

        {/* 4. UNDERSTOOD ITEMS: [Add to Context] */}
        {!hasHighConfidenceSuggestion && !hasCandidate && !isUncertain && isUnderstood && (
          <div className="flex items-center gap-1.5 pt-0.5">
            <button
              onClick={openQuickMenu}
              disabled={isProcessingAction}
              className="flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 bg-teal-600 hover:bg-teal-700 text-white rounded-lg text-[11px] font-semibold shadow-2xs transition-colors cursor-pointer"
              title="Add / Confirm Context"
            >
              <Plus size={13} strokeWidth={2.5} />
              <span>Add to Context</span>
            </button>
          </div>
        )}

        {/* 5. ITEMS WITH NO MEANINGFUL EVIDENCE: SHOW NO ACTION */}
      </div>

      {/* Floating Options Dropdown Menu */}
      {menuOpen && menuPos && createPortal(
        <div
          ref={menuRef}
          style={{ top: menuPos.top, right: menuPos.right }}
          className="fixed w-52 bg-white rounded-xl shadow-xl border border-gray-200/90 py-1.5 z-[9999] overflow-hidden"
        >
          <div className="px-3 pt-1 pb-0.5 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
            Item Actions
          </div>
          <button
            className="w-full text-left px-3 py-1.5 text-[12px] text-gray-700 hover:bg-gray-50 flex items-center gap-2 transition-colors cursor-pointer"
            onClick={openQuickMenu}
          >
            <FolderPlus size={13} className="text-indigo-600" />
            <span>Add to Context...</span>
          </button>
          <button
            className="w-full text-left px-3 py-1.5 text-[12px] text-gray-700 hover:bg-gray-50 flex items-center gap-2 transition-colors cursor-pointer"
            onClick={() => {
              setInspectorOpen(true);
              setMenuOpen(false);
            }}
          >
            <Search size={13} className="text-indigo-500" />
            <span>Inspect Context Evidence</span>
          </button>

          {section !== 'already_understood' && (
            <button
              className="w-full text-left px-3 py-1.5 text-[12px] text-gray-700 hover:bg-gray-50 flex items-center gap-2 transition-colors cursor-pointer"
              onClick={() => {
                db.transitionInboxItemState(item.id, 'understood', 'User marked as understood')
                  .then(() => setMenuOpen(false));
              }}
            >
              <CheckCircle2 size={13} className="text-emerald-500" />
              <span>Mark as Understood</span>
            </button>
          )}

          <div className="my-1 border-t border-gray-100" />
          <div className="px-3 pt-1 pb-0.5 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
            Move to Workspace
          </div>

          {workspaces.map((ws) => (
            <button
              key={ws.id}
              className="w-full text-left px-3 py-1.5 text-[12px] text-gray-700 hover:bg-gray-50 flex items-center gap-2 transition-colors cursor-pointer"
              onClick={() => {
                onMoveToWorkspace(item.id, ws.id);
                setMenuOpen(false);
              }}
            >
              <BoxSelect size={13} className="text-gray-400" />
              <span className="truncate">{ws.name}</span>
            </button>
          ))}
        </div>,
        document.body
      )}

      {/* Quick Context Assignment Menu */}
      <QuickContextAssignmentMenu
        itemId={item.id}
        suggestedWorkspaceId={suggestedWsId}
        suggestedConfidence={attention?.factors?.contextual_confidence}
        workspaces={workspaces}
        isOpen={quickMenuOpen}
        position={quickMenuPos}
        onClose={() => setQuickMenuOpen(false)}
        onAssignSuccess={(wsId) => {
          onMoveToWorkspace(item.id, wsId);
        }}
      />

      {/* Context Inspector Modal */}
      {inspectorOpen && (
        <ContextInspector itemId={item.id} onClose={() => setInspectorOpen(false)} />
      )}
    </div>
  );
}

export const InboxItemCard = memo(InboxItemCardComponent);
