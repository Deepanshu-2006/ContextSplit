import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  ExternalLink,
  Copy,
  Check,
  Trash2,
  Folder,
  ChevronDown,
  Maximize2,
  Globe,
  FileText,
  Image as ImageIcon,
  ArrowRight,
  Sparkles,
} from 'lucide-react';
import { Item, Workspace, ItemRelationship, UnifiedContextInterpretation, ContextReasoningResult, db } from '../../services/db';
import { DerivedInboxItem, useInboxStore } from '../../store/inboxStore';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { useDiscoveryStore } from '../../store/discoveryStore';
import { toast } from 'sonner';
import { showUndoToast } from './UndoToast';

interface InboxDetailDrawerProps {
  derived: DerivedInboxItem | null;
  workspaces: Workspace[];
  onClose: () => void;
  onSelectItem: (itemId: string) => void;
  onMoveToWorkspace: (itemId: string, workspaceId: string) => void;
}

export function InboxDetailDrawer({
  derived,
  workspaces,
  onClose,
  onSelectItem,
  onMoveToWorkspace,
}: InboxDetailDrawerProps) {
  const { acceptItemSuggestion, dismissItemSuggestion } = useInboxStore();
  const { reviewCandidate } = useDiscoveryStore();

  const [copied, setCopied] = useState(false);
  const [relatedItems, setRelatedItems] = useState<Item[]>([]);
  const [interpretation, setInterpretation] = useState<UnifiedContextInterpretation | null>(null);
  const [aiResult, setAiResult] = useState<ContextReasoningResult | null>(null);
  const [isAiLoading, setIsAiLoading] = useState(false);
  const [isAssignDropdownOpen, setIsAssignDropdownOpen] = useState(false);
  const [isActing, setIsActing] = useState(false);
  const [isLightboxOpen, setIsLightboxOpen] = useState(false);

  const item = derived?.item;

  useEffect(() => {
    if (!item) return;

    setCopied(false);
    setAiResult(null);
    setIsAssignDropdownOpen(false);
    setIsLightboxOpen(false);

    let isMounted = true;

    Promise.all([
      db.getStrongestRelationships(item.id, 5).catch(() => [] as ItemRelationship[]),
      db.evaluateItemContext(item.id).catch(() => null),
    ]).then(async ([rels, interp]) => {
      if (!isMounted) return;
      setInterpretation(interp);

      if (rels.length > 0) {
        const otherIds = rels.map((r) => (r.item_a_id === item.id ? r.item_b_id : r.item_a_id));
        const itemPromises = otherIds.map((id) => db.getItem(id).catch(() => null));
        const resolvedItems = await Promise.all(itemPromises);
        if (isMounted) {
          setRelatedItems(resolvedItems.filter((i): i is Item => i !== null));
        }
      } else {
        setRelatedItems([]);
      }
    });

    return () => {
      isMounted = false;
    };
  }, [item?.id]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (isLightboxOpen) {
          setIsLightboxOpen(false);
        } else {
          onClose();
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose, isLightboxOpen]);

  if (!derived || !item) return null;

  const { attention, candidateInfo } = derived;
  const suggestedWsId = attention?.factors?.suggested_workspace_id;
  const suggestedWs = workspaces.find((w) => w.id === suggestedWsId);
  const currentWs = workspaces.find((w) => w.id === item.workspace_id);

  // Parse metadata
  let appName: string | null = null;
  let sourceUrl: string | null = null;
  let domain: string | null = null;
  try {
    const meta = JSON.parse(item.metadata || '{}');
    appName = meta.app_name || meta.source_app || null;
    sourceUrl = meta.source_url || meta.url || null;
    if (sourceUrl) {
      try {
        domain = new URL(sourceUrl).hostname.replace(/^www\./, '');
      } catch {
        domain = sourceUrl;
      }
    }
  } catch {}

  const formatTimestamp = (dateStr: string) => {
    try {
      const date = new Date(dateStr);
      return date.toLocaleString(undefined, {
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
      });
    } catch {
      return '';
    }
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(item.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
      toast.success('Copied');
    } catch (e) {
      console.warn('Failed to copy', e);
    }
  };

  const handleAcceptSuggestion = async (wsId: string) => {
    setIsActing(true);
    try {
      await acceptItemSuggestion(item.id, wsId);
      toast.success(`Moved to ${workspaces.find((w) => w.id === wsId)?.name || 'workspace'}`);
      onClose();
    } catch (e) {
      console.error(e);
      toast.error('Failed to move item');
    } finally {
      setIsActing(false);
    }
  };

  const handleDismiss = async () => {
    setIsActing(true);
    try {
      await dismissItemSuggestion(item.id, suggestedWsId || undefined);
      toast.success('Suggestion cleared');
    } catch (e) {
      console.error(e);
      toast.error('Failed to dismiss');
    } finally {
      setIsActing(false);
    }
  };

  const handleDeleteItem = async () => {
    const itemToRestore = { ...item };
    try {
      await db.deleteItem(item.id);
      const currentActive = useWorkspaceStore.getState().activeItems;
      useWorkspaceStore.setState({
        activeItems: currentActive.filter((i) => i.id !== item.id),
        inboxCount: Math.max(0, useWorkspaceStore.getState().inboxCount - 1),
      });
      onClose();
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
            toast.error('Failed to restore');
          }
        },
      });
    } catch (e) {
      console.error(e);
      toast.error('Failed to delete item');
    }
  };

  const handleRunAiReasoning = async () => {
    setIsAiLoading(true);
    try {
      const resp = await db.analyzeItemContextAI(item.id, true);
      if (resp.ai_result) {
        setAiResult(resp.ai_result);
      } else {
        toast.info(resp.unavailability_reason || 'Context reasoning unavailable');
      }
    } catch (e) {
      console.error('Failed to run reasoning', e);
      toast.error('Could not analyze context');
    } finally {
      setIsAiLoading(false);
    }
  };

  const getItemTypeLabel = () => {
    switch (item.item_type) {
      case 'image':
        return 'Image';
      case 'url':
        return 'Link';
      case 'file':
        return 'File';
      default:
        return 'Text Note';
    }
  };

  return createPortal(
    <>
      {/* Lightbox full-image preview */}
      {isLightboxOpen && item.item_type === 'image' && (
        <div
          className="fixed inset-0 z-60 bg-black/90 flex items-center justify-center p-8 animate-in fade-in duration-150 cursor-zoom-out"
          onClick={() => setIsLightboxOpen(false)}
        >
          <div className="relative max-w-5xl max-h-[92vh] flex items-center justify-center">
            <img
              src={item.content}
              alt={item.title || 'Preview'}
              className="max-w-full max-h-[90vh] object-contain rounded-lg shadow-2xl"
              onClick={(e) => e.stopPropagation()}
            />
            <button
              onClick={() => setIsLightboxOpen(false)}
              className="absolute -top-3 -right-3 w-8 h-8 rounded-full bg-zinc-800 text-zinc-300 hover:text-white flex items-center justify-center transition-colors cursor-pointer"
            >
              <X size={16} />
            </button>
          </div>
        </div>
      )}

      {/* Main Drawer Overlay */}
      <div className="fixed inset-0 z-40 flex justify-end pointer-events-auto">
        <div
          className="fixed inset-0 bg-black/15 transition-opacity duration-150"
          onClick={onClose}
        />

        {/* macOS Inspector Style Drawer */}
        <div
          className="relative w-full max-w-[420px] h-full bg-[#FAFAFA] border-l border-zinc-200/90 shadow-xl flex flex-col z-50 overflow-hidden text-zinc-900 select-text"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Top Bar */}
          <div className="h-12 px-4 border-b border-zinc-200/80 bg-white flex items-center justify-between shrink-0">
            <div className="flex items-center gap-2 min-w-0">
              <span className="text-[13px] font-medium text-zinc-900 truncate">
                {item.title || getItemTypeLabel()}
              </span>
            </div>

            <div className="flex items-center gap-0.5">
              <button
                onClick={handleCopy}
                title="Copy"
                className="w-7 h-7 flex items-center justify-center text-zinc-500 hover:text-zinc-900 hover:bg-zinc-100 rounded-md transition-colors cursor-pointer"
              >
                {copied ? <Check size={14} className="text-zinc-900" /> : <Copy size={14} />}
              </button>
              <button
                onClick={handleDeleteItem}
                title="Delete"
                className="w-7 h-7 flex items-center justify-center text-zinc-500 hover:text-red-600 hover:bg-zinc-100 rounded-md transition-colors cursor-pointer"
              >
                <Trash2 size={14} />
              </button>
              <div className="w-px h-3.5 bg-zinc-200 mx-1" />
              <button
                onClick={onClose}
                title="Close"
                className="w-7 h-7 flex items-center justify-center text-zinc-500 hover:text-zinc-900 hover:bg-zinc-100 rounded-md transition-colors cursor-pointer"
              >
                <X size={15} />
              </button>
            </div>
          </div>

          {/* Body Content */}
          <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-4">
            {/* Visual Hero Preview */}
            {item.item_type === 'image' ? (
              <div
                onClick={() => setIsLightboxOpen(true)}
                className="group relative rounded-lg border border-zinc-200 bg-white p-2 flex items-center justify-center cursor-zoom-in hover:border-zinc-300 transition-colors"
              >
                <img
                  src={item.content}
                  alt={item.title || 'Preview'}
                  className="w-full h-auto max-h-64 object-contain rounded"
                />
                <div className="absolute top-3 right-3 opacity-0 group-hover:opacity-100 transition-opacity bg-black/60 text-white p-1 rounded backdrop-blur-xs">
                  <Maximize2 size={12} />
                </div>
              </div>
            ) : item.item_type === 'url' ? (
              <div className="rounded-lg border border-zinc-200 bg-white p-3 flex flex-col gap-1.5">
                <a
                  href={item.content}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[13px] font-medium text-zinc-900 hover:underline flex items-center gap-1.5 break-all"
                >
                  <span>{item.title || domain || item.content}</span>
                  <ExternalLink size={12} className="text-zinc-400 shrink-0" />
                </a>
                <span className="text-[11px] text-zinc-400 font-mono break-all select-all">
                  {item.content}
                </span>
              </div>
            ) : (
              <div className="rounded-lg border border-zinc-200 bg-white p-3.5 text-[13px] text-zinc-800 leading-relaxed font-sans whitespace-pre-wrap break-words max-h-56 overflow-y-auto">
                {item.content}
              </div>
            )}

            {/* Smart Suggestion Banner (if detected) */}
            {suggestedWs && (
              <div className="rounded-lg border border-zinc-200 bg-white p-3 flex flex-col gap-2">
                <div className="flex items-center justify-between text-[12px]">
                  <span className="text-zinc-500">Suggested Workspace</span>
                  <span className="font-medium text-zinc-900">{suggestedWs.name}</span>
                </div>
                <div className="flex items-center gap-2 pt-1">
                  <button
                    onClick={() => handleAcceptSuggestion(suggestedWs.id)}
                    disabled={isActing}
                    className="flex-1 py-1 px-3 bg-zinc-900 hover:bg-black text-white text-[12px] font-medium rounded-md transition-colors cursor-pointer"
                  >
                    Move to {suggestedWs.name}
                  </button>
                  <button
                    onClick={handleDismiss}
                    disabled={isActing}
                    className="py-1 px-2.5 text-zinc-500 hover:text-zinc-800 text-[12px] rounded-md transition-colors cursor-pointer"
                  >
                    Dismiss
                  </button>
                </div>
              </div>
            )}

            {/* Candidate Project Banner */}
            {candidateInfo && (
              <div className="rounded-lg border border-zinc-200 bg-white p-3 flex flex-col gap-2">
                <div className="flex items-center justify-between text-[12px]">
                  <span className="text-zinc-500">New Cluster Detected</span>
                  <span className="font-medium text-zinc-900">"{candidateInfo.candidateName}"</span>
                </div>
                <button
                  onClick={() => {
                    reviewCandidate(candidateInfo.candidateId);
                    onClose();
                  }}
                  className="w-full py-1.5 px-3 bg-zinc-900 hover:bg-black text-white text-[12px] font-medium rounded-md transition-colors cursor-pointer"
                >
                  Review Candidate
                </button>
              </div>
            )}

            {/* Inspector Properties Table */}
            <div className="rounded-lg border border-zinc-200 bg-white overflow-hidden">
              <div className="px-3 py-2 border-b border-zinc-100 flex items-center justify-between text-[12px]">
                <span className="text-zinc-500">Workspace</span>
                <div className="relative">
                  <button
                    onClick={() => setIsAssignDropdownOpen(!isAssignDropdownOpen)}
                    className="inline-flex items-center gap-1.5 text-zinc-800 hover:text-zinc-950 font-medium cursor-pointer"
                  >
                    <Folder size={13} className="text-zinc-400" />
                    <span>{currentWs ? currentWs.name : 'Inbox (Unassigned)'}</span>
                    <ChevronDown size={12} className="text-zinc-400" />
                  </button>

                  {isAssignDropdownOpen && (
                    <div className="absolute right-0 top-full mt-1 w-52 bg-white border border-zinc-200 rounded-lg shadow-lg py-1 z-30 max-h-48 overflow-y-auto">
                      {workspaces.map((ws) => (
                        <button
                          key={ws.id}
                          onClick={() => {
                            onMoveToWorkspace(item.id, ws.id);
                            setIsAssignDropdownOpen(false);
                          }}
                          className={`w-full text-left px-3 py-1.5 text-[12px] flex items-center justify-between transition-colors cursor-pointer ${
                            ws.id === item.workspace_id
                              ? 'bg-zinc-100 font-medium text-zinc-900'
                              : 'text-zinc-700 hover:bg-zinc-50'
                          }`}
                        >
                          <span className="truncate">{ws.name}</span>
                          {ws.id === item.workspace_id && (
                            <Check size={12} className="text-zinc-600 shrink-0" />
                          )}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>

              {appName && (
                <div className="px-3 py-2 border-b border-zinc-100 flex items-center justify-between text-[12px]">
                  <span className="text-zinc-500">Source</span>
                  <span className="font-medium text-zinc-800 truncate max-w-[200px]">{appName}</span>
                </div>
              )}

              {domain && (
                <div className="px-3 py-2 border-b border-zinc-100 flex items-center justify-between text-[12px]">
                  <span className="text-zinc-500">Domain</span>
                  <span className="font-mono text-[11px] text-zinc-700 truncate max-w-[200px] flex items-center gap-1">
                    <Globe size={11} className="text-zinc-400" />
                    {domain}
                  </span>
                </div>
              )}

              <div className="px-3 py-2 border-b border-zinc-100 flex items-center justify-between text-[12px]">
                <span className="text-zinc-500">Type</span>
                <span className="font-medium text-zinc-800">{getItemTypeLabel()}</span>
              </div>

              <div className="px-3 py-2 flex items-center justify-between text-[12px]">
                <span className="text-zinc-500">Captured</span>
                <span className="text-zinc-700">{formatTimestamp(item.created_at)}</span>
              </div>
            </div>

            {/* Context Insights (Clean Native Disclosure) */}
            <div className="rounded-lg border border-zinc-200 bg-white p-3 flex flex-col gap-2">
              <div className="flex items-center justify-between">
                <span className="text-[12px] font-medium text-zinc-800">Context Analysis</span>
                {!aiResult && (
                  <button
                    onClick={handleRunAiReasoning}
                    disabled={isAiLoading}
                    className="text-[11px] text-zinc-500 hover:text-zinc-900 transition-colors flex items-center gap-1 cursor-pointer"
                  >
                    <Sparkles size={11} className={isAiLoading ? 'animate-spin' : ''} />
                    <span>{isAiLoading ? 'Analyzing...' : 'Deep inspect'}</span>
                  </button>
                )}
              </div>

              {interpretation?.summary && (
                <p className="text-[12px] text-zinc-600 leading-normal">
                  {interpretation.summary}
                </p>
              )}

              {aiResult && (
                <div className="mt-1 pt-2 border-t border-zinc-100 text-[12px] text-zinc-700 leading-relaxed bg-zinc-50 p-2.5 rounded">
                  {aiResult.explanation}
                </div>
              )}
            </div>

            {/* Connected / Related Items */}
            {relatedItems.length > 0 && (
              <div className="flex flex-col gap-1.5">
                <span className="text-[11px] font-medium text-zinc-400 px-1">
                  Related Captures ({relatedItems.length})
                </span>
                <div className="rounded-lg border border-zinc-200 bg-white divide-y divide-zinc-100">
                  {relatedItems.map((relItem) => (
                    <div
                      key={relItem.id}
                      onClick={() => onSelectItem(relItem.id)}
                      className="p-2.5 flex items-center justify-between hover:bg-zinc-50 transition-colors cursor-pointer text-[12px]"
                    >
                      <div className="flex items-center gap-2 min-w-0 flex-1">
                        <span className="text-zinc-400">
                          {relItem.item_type === 'image' ? (
                            <ImageIcon size={12} />
                          ) : relItem.item_type === 'url' ? (
                            <Globe size={12} />
                          ) : (
                            <FileText size={12} />
                          )}
                        </span>
                        <span className="text-zinc-800 truncate font-normal">
                          {relItem.title || relItem.content.slice(0, 35)}
                        </span>
                      </div>
                      <ArrowRight size={11} className="text-zinc-300 ml-2 shrink-0" />
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </>,
    document.body
  );
}
