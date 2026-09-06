import { useState, useRef, useEffect } from 'react';
import { createPortal } from 'react-dom';
import {
  FileText,
  Image as ImageIcon,
  Link2,
  FileCode,
  Sparkles,
  Check,
  X,
  AlertTriangle,
  Layers,
  Search,
  MoreHorizontal,
  BoxSelect,
  CheckCircle2,
  Clock,
  ExternalLink,
  GripVertical,
} from 'lucide-react';
import { Workspace } from '../../services/db';
import { DerivedInboxItem, useInboxStore } from '../../store/inboxStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { useDiscoveryStore } from '../../store/discoveryStore';
import { ContextInspector } from '../workspace/ContextInspector';

interface InboxItemCardProps {
  derived: DerivedInboxItem;
  workspaces: Workspace[];
  onMoveToWorkspace: (itemId: string, workspaceId: string) => void;
  showSectionBadge?: boolean;
}

export function InboxItemCard({
  derived,
  workspaces,
  onMoveToWorkspace,
  showSectionBadge = false,
}: InboxItemCardProps) {
  const { item, section, attention, candidateInfo } = derived;
  const { acceptItemSuggestion, dismissItemSuggestion, markItemUnderstood } = useInboxStore();
  const { reviewCandidate } = useDiscoveryStore();
  const { setIsDraggingCard, setHoveredWorkspaceId } = useWorkspaceStore();

  const [menuOpen, setMenuOpen] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ top: number; right: number } | null>(null);
  const menuBtnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [isProcessingAction, setIsProcessingAction] = useState(false);

  // Close menu on click outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
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

  const handleAccept = async (targetWsId: string) => {
    try {
      setIsProcessingAction(true);
      await acceptItemSuggestion(item.id, targetWsId);
      onMoveToWorkspace(item.id, targetWsId);
    } catch (e) {
      console.error(e);
    } finally {
      setIsProcessingAction(false);
    }
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
  const suggestedWsName =
    attention?.factors?.suggested_workspace_name ||
    workspaces.find((w) => w.id === suggestedWsId)?.name;
  const confidence = attention?.factors?.contextual_confidence
    ? Math.round(attention.factors.contextual_confidence * 100)
    : null;

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
      className={`group bg-white rounded-xl border transition-all duration-200 flex flex-col justify-between overflow-hidden shadow-2xs hover:shadow-md ${
        section === 'needs_attention'
          ? 'border-amber-200/90 hover:border-amber-400/80 bg-linear-to-b from-amber-50/20 to-white'
          : section === 'new_candidates'
          ? 'border-cyan-200/90 hover:border-cyan-400/80 bg-linear-to-b from-cyan-50/20 to-white'
          : section === 'context_suggestions'
          ? 'border-indigo-200/90 hover:border-indigo-400/80 bg-linear-to-b from-indigo-50/15 to-white'
          : 'border-black/6 hover:border-black/15'
      }`}
    >
      {/* Top Card Header */}
      <div className="flex items-center justify-between px-3.5 pt-3 pb-2 border-b border-black/3">
        <div className="flex items-center gap-2 min-w-0">
          <div className="cursor-grab active:cursor-grabbing text-gray-300 group-hover:text-gray-500 transition-colors">
            <GripVertical size={13} />
          </div>
          {getItemIcon()}
          <span className="text-[12px] font-medium text-gray-700 truncate max-w-[140px]">
            {item.title || item.item_type.toUpperCase()}
          </span>
        </div>

        <div className="flex items-center gap-1.5 shrink-0">
          {appName && (
            <span className="text-[10px] text-gray-400 bg-gray-100/80 px-1.5 py-0.5 rounded font-medium">
              {appName}
            </span>
          )}
          <span className="text-[10px] text-gray-400 font-mono">
            {getRelativeTime(item.created_at)}
          </span>

          <button
            ref={menuBtnRef}
            onClick={openMenu}
            className="p-1 rounded hover:bg-black/5 text-gray-400 hover:text-gray-800 transition-colors cursor-pointer"
            title="Options"
          >
            <MoreHorizontal size={13} />
          </button>
        </div>
      </div>

      {/* Main Content Preview */}
      <div className="p-3.5 flex-1 flex flex-col gap-2.5">{renderContent()}</div>

      {/* Context Status & Rationale Section */}
      <div className="px-3.5 pb-3 flex flex-col gap-2 border-t border-black/3 pt-2.5 bg-gray-50/40">
        {/* Section Pill if enabled in chronological view */}
        {showSectionBadge && (
          <div className="flex items-center gap-1.5">
            {section === 'needs_attention' && (
              <span className="inline-flex items-center gap-1 text-[10px] font-semibold text-amber-700 bg-amber-100/80 px-2 py-0.5 rounded-full">
                <AlertTriangle size={10} /> Needs Attention
              </span>
            )}
            {section === 'new_candidates' && (
              <span className="inline-flex items-center gap-1 text-[10px] font-semibold text-cyan-700 bg-cyan-100/80 px-2 py-0.5 rounded-full">
                <Layers size={10} /> Candidate Cluster
              </span>
            )}
            {section === 'context_suggestions' && (
              <span className="inline-flex items-center gap-1 text-[10px] font-semibold text-indigo-700 bg-indigo-100/80 px-2 py-0.5 rounded-full">
                <Sparkles size={10} /> Suggestion
              </span>
            )}
            {section === 'recently_captured' && (
              <span className="inline-flex items-center gap-1 text-[10px] font-medium text-sky-700 bg-sky-100/70 px-2 py-0.5 rounded-full">
                <Clock size={10} /> Recent Capture
              </span>
            )}
            {section === 'already_understood' && (
              <span className="inline-flex items-center gap-1 text-[10px] font-medium text-slate-600 bg-slate-100/80 px-2 py-0.5 rounded-full">
                <CheckCircle2 size={10} /> Understood
              </span>
            )}
          </div>
        )}

        {/* 1. Needs Attention Context Banner */}
        {section === 'needs_attention' && (
          <div className="flex flex-col gap-2 bg-amber-50/80 border border-amber-200/70 rounded-lg p-2.5">
            <div className="flex items-start gap-1.5">
              <AlertTriangle size={13} className="text-amber-600 mt-0.5 shrink-0" />
              <div className="flex-1 min-w-0">
                <p className="text-[11px] font-semibold text-amber-900 leading-tight">
                  {attention?.factors?.is_ambiguous
                    ? 'Context Ambiguity Detected'
                    : 'Attention Required'}
                </p>
                <p className="text-[10px] text-amber-700/90 mt-0.5 leading-snug">
                  {attention?.reason || 'Evaluate or assign to the appropriate workspace.'}
                </p>
              </div>
            </div>

            <div className="flex items-center gap-1.5 mt-0.5">
              {suggestedWsId && suggestedWsName && (
                <button
                  onClick={() => handleAccept(suggestedWsId)}
                  disabled={isProcessingAction}
                  className="flex-1 flex items-center justify-center gap-1 px-2 py-1 bg-amber-600 hover:bg-amber-700 text-white rounded text-[11px] font-medium transition-colors cursor-pointer"
                >
                  <Check size={11} />
                  <span>Assign to {suggestedWsName}</span>
                </button>
              )}
              <button
                onClick={() => setInspectorOpen(true)}
                className="px-2 py-1 bg-white hover:bg-amber-100/60 border border-amber-300/80 text-amber-900 rounded text-[11px] font-medium flex items-center gap-1 transition-colors cursor-pointer"
              >
                <Search size={11} />
                <span>Inspect</span>
              </button>
            </div>
          </div>
        )}

        {/* 2. New Context Candidates Banner */}
        {section === 'new_candidates' && candidateInfo && (
          <div className="flex flex-col gap-2 bg-cyan-50/80 border border-cyan-200/80 rounded-lg p-2.5">
            <div className="flex items-start gap-1.5">
              <Layers size={13} className="text-cyan-600 mt-0.5 shrink-0" />
              <div className="flex-1 min-w-0">
                <p className="text-[11px] font-semibold text-cyan-950 leading-tight truncate">
                  Cluster: {candidateInfo.candidateName}
                </p>
                <p className="text-[10px] text-cyan-800/80 mt-0.5">
                  {Math.round(candidateInfo.confidence * 100)}% cluster coherence
                </p>
              </div>
            </div>

            <button
              onClick={handleReviewCandidate}
              className="w-full flex items-center justify-center gap-1 px-2.5 py-1 bg-cyan-600 hover:bg-cyan-700 text-white rounded text-[11px] font-medium transition-colors cursor-pointer"
            >
              <Sparkles size={11} />
              <span>Review Candidate Cluster</span>
            </button>
          </div>
        )}

        {/* 3. Context Suggestions Banner */}
        {section === 'context_suggestions' && suggestedWsId && suggestedWsName && (
          <div className="flex flex-col gap-2 bg-indigo-50/70 border border-indigo-200/70 rounded-lg p-2.5">
            <div className="flex items-start justify-between gap-1.5">
              <div className="flex items-start gap-1.5 min-w-0">
                <Sparkles size={13} className="text-indigo-600 mt-0.5 shrink-0" />
                <div className="min-w-0">
                  <p className="text-[11px] font-semibold text-indigo-950 truncate">
                    Suggests: {suggestedWsName}
                  </p>
                  <p className="text-[10px] text-indigo-700/80 line-clamp-1">
                    {attention?.reason || 'Matching context profile & keywords'}
                  </p>
                </div>
              </div>
              {confidence !== null && (
                <span className="text-[10px] font-bold text-indigo-700 bg-indigo-100 px-1.5 py-0.5 rounded shrink-0">
                  {confidence}%
                </span>
              )}
            </div>

            <div className="flex items-center gap-1.5">
              <button
                onClick={() => handleAccept(suggestedWsId)}
                disabled={isProcessingAction}
                className="flex-1 flex items-center justify-center gap-1 px-2 py-1 bg-indigo-600 hover:bg-indigo-700 text-white rounded text-[11px] font-medium transition-colors cursor-pointer"
              >
                <Check size={11} />
                <span>Move to {suggestedWsName}</span>
              </button>
              <button
                onClick={() => handleDismiss(suggestedWsId)}
                disabled={isProcessingAction}
                title="Dismiss suggestion"
                className="p-1 text-gray-400 hover:text-gray-700 hover:bg-black/5 rounded transition-colors cursor-pointer"
              >
                <X size={13} />
              </button>
            </div>
          </div>
        )}

        {/* 4. Recently Captured Status */}
        {section === 'recently_captured' && (
          <div className="flex items-center justify-between text-[11px] text-gray-500 px-1">
            <span className="flex items-center gap-1.5">
              <Clock size={12} className="text-sky-500" />
              <span>Ready for organization</span>
            </span>
            <button
              onClick={() => markItemUnderstood(item.id)}
              className="text-[10px] text-gray-500 hover:text-gray-900 font-medium hover:underline cursor-pointer"
            >
              Mark understood
            </button>
          </div>
        )}

        {/* 5. Already Understood Status */}
        {section === 'already_understood' && (
          <div className="flex items-center justify-between text-[11px] text-slate-500 px-1">
            <span className="flex items-center gap-1.5">
              <CheckCircle2 size={12} className="text-emerald-500" />
              <span>Context Understood</span>
            </span>
            <button
              onClick={() => setInspectorOpen(true)}
              className="text-[10px] text-slate-400 hover:text-indigo-600 font-medium flex items-center gap-1 cursor-pointer"
            >
              <Search size={10} /> Inspect
            </button>
          </div>
        )}
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
                markItemUnderstood(item.id);
                setMenuOpen(false);
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

      {/* Context Inspector Modal */}
      {inspectorOpen && (
        <ContextInspector itemId={item.id} onClose={() => setInspectorOpen(false)} />
      )}
    </div>
  );
}
