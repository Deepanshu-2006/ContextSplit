import { useState, useRef, useEffect } from 'react';
import {
  ArrowLeft,
  Layers,
  FileText,
  Globe,
  Image as ImageIcon,
  Code as CodeIcon,
  ExternalLink,
  HelpCircle,
  Copy,
  Check,
  ChevronDown,
  ChevronUp,
  Sparkles,
  MoreHorizontal,
  BookmarkCheck,
  BookmarkMinus,
  EyeOff,
} from 'lucide-react';
import {
  ContextWorkset,
  WorksetItemReference,
  ContextMemoryControlAction,
  contextWorksetService,
} from '../../services/contextWorkset';
import { toast } from 'sonner';

interface ContextWorksetDetailViewProps {
  workset: ContextWorkset | null;
  contextName: string;
  currentTaskTitle?: string | null;
  onBack: () => void;
  onSelectItem: (itemId: string) => void;
  onWorksetUpdated?: () => void;
}

export function ContextWorksetDetailView({
  workset,
  contextName,
  currentTaskTitle,
  onBack,
  onSelectItem,
  onWorksetUpdated,
}: ContextWorksetDetailViewProps) {
  const [showGlobalWhy, setShowGlobalWhy] = useState(false);
  const [expandedWhyItemIds, setExpandedWhyItemIds] = useState<Set<string>>(new Set());

  const effectiveContextName = workset?.context_name || contextName || 'Workspace';
  const effectiveTaskTitle = workset?.task_title || currentTaskTitle || null;
  const contextId = workset?.context_id;

  const toggleItemWhy = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setExpandedWhyItemIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const allItems: WorksetItemReference[] = workset?.items || [];

  const primaryItems = allItems.filter((i) => i.relevance === 'primary');
  const supportingItems = allItems.filter((i) => i.relevance === 'supporting');
  const referenceItems = allItems.filter((i) => i.relevance === 'reference');
  const recentItems = allItems.filter((i) => i.relevance === 'recent');
  const relatedItems = allItems.filter((i) => i.relevance === 'related');

  return (
    <div className="space-y-6 animate-in fade-in duration-150 text-gray-900">
      {/* Navigation Header */}
      <div className="flex items-center justify-between pb-3 border-b border-gray-100">
        <button
          onClick={onBack}
          className="inline-flex items-center gap-1.5 text-xs font-medium text-gray-600 hover:text-gray-900 transition-colors py-1 px-2 -ml-2 rounded-md hover:bg-gray-100 cursor-pointer"
        >
          <ArrowLeft size={14} />
          <span>Back</span>
        </button>

        {workset?.explanation && (
          <button
            onClick={() => setShowGlobalWhy(!showGlobalWhy)}
            className={`inline-flex items-center gap-1 text-xs font-medium px-2.5 py-1 rounded-md transition-colors cursor-pointer border ${
              showGlobalWhy
                ? 'bg-indigo-50 text-indigo-700 border-indigo-200'
                : 'bg-gray-50 text-gray-600 hover:text-gray-900 hover:bg-gray-100 border-gray-200/80'
            }`}
          >
            <HelpCircle size={12} className={showGlobalWhy ? 'text-indigo-600' : 'text-gray-400'} />
            <span>Why?</span>
            {showGlobalWhy ? <ChevronUp size={11} /> : <ChevronDown size={11} />}
          </button>
        )}
      </div>

      {/* Global "Why?" Explanation Popover */}
      {showGlobalWhy && workset?.explanation && (
        <div className="p-3.5 bg-slate-50/95 rounded-xl border border-slate-200 text-xs space-y-2 animate-in fade-in duration-150">
          <div className="flex items-center gap-1.5 font-semibold text-slate-700 uppercase tracking-wider text-[10px]">
            <Sparkles size={12} className="text-indigo-500" />
            <span>Working Set Grounding</span>
          </div>
          {workset.explanation.why_reasons && workset.explanation.why_reasons.length > 0 && (
            <ul className="space-y-1 text-slate-700 pl-1">
              {workset.explanation.why_reasons.map((reason, idx) => (
                <li key={idx} className="flex items-start gap-2">
                  <span className="text-indigo-500 font-bold">•</span>
                  <span>{reason}</span>
                </li>
              ))}
            </ul>
          )}
          {workset.explanation.summary && (
            <p className="text-[11px] text-slate-600 pt-1 border-t border-slate-200/70 leading-relaxed">
              {workset.explanation.summary}
            </p>
          )}
        </div>
      )}

      {/* Context & Task Metadata Block */}
      <div className="space-y-3">
        <div>
          <div className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
            Context:
          </div>
          <div className="text-lg font-semibold text-gray-900 mt-0.5">
            {effectiveContextName}
          </div>
        </div>

        {effectiveTaskTitle && (
          <div>
            <div className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
              Current task:
            </div>
            <div className="text-sm font-medium text-gray-800 mt-0.5">
              {effectiveTaskTitle}
            </div>
          </div>
        )}
      </div>

      {/* WORKING SET SECTION */}
      <div className="space-y-5 pt-2 border-t border-gray-100">
        <div className="flex items-center justify-between">
          <h3 className="text-xs font-bold text-gray-900 uppercase tracking-wider">
            Working Set
          </h3>
          <span className="text-[11px] text-gray-400 font-medium">
            {allItems.length} item{allItems.length === 1 ? '' : 's'}
          </span>
        </div>

        {allItems.length === 0 ? (
          <div className="p-8 text-center text-xs text-gray-400 italic">
            No working items recorded for this context yet.
          </div>
        ) : (
          <div className="space-y-5">
            {/* Primary Section */}
            {primaryItems.length > 0 && (
              <WorksetSection
                title="Primary"
                items={primaryItems}
                contextId={contextId}
                onSelectItem={onSelectItem}
                expandedWhyItemIds={expandedWhyItemIds}
                onToggleItemWhy={toggleItemWhy}
                onWorksetUpdated={onWorksetUpdated}
              />
            )}

            {/* Supporting Section */}
            {supportingItems.length > 0 && (
              <WorksetSection
                title="Supporting"
                items={supportingItems}
                contextId={contextId}
                onSelectItem={onSelectItem}
                expandedWhyItemIds={expandedWhyItemIds}
                onToggleItemWhy={toggleItemWhy}
                onWorksetUpdated={onWorksetUpdated}
              />
            )}

            {/* References Section */}
            {referenceItems.length > 0 && (
              <WorksetSection
                title="References"
                items={referenceItems}
                contextId={contextId}
                onSelectItem={onSelectItem}
                expandedWhyItemIds={expandedWhyItemIds}
                onToggleItemWhy={toggleItemWhy}
                onWorksetUpdated={onWorksetUpdated}
              />
            )}

            {/* Recent Section */}
            {recentItems.length > 0 && (
              <WorksetSection
                title="Recent"
                items={recentItems}
                contextId={contextId}
                onSelectItem={onSelectItem}
                expandedWhyItemIds={expandedWhyItemIds}
                onToggleItemWhy={toggleItemWhy}
                onWorksetUpdated={onWorksetUpdated}
              />
            )}

            {/* Related Section (if any) */}
            {relatedItems.length > 0 && (
              <WorksetSection
                title="Related"
                items={relatedItems}
                contextId={contextId}
                onSelectItem={onSelectItem}
                expandedWhyItemIds={expandedWhyItemIds}
                onToggleItemWhy={toggleItemWhy}
                onWorksetUpdated={onWorksetUpdated}
              />
            )}
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Workset Category Section
// ---------------------------------------------------------------------------

function WorksetSection({
  title,
  items,
  contextId,
  onSelectItem,
  expandedWhyItemIds,
  onToggleItemWhy,
  onWorksetUpdated,
}: {
  title: string;
  items: WorksetItemReference[];
  contextId?: string;
  onSelectItem: (itemId: string) => void;
  expandedWhyItemIds: Set<string>;
  onToggleItemWhy: (id: string, e: React.MouseEvent) => void;
  onWorksetUpdated?: () => void;
}) {
  return (
    <div className="space-y-2">
      <div className="text-xs font-semibold text-gray-700 flex items-center justify-between">
        <span>{title}</span>
        <span className="text-[10px] text-gray-400 font-normal">({items.length})</span>
      </div>

      <div className="space-y-1.5">
        {items.map((item) => (
          <WorksetDetailItemRow
            key={item.id}
            item={item}
            contextId={contextId}
            onSelect={() => onSelectItem(item.id)}
            isWhyExpanded={expandedWhyItemIds.has(item.id)}
            onToggleWhy={(e) => onToggleItemWhy(item.id, e)}
            onWorksetUpdated={onWorksetUpdated}
          />
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Actionable Workset Item Row with Subtle Feedback Menu
// ---------------------------------------------------------------------------

function WorksetDetailItemRow({
  item,
  contextId,
  onSelect,
  isWhyExpanded,
  onToggleWhy,
  onWorksetUpdated,
}: {
  item: WorksetItemReference;
  contextId?: string;
  onSelect: () => void;
  isWhyExpanded: boolean;
  onToggleWhy: (e: React.MouseEvent) => void;
  onWorksetUpdated?: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const getItemIcon = (type: string, appName?: string | null) => {
    const lowerApp = (appName || '').toLowerCase();
    if (lowerApp.includes('figma')) return <Layers size={14} className="text-purple-500" />;
    if (lowerApp.includes('github') || lowerApp.includes('code') || lowerApp.includes('terminal')) {
      return <CodeIcon size={14} className="text-gray-700" />;
    }
    switch (type) {
      case 'url':
        return <Globe size={14} className="text-blue-500" />;
      case 'code':
        return <CodeIcon size={14} className="text-emerald-600" />;
      case 'image':
        return <ImageIcon size={14} className="text-pink-500" />;
      default:
        return <FileText size={14} className="text-gray-500" />;
    }
  };

  const handleCopy = (e: React.MouseEvent) => {
    e.stopPropagation();
    const textToCopy = item.content_snippet || item.title || '';
    if (!textToCopy) return;
    navigator.clipboard.writeText(textToCopy);
    setCopied(true);
    toast.success('Copied snippet');
    setTimeout(() => setCopied(false), 2000);
  };

  const displayTitle = item.title || item.content_snippet || 'Untitled Item';
  const hasReasons = item.reasons && item.reasons.length > 0;

  return (
    <div className="rounded-lg border border-gray-200/80 bg-white hover:border-gray-300 transition-all overflow-hidden shadow-2xs group">
      <div
        onClick={onSelect}
        className="p-3 flex items-center justify-between gap-3 cursor-pointer"
      >
        <div className="flex items-center gap-3 min-w-0">
          <div className="w-7 h-7 rounded bg-gray-50 border border-gray-200 group-hover:border-gray-300 flex items-center justify-center flex-shrink-0">
            {getItemIcon(item.item_type, item.app_name)}
          </div>
          <div className="min-w-0">
            <div className="text-xs font-medium text-gray-900 truncate">
              {displayTitle}
            </div>
            <div className="flex items-center gap-2 text-[10px] text-gray-400 mt-0.5">
              {item.app_name && <span>{item.app_name}</span>}
              {item.domain && <span>{item.domain}</span>}
              {item.is_suggested_addition && (
                <span className="text-amber-600 font-medium">Suggested</span>
              )}
            </div>
          </div>
        </div>

        {/* Action Buttons */}
        <div className="flex items-center gap-1 flex-shrink-0">
          {/* Quick Copy */}
          <button
            onClick={handleCopy}
            className="p-1 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded transition-colors"
            title="Copy content snippet"
          >
            {copied ? <Check size={13} className="text-emerald-600" /> : <Copy size={13} />}
          </button>

          {/* External URL link if applicable */}
          {item.item_type === 'url' && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                if (item.content_snippet.startsWith('http')) {
                  window.open(item.content_snippet, '_blank');
                } else {
                  onSelect();
                }
              }}
              className="p-1 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded transition-colors"
              title="Open link"
            >
              <ExternalLink size={13} />
            </button>
          )}

          {/* Item "Why?" toggle */}
          {hasReasons && (
            <button
              onClick={onToggleWhy}
              className={`p-1 rounded text-[11px] font-medium transition-colors flex items-center gap-0.5 ${
                isWhyExpanded
                  ? 'bg-indigo-50 text-indigo-700'
                  : 'text-gray-400 hover:text-gray-700 hover:bg-gray-100'
              }`}
              title="Why is this item in the working set?"
            >
              <HelpCircle size={13} />
              {isWhyExpanded ? <ChevronUp size={11} /> : <ChevronDown size={11} />}
            </button>
          )}

          {/* Subtle Menu: Keep / Remove from workset */}
          <WorksetItemActionsMenu
            itemId={item.id}
            contextId={contextId}
            onWorksetUpdated={onWorksetUpdated}
          />
        </div>
      </div>

      {/* Item-level Explainable Reasons (Revealed via "Why?") */}
      {isWhyExpanded && hasReasons && (
        <div className="px-3 py-2 bg-slate-50 border-t border-slate-100 text-[11px] text-slate-600 space-y-1 animate-in fade-in duration-100">
          <div className="font-semibold text-slate-700 text-[10px] uppercase tracking-wider flex items-center gap-1">
            <Sparkles size={11} className="text-indigo-500" />
            <span>Relevance reasons</span>
          </div>
          <ul className="space-y-0.5 pl-1 text-slate-700">
            {item.reasons.map((reason, idx) => (
              <li key={idx} className="flex items-start gap-1.5">
                <span className="text-indigo-500 font-bold">•</span>
                <span>{reason}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Subtle Workset Item Actions Menu (⋯)
// ---------------------------------------------------------------------------

export function WorksetItemActionsMenu({
  itemId,
  contextId,
  onWorksetUpdated,
}: {
  itemId: string;
  contextId?: string;
  onWorksetUpdated?: () => void;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [isOpen]);

  const handleAction = async (action: ContextMemoryControlAction, e: React.MouseEvent) => {
    e.stopPropagation();
    setIsOpen(false);
    if (!contextId) return;

    try {
      const ok = await contextWorksetService.controlContextMemory(contextId, itemId, action);
      if (ok) {
        if (action === 'keep_important') {
          toast.success('Kept as important memory');
        } else if (action === 'remove_important') {
          toast.success('Removed from important memory');
        } else if (action === 'remove_from_context') {
          toast.success('Removed from context (item remains saved)');
        }
        onWorksetUpdated?.();
      } else {
        toast.error('Failed to update contextual memory');
      }
    } catch (err) {
      console.error('Failed to update context memory control:', err);
      toast.error('Failed to update contextual memory');
    }
  };

  return (
    <div className="relative" ref={menuRef}>
      <button
        onClick={(e) => {
          e.stopPropagation();
          setIsOpen(!isOpen);
        }}
        className="p-1 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded transition-colors cursor-pointer"
        title="Context memory options"
      >
        <MoreHorizontal size={14} />
      </button>

      {isOpen && (
        <div
          onClick={(e) => e.stopPropagation()}
          className="absolute right-0 top-full mt-1 w-48 bg-white rounded-lg border border-gray-200 shadow-lg py-1 z-30 text-xs animate-in fade-in-0 zoom-in-95 duration-100"
        >
          <button
            onClick={(e) => handleAction('keep_important', e)}
            className="w-full text-left px-3 py-1.5 text-gray-700 hover:bg-gray-50 flex items-center gap-2 transition-colors cursor-pointer"
          >
            <BookmarkCheck size={13} className="text-emerald-600" />
            <span>Keep important</span>
          </button>
          <button
            onClick={(e) => handleAction('remove_important', e)}
            className="w-full text-left px-3 py-1.5 text-amber-700 hover:bg-amber-50/70 flex items-center gap-2 transition-colors cursor-pointer"
          >
            <BookmarkMinus size={13} className="text-amber-500" />
            <span>Remove from important</span>
          </button>
          <button
            onClick={(e) => handleAction('remove_from_context', e)}
            className="w-full text-left px-3 py-1.5 text-red-600 hover:bg-red-50 flex items-center gap-2 transition-colors cursor-pointer border-t border-gray-100 mt-0.5 pt-1.5"
          >
            <EyeOff size={13} className="text-red-500" />
            <span>Remove from context</span>
          </button>
        </div>
      )}
    </div>
  );
}
