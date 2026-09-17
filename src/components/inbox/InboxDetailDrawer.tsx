import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  ExternalLink,
  Copy,
  Check,
  Plus,
  Trash2,
  Sparkles,
  ArrowRight,
  FileText,
  Image as ImageIcon,
  Link2,
  FileCode,
  AppWindow,
  ChevronDown,
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
  const { acceptItemSuggestion, dismissItemSuggestion, markItemUnderstood } = useInboxStore();
  const { reviewCandidate } = useDiscoveryStore();

  const [copied, setCopied] = useState(false);
  const [relationships, setRelationships] = useState<ItemRelationship[]>([]);
  const [relatedItems, setRelatedItems] = useState<Item[]>([]);
  const [interpretation, setInterpretation] = useState<UnifiedContextInterpretation | null>(null);
  const [aiResult, setAiResult] = useState<ContextReasoningResult | null>(null);
  const [isAiLoading, setIsAiLoading] = useState(false);
  const [isAssignDropdownOpen, setIsAssignDropdownOpen] = useState(false);
  const [isActing, setIsActing] = useState(false);

  const item = derived?.item;

  // Load contextual evidence and relationships whenever item changes
  useEffect(() => {
    if (!item) return;

    setCopied(false);
    setAiResult(null);
    setIsAssignDropdownOpen(false);

    // Fetch relationships and context evaluation
    let isMounted = true;

    Promise.all([
      db.getStrongestRelationships(item.id, 5).catch(() => [] as ItemRelationship[]),
      db.evaluateItemContext(item.id).catch(() => null),
    ]).then(async ([rels, interp]) => {
      if (!isMounted) return;
      setRelationships(rels);
      setInterpretation(interp);

      // Fetch the actual Item objects for related items
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

  // Handle Escape key to close drawer
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  if (!derived || !item) return null;

  const { attention, candidateInfo, inboxState } = derived;
  const suggestedWsId = attention?.factors?.suggested_workspace_id;
  const suggestedWs = workspaces.find((w) => w.id === suggestedWsId);
  const currentWs = workspaces.find((w) => w.id === item.workspace_id);

  // Parse metadata
  let appName: string | null = null;
  let sourceUrl: string | null = null;
  try {
    const meta = JSON.parse(item.metadata || '{}');
    appName = meta.app_name || meta.source_app || null;
    sourceUrl = meta.source_url || meta.url || null;
  } catch {}

  // Format relative timestamp
  const getFormattedTime = (dateStr: string) => {
    try {
      const date = new Date(dateStr);
      const now = new Date();
      const diffMs = now.getTime() - date.getTime();
      const diffMins = Math.floor(diffMs / (1000 * 60));
      const diffHours = Math.floor(diffMins / 60);
      const diffDays = Math.floor(diffHours / 24);

      if (diffMins < 1) return 'Captured just now';
      if (diffMins < 60) return `Captured ${diffMins} min ago`;
      if (diffHours < 24) return `Captured ${diffHours} hr ago`;
      if (diffDays === 1) return 'Captured yesterday';
      return `Captured on ${date.toLocaleDateString()}`;
    } catch {
      return '';
    }
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(item.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
      toast.success('Copied to clipboard');
    } catch (e) {
      console.warn('Failed to copy', e);
    }
  };

  const handleAcceptSuggestion = async (wsId: string) => {
    setIsActing(true);
    try {
      await acceptItemSuggestion(item.id, wsId);
      toast.success(`Added to ${workspaces.find((w) => w.id === wsId)?.name || 'workspace'}`);
      onClose();
    } catch (e) {
      console.error(e);
      toast.error('Failed to accept suggestion');
    } finally {
      setIsActing(false);
    }
  };

  const handleDismiss = async () => {
    setIsActing(true);
    try {
      await dismissItemSuggestion(item.id, suggestedWsId || undefined);
      toast.success('Suggestion dismissed');
    } catch (e) {
      console.error(e);
      toast.error('Failed to dismiss suggestion');
    } finally {
      setIsActing(false);
    }
  };

  const handleMarkReviewed = async () => {
    setIsActing(true);
    try {
      await markItemUnderstood(item.id, 'User marked as reviewed');
      toast.success('Marked as reviewed');
    } catch (e) {
      console.error(e);
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
            toast.error('Failed to restore item');
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
        toast.info(resp.unavailability_reason || 'AI Reasoning unavailable');
      }
    } catch (e) {
      console.error('Failed to run AI reasoning', e);
      toast.error('Could not run AI reasoning');
    } finally {
      setIsAiLoading(false);
    }
  };

  // Determine context presentation
  const confidencePct = Math.round((attention?.factors?.contextual_confidence ?? 0) * 100);
  const isSuggested = Boolean(suggestedWsId && confidencePct > 0);
  const isCandidate = Boolean(candidateInfo);
  const isNeedsReview = Boolean(
    attention?.factors?.is_ambiguous || derived.section === 'needs_attention' || inboxState?.state === 'needs_review'
  );
  const isConfirmed = Boolean(currentWs);

  // Extract explanation reasons
  const explanationBullets: string[] = [];
  if (appName) {
    explanationBullets.push(`Captured while working in ${appName}`);
  }
  if (sourceUrl) {
    try {
      const urlObj = new URL(sourceUrl);
      explanationBullets.push(`Associated with domain ${urlObj.hostname}`);
    } catch {
      explanationBullets.push(`Associated with URL ${sourceUrl}`);
    }
  }
  if (attention?.reason) {
    explanationBullets.push(attention.reason);
  }
  if (interpretation?.summary && !explanationBullets.includes(interpretation.summary)) {
    explanationBullets.push(interpretation.summary);
  }
  if (relationships.length > 0) {
    explanationBullets.push(`Connected to ${relationships.length} related recent items in this context session`);
  }
  if (explanationBullets.length === 0) {
    explanationBullets.push('Content similarities and temporal co-occurrence with recent captures');
  }

  const getItemIcon = () => {
    switch (item.item_type) {
      case 'image':
        return <ImageIcon size={15} className="text-purple-600" />;
      case 'url':
        return <Link2 size={15} className="text-blue-600" />;
      case 'file':
        return <FileCode size={15} className="text-gray-600" />;
      default:
        return <FileText size={15} className="text-gray-600" />;
    }
  };

  return createPortal(
    <div className="fixed inset-0 z-40 flex justify-end pointer-events-auto">
      {/* Semi-transparent backdrop */}
      <div
        className="fixed inset-0 bg-black/15 backdrop-blur-[1px] transition-opacity duration-200"
        onClick={onClose}
      />

      {/* Slide-over Drawer Panel */}
      <div
        className="relative w-full max-w-md h-full bg-white border-l border-black/10 shadow-2xl flex flex-col z-50 overflow-hidden animate-in slide-in-from-right duration-200"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Drawer Header */}
        <div className="px-6 py-4 border-b border-black/6 flex items-center justify-between shrink-0 bg-[#FAFAFA]">
          <div className="flex items-center gap-2.5 min-w-0">
            <div className="w-7 h-7 rounded-lg bg-black/5 flex items-center justify-center shrink-0">
              {getItemIcon()}
            </div>
            <div className="min-w-0">
              <h2 className="text-[14px] font-semibold text-gray-900 truncate">
                {item.title || 'Captured Item'}
              </h2>
              <p className="text-[11px] text-gray-400 font-normal">
                {getFormattedTime(item.created_at)}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-1">
            <button
              onClick={handleDeleteItem}
              title="Delete item"
              className="p-1.5 text-gray-400 hover:text-red-600 hover:bg-red-50 rounded-lg transition-colors cursor-pointer"
            >
              <Trash2 size={15} />
            </button>
            <button
              onClick={onClose}
              title="Close panel (Esc)"
              className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-black/5 rounded-lg transition-colors cursor-pointer"
            >
              <X size={16} />
            </button>
          </div>
        </div>

        {/* Drawer Scrollable Content */}
        <div className="flex-1 overflow-y-auto px-6 py-5 flex flex-col gap-6 text-[13px]">
          {/* SECTION: CONTENT */}
          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
                Content
              </span>
              <button
                onClick={handleCopy}
                className="flex items-center gap-1 text-[11px] font-medium text-gray-500 hover:text-gray-800 transition-colors cursor-pointer"
              >
                {copied ? (
                  <>
                    <Check size={12} className="text-emerald-600" />
                    <span className="text-emerald-600">Copied</span>
                  </>
                ) : (
                  <>
                    <Copy size={12} />
                    <span>Copy</span>
                  </>
                )}
              </button>
            </div>

            {/* Content Rendering */}
            {item.item_type === 'image' ? (
              <div className="rounded-xl overflow-hidden border border-black/8 bg-gray-50 flex items-center justify-center p-1">
                <img
                  src={item.content}
                  alt={item.title || 'Captured image'}
                  className="w-full h-auto max-h-72 object-contain rounded-lg"
                />
              </div>
            ) : item.item_type === 'url' ? (
              <div className="p-3 bg-blue-50/40 rounded-xl border border-blue-100 flex flex-col gap-1.5">
                <a
                  href={item.content}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-sm font-semibold text-blue-600 hover:text-blue-800 flex items-center gap-1.5 break-all"
                >
                  <span>{item.title || item.content}</span>
                  <ExternalLink size={13} className="shrink-0 opacity-70" />
                </a>
                <span className="text-[11px] text-gray-400 font-mono break-all select-all">
                  {item.content}
                </span>
              </div>
            ) : (
              <div className="bg-[#FBFBFB] rounded-xl border border-black/6 p-3.5 select-text text-gray-800 leading-relaxed font-sans text-[13px] whitespace-pre-wrap break-words max-h-60 overflow-y-auto">
                {item.content}
              </div>
            )}

            {/* Source Application / Origin */}
            {appName && (
              <div className="flex items-center gap-1.5 text-[11px] text-gray-400 pt-0.5">
                <AppWindow size={12} />
                <span>Captured from {appName}</span>
              </div>
            )}
          </div>

          <div className="h-px bg-black/5" />

          {/* SECTION: CONTEXT */}
          <div className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
                Context
              </span>
              {confidencePct > 0 && (
                <span className="text-[11px] font-semibold text-gray-500 font-mono">
                  {confidencePct}% confidence
                </span>
              )}
            </div>

            {/* Context Identification Card */}
            <div className="p-3.5 bg-gray-50/80 rounded-xl border border-black/5 flex flex-col gap-2">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <div className="w-2 h-2 rounded-full bg-indigo-500" />
                  <span className="text-[13px] font-semibold text-gray-900">
                    {isConfirmed
                      ? currentWs?.name
                      : isSuggested
                      ? suggestedWs?.name || 'Suggested Context'
                      : isCandidate
                      ? candidateInfo?.candidateName
                      : isNeedsReview
                      ? 'Needs Review'
                      : 'No context yet'}
                  </span>
                </div>
                <span
                  className={`text-[10px] font-semibold px-2 py-0.5 rounded-full border ${
                    isConfirmed
                      ? 'bg-emerald-50 text-emerald-700 border-emerald-200'
                      : isSuggested
                      ? 'bg-indigo-50 text-indigo-700 border-indigo-200'
                      : isCandidate
                      ? 'bg-cyan-50 text-cyan-700 border-cyan-200'
                      : isNeedsReview
                      ? 'bg-amber-50 text-amber-700 border-amber-200'
                      : 'bg-gray-100 text-gray-600 border-gray-200'
                  }`}
                >
                  {isConfirmed
                    ? 'Confirmed'
                    : isSuggested
                    ? 'Suggested'
                    : isCandidate
                    ? 'Candidate'
                    : isNeedsReview
                    ? 'Review'
                    : 'Unassigned'}
                </span>
              </div>

              {/* Why ContextSplit thinks this */}
              <div className="pt-2 border-t border-black/4 flex flex-col gap-1.5">
                <span className="text-[11px] font-semibold text-gray-500">
                  Why ContextSplit thinks this:
                </span>
                <ul className="space-y-1 text-[12px] text-gray-600">
                  {explanationBullets.map((bullet, idx) => (
                    <li key={idx} className="flex items-start gap-1.5">
                      <span className="text-gray-400 mt-0.5">•</span>
                      <span>{bullet}</span>
                    </li>
                  ))}
                </ul>
              </div>

              {/* AI Context Reasoning if triggered */}
              {aiResult ? (
                <div className="mt-2 pt-2 border-t border-indigo-100 bg-indigo-50/50 p-2.5 rounded-lg text-[12px] text-indigo-900">
                  <div className="flex items-center gap-1.5 font-semibold text-indigo-700 mb-1">
                    <Sparkles size={12} className="text-amber-500" />
                    <span>AI Reasoning Analysis</span>
                  </div>
                  <p className="italic text-gray-700 leading-relaxed">
                    "{aiResult.explanation}"
                  </p>
                </div>
              ) : (
                <div className="pt-1">
                  <button
                    onClick={handleRunAiReasoning}
                    disabled={isAiLoading}
                    className="inline-flex items-center gap-1.5 text-[11px] font-medium text-indigo-600 hover:text-indigo-800 transition-colors cursor-pointer"
                  >
                    <Sparkles size={12} className={isAiLoading ? 'animate-spin' : ''} />
                    <span>{isAiLoading ? 'Analyzing Context...' : 'Run deep AI explanation'}</span>
                  </button>
                </div>
              )}
            </div>
          </div>

          <div className="h-px bg-black/5" />

          {/* SECTION: ACTIONS */}
          <div className="flex flex-col gap-2.5">
            <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
              Actions
            </span>

            <div className="flex flex-col gap-2">
              {/* Primary Action Button */}
              {isSuggested && suggestedWsId && (
                <button
                  onClick={() => handleAcceptSuggestion(suggestedWsId)}
                  disabled={isActing}
                  className="w-full flex items-center justify-center gap-2 py-2 px-3 bg-gray-900 hover:bg-black text-white rounded-xl text-[12px] font-semibold shadow-xs transition-colors cursor-pointer"
                >
                  <Plus size={14} strokeWidth={2.5} />
                  <span>Add to {suggestedWs?.name || 'Workspace'}</span>
                </button>
              )}

              {isCandidate && candidateInfo && (
                <button
                  onClick={() => {
                    reviewCandidate(candidateInfo.candidateId);
                    onClose();
                  }}
                  className="w-full flex items-center justify-center gap-2 py-2 px-3 bg-cyan-600 hover:bg-cyan-700 text-white rounded-xl text-[12px] font-semibold shadow-xs transition-colors cursor-pointer"
                >
                  <Sparkles size={14} />
                  <span>Review Candidate "{candidateInfo.candidateName}"</span>
                </button>
              )}

              {/* Secondary Actions */}
              <div className="flex items-center gap-2">
                {isSuggested && (
                  <button
                    onClick={handleDismiss}
                    disabled={isActing}
                    className="flex-1 py-1.5 px-3 bg-white hover:bg-gray-50 text-gray-700 border border-black/10 rounded-xl text-[12px] font-medium transition-colors cursor-pointer text-center"
                  >
                    Dismiss
                  </button>
                )}

                {isNeedsReview && (
                  <button
                    onClick={handleMarkReviewed}
                    disabled={isActing}
                    className="flex-1 py-1.5 px-3 bg-white hover:bg-gray-50 text-gray-700 border border-black/10 rounded-xl text-[12px] font-medium transition-colors cursor-pointer text-center"
                  >
                    Mark as reviewed
                  </button>
                )}

                {/* Move to another workspace dropdown */}
                <div className="relative flex-1">
                  <button
                    onClick={() => setIsAssignDropdownOpen(!isAssignDropdownOpen)}
                    className="w-full flex items-center justify-between py-1.5 px-3 bg-white hover:bg-gray-50 text-gray-700 border border-black/10 rounded-xl text-[12px] font-medium transition-colors cursor-pointer"
                  >
                    <span className="truncate">Move to...</span>
                    <ChevronDown size={12} className="text-gray-400" />
                  </button>

                  {isAssignDropdownOpen && (
                    <div className="absolute right-0 bottom-full mb-1 w-52 bg-white border border-black/10 rounded-xl shadow-xl py-1 z-30 max-h-48 overflow-y-auto">
                      <div className="px-3 py-1 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
                        Workspaces
                      </div>
                      {workspaces.map((ws) => (
                        <button
                          key={ws.id}
                          onClick={() => {
                            onMoveToWorkspace(item.id, ws.id);
                            setIsAssignDropdownOpen(false);
                            onClose();
                          }}
                          className="w-full text-left px-3 py-1.5 text-[12px] text-gray-700 hover:bg-black/5 flex items-center gap-2 transition-colors cursor-pointer"
                        >
                          <span className="truncate">{ws.name}</span>
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </div>
          </div>

          {/* SECTION: RELATED ITEMS (Only shown here in detail view) */}
          {relatedItems.length > 0 && (
            <>
              <div className="h-px bg-black/5" />
              <div className="flex flex-col gap-2.5">
                <div className="flex items-center justify-between">
                  <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
                    Related Items ({relatedItems.length})
                  </span>
                  <span className="text-[10px] text-gray-400">From same session / context</span>
                </div>

                <div className="flex flex-col gap-1.5">
                  {relatedItems.map((relItem) => {
                    const rel = relationships.find(
                      (r) => r.item_a_id === relItem.id || r.item_b_id === relItem.id
                    );
                    const percent = rel ? Math.round(rel.strength * 100) : null;

                    return (
                      <div
                        key={relItem.id}
                        onClick={() => onSelectItem(relItem.id)}
                        className="group flex items-center justify-between p-2.5 bg-gray-50/70 hover:bg-gray-100/80 rounded-xl border border-black/5 transition-colors cursor-pointer"
                      >
                        <div className="flex items-center gap-2 min-w-0 flex-1">
                          <span className="text-gray-400 group-hover:text-gray-600">
                            {relItem.item_type === 'image' ? (
                              <ImageIcon size={13} />
                            ) : relItem.item_type === 'url' ? (
                              <Link2 size={13} />
                            ) : (
                              <FileText size={13} />
                            )}
                          </span>
                          <span className="text-[12px] font-medium text-gray-800 truncate">
                            {relItem.title || relItem.content.slice(0, 40)}
                          </span>
                        </div>

                        {percent !== null && (
                          <div className="flex items-center gap-1.5 shrink-0 ml-2">
                            <span className="text-[10px] font-semibold text-gray-400 font-mono">
                              {percent}%
                            </span>
                            <ArrowRight size={11} className="text-gray-300 group-hover:text-gray-600" />
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            </>
          )}
        </div>
      </div>
    </div>,
    document.body
  );
}
