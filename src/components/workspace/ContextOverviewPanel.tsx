import { useState, useEffect, useMemo } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  Clock,
  Layers,
  FileText,
  Globe,
  Image as ImageIcon,
  Code as CodeIcon,
  Search,
  Activity,
  RefreshCw,
  Archive,
  ArchiveRestore,
  ExternalLink,
  Laptop,
  CheckCircle2,
  AlertCircle,
  Inbox,
  Folder,
  ChevronDown,
  ChevronUp,
  RotateCcw,
  HelpCircle,
  Sparkles,
  Info,
  Check,
  Copy,
  Calendar,
  ArrowRight,
} from 'lucide-react';
import { db, ContextOverview, TimelineEvent, TimelineItemSummary, Item } from '../../services/db';
import { contextMemoryService, ContextMemoryResponse } from '../../services/contextMemory';
import { contextWorksetService, ContextWorkset, WorksetItemReference } from '../../services/contextWorkset';
import { ContextWorksetDetailView } from './ContextWorksetDetailView';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { toast } from 'sonner';

export function ContextOverviewPanel() {
  const {
    isOverviewOpen,
    overviewWorkspaceId,
    closeOverview,
    workspaces,
    loadWorkspaces,
    openTimeline,
  } = useWorkspaceStore();

  const [overview, setOverview] = useState<ContextOverview | null>(null);
  const [memory, setMemory] = useState<ContextMemoryResponse | null>(null);
  const [workset, setWorkset] = useState<ContextWorkset | null>(null);
  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isUpdatingLifecycle, setIsUpdatingLifecycle] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'overview' | 'workset' | 'timeline'>('overview');
  const [timelineSearch, setTimelineSearch] = useState('');

  const targetWorkspace = workspaces.find((w) => w.id === overviewWorkspaceId);

  useEffect(() => {
    if (isOverviewOpen && overviewWorkspaceId) {
      loadOverviewAndMemory(overviewWorkspaceId);
    } else {
      setOverview(null);
      setMemory(null);
      setWorkset(null);
      setSelectedItemId(null);
    }
  }, [isOverviewOpen, overviewWorkspaceId]);

  const loadOverviewAndMemory = async (wsId: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const [overviewData, memoryData, worksetData] = await Promise.all([
        db.getContextOverview(wsId).catch(() => null),
        contextMemoryService.getContextMemory(wsId).catch(() => null),
        contextWorksetService.getContextWorkset(wsId).catch(() => null),
      ]);
      setOverview(overviewData);
      setMemory(memoryData);
      setWorkset(worksetData);
    } catch (err: any) {
      setError(err?.toString() || 'Failed to load context overview');
    } finally {
      setIsLoading(false);
    }
  };

  // Close on Escape or return to overview
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (selectedItemId) {
          setSelectedItemId(null);
        } else if (activeTab === 'workset') {
          setActiveTab('overview');
        } else if (isOverviewOpen) {
          closeOverview();
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOverviewOpen, selectedItemId, activeTab, closeOverview]);

  const handleToggleArchive = async () => {
    if (!overview) return;
    setIsUpdatingLifecycle(true);
    try {
      if (overview.lifecycle_state === 'archived') {
        await db.unarchiveWorkspace(overview.workspace_id);
        toast.success(`Reactivated context "${overview.name}"`);
      } else {
        await db.archiveWorkspace(overview.workspace_id);
        toast.success(`Archived context "${overview.name}"`);
      }
      await loadWorkspaces();
      await loadOverviewAndMemory(overview.workspace_id);
    } catch (err: any) {
      toast.error(`Failed to update lifecycle: ${err?.message || err}`);
    } finally {
      setIsUpdatingLifecycle(false);
    }
  };

  const filteredTimelineEvents = useMemo(() => {
    if (!overview?.timeline?.chronological_activity) return [];
    if (!timelineSearch.trim()) return overview.timeline.chronological_activity;

    const q = timelineSearch.toLowerCase();
    return overview.timeline.chronological_activity.filter((ev) =>
      ev.title.toLowerCase().includes(q) || ev.description.toLowerCase().includes(q)
    );
  }, [overview, timelineSearch]);

  if (!isOverviewOpen || !overviewWorkspaceId) return null;

  return createPortal(
    <div className="fixed inset-0 z-50 flex justify-end bg-black/30 backdrop-blur-xs animate-in fade-in duration-200">
      {/* Backdrop click to close */}
      <div className="flex-1" onClick={closeOverview} />

      {/* Slide-over Drawer Panel */}
      <div className="w-full max-w-xl h-full bg-white shadow-xl flex flex-col border-l border-gray-200 animate-in slide-in-from-right duration-200">
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-100 flex items-center justify-between bg-white">
          <div className="flex items-center gap-3 min-w-0">
            <div className="w-9 h-9 rounded-lg bg-gray-100 flex items-center justify-center text-gray-700 flex-shrink-0">
              <Layers size={18} />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <h2 className="text-base font-semibold text-gray-900 truncate">
                  {overview?.name || memory?.context || targetWorkspace?.name || 'Context Overview'}
                </h2>
                {overview && (
                  <LifecycleBadge state={overview.lifecycle_state} />
                )}
              </div>
              <p className="text-xs text-gray-500 flex items-center gap-2 mt-0.5">
                <span>Working Context Overview</span>
                <span>•</span>
                <span>Resume & Memory</span>
              </p>
            </div>
          </div>

          <div className="flex items-center gap-1.5">
            <button
              onClick={() => overviewWorkspaceId && loadOverviewAndMemory(overviewWorkspaceId)}
              disabled={isLoading}
              className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-md transition-colors cursor-pointer"
              title="Refresh Overview"
            >
              <RefreshCw size={15} className={isLoading ? 'animate-spin text-gray-700' : ''} />
            </button>
            <button
              onClick={closeOverview}
              className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-md transition-colors cursor-pointer"
              title="Close panel (Esc)"
            >
              <X size={17} />
            </button>
          </div>
        </div>

        {/* Tab switcher */}
        <div className="flex border-b border-gray-100 px-6 bg-white">
          <button
            onClick={() => setActiveTab('overview')}
            className={`py-2.5 px-3 text-xs font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
              activeTab === 'overview'
                ? 'border-gray-900 text-gray-900'
                : 'border-transparent text-gray-500 hover:text-gray-800'
            }`}
          >
            <Activity size={13} />
            <span>Resume & Overview</span>
          </button>
          <button
            onClick={() => setActiveTab('workset')}
            className={`py-2.5 px-3 text-xs font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
              activeTab === 'workset'
                ? 'border-gray-900 text-gray-900'
                : 'border-transparent text-gray-500 hover:text-gray-800'
            }`}
          >
            <Layers size={13} />
            <span>Working Set ({workset?.items.length ?? 0})</span>
          </button>
          <button
            onClick={() => setActiveTab('timeline')}
            className={`py-2.5 px-3 text-xs font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
              activeTab === 'timeline'
                ? 'border-gray-900 text-gray-900'
                : 'border-transparent text-gray-500 hover:text-gray-800'
            }`}
          >
            <Clock size={13} />
            <span>Activity Timeline ({overview?.timeline.chronological_activity.length ?? 0})</span>
          </button>
        </div>

        {/* Content Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {isLoading && !overview && !memory ? (
            <div className="h-64 flex flex-col items-center justify-center text-gray-400 gap-2">
              <RefreshCw size={20} className="animate-spin text-gray-500" />
              <p className="text-xs">Reconstructing working memory...</p>
            </div>
          ) : error ? (
            <div className="p-4 bg-red-50 text-red-700 rounded-xl border border-red-200 text-sm flex items-start gap-3">
              <AlertCircle size={18} className="flex-shrink-0 mt-0.5" />
              <div>
                <div className="font-semibold">Unable to load overview</div>
                <div className="text-xs text-red-600 mt-1">{error}</div>
              </div>
            </div>
          ) : (
            activeTab === 'overview' ? (
              <ResumeMemoryOverviewContent
                overview={overview}
                memory={memory}
                workset={workset}
                targetWorkspaceName={targetWorkspace?.name}
                onToggleArchive={handleToggleArchive}
                isUpdatingLifecycle={isUpdatingLifecycle}
                onSelectItem={(itemId) => setSelectedItemId(itemId)}
                onOpenWorksetDetail={() => setActiveTab('workset')}
                onViewFullTimeline={() => {
                  closeOverview();
                  if (overview) {
                    openTimeline(overview.workspace_id);
                  }
                }}
                onMemoryFeedbackRecorded={() => {
                  if (overviewWorkspaceId) {
                    loadOverviewAndMemory(overviewWorkspaceId);
                  }
                }}
              />
            ) : activeTab === 'workset' ? (
              <ContextWorksetDetailView
                workset={workset}
                contextName={overview?.name || memory?.context || targetWorkspace?.name || 'Workspace'}
                currentTaskTitle={
                  memory?.task ||
                  workset?.task_title ||
                  (overview?.current_task?.is_confident && overview.current_task.title !== 'Not clear yet'
                    ? overview.current_task.title
                    : null)
                }
                onBack={() => setActiveTab('overview')}
                onSelectItem={(itemId) => setSelectedItemId(itemId)}
              />
            ) : (
              <TimelineTabContent
                events={filteredTimelineEvents}
                searchQuery={timelineSearch}
                onSearchChange={setTimelineSearch}
              />
            )
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-3 border-t border-gray-100 bg-gray-50/50 flex items-center justify-between text-[11px] text-gray-400">
          <span className="flex items-center gap-1.5">
            <CheckCircle2 size={12} className="text-emerald-500" />
            Human-readable contextual working memory
          </span>
          <span>Context Engine</span>
        </div>
      </div>

      {/* Item Detail Modal Preview */}
      {selectedItemId && (
        <ContextItemDetailModal
          itemId={selectedItemId}
          onClose={() => setSelectedItemId(null)}
        />
      )}
    </div>,
    document.body
  );
}

// ---------------------------------------------------------------------------
// Subcomponents
// ---------------------------------------------------------------------------

function LifecycleBadge({ state }: { state: 'active' | 'dormant' | 'archived' }) {
  if (state === 'active') {
    return (
      <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-medium bg-emerald-50 text-emerald-700 border border-emerald-200/80">
        <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
        Active
      </span>
    );
  }
  if (state === 'dormant') {
    return (
      <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-medium bg-amber-50 text-amber-700 border border-amber-200/80">
        <span className="w-1.5 h-1.5 rounded-full bg-amber-500" />
        Dormant
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-medium bg-gray-100 text-gray-600 border border-gray-200">
      <Archive size={11} />
      Archived
    </span>
  );
}

interface ResumeMemoryOverviewContentProps {
  overview: ContextOverview | null;
  memory: ContextMemoryResponse | null;
  workset: ContextWorkset | null;
  targetWorkspaceName?: string;
  onToggleArchive: () => void;
  isUpdatingLifecycle: boolean;
  onSelectItem: (itemId: string) => void;
  onOpenWorksetDetail: () => void;
  onViewFullTimeline: () => void;
  onMemoryFeedbackRecorded?: () => void;
}

function ResumeMemoryOverviewContent({
  overview,
  memory,
  workset,
  targetWorkspaceName,
  onToggleArchive,
  isUpdatingLifecycle,
  onSelectItem,
  onOpenWorksetDetail,
  onViewFullTimeline,
  onMemoryFeedbackRecorded,
}: ResumeMemoryOverviewContentProps) {
  const [showRecentWork, setShowRecentWork] = useState(false);
  const [showWhy, setShowWhy] = useState(false);
  const [isViewAllWorkset, setIsViewAllWorkset] = useState(false);
  const [feedbackStatus, setFeedbackStatus] = useState<'idle' | 'submitting' | 'submitted'>('idle');

  const contextId = overview?.workspace_id || memory?.context_id || workset?.context_id;
  const contextName = overview?.name || memory?.context || workset?.context_name || targetWorkspaceName || 'Workspace';

  // Determine if we have a confident task
  const rawTask = memory?.task || workset?.task_title || (overview?.current_task?.is_confident && overview.current_task.title !== 'Not clear yet' ? overview.current_task.title : null);
  const isTaskAvailable = Boolean(rawTask && rawTask.trim() !== '' && rawTask.toLowerCase() !== 'not clear yet');
  const taskTitle = rawTask || contextName;

  // Task provenance
  const taskProvenance = (workset?.provenance === 'confirmed' || memory?.recent_task?.provenance === 'confirmed') ? 'Confirmed' : 'Inferred';

  // Context Return indicator
  const isContextReturn = Boolean(
    workset?.workset_state === 'detour_return' ||
    memory?.is_context_return ||
    memory?.evidence?.some((e) => e.toLowerCase().includes('returned to'))
  );

  // Last active timestamp
  const lastActiveTimestamp = workset?.last_active_at || memory?.last_active_at || overview?.recent_activity;
  const formattedLastActive = lastActiveTimestamp ? formatLastActive(lastActiveTimestamp) : 'Recently active';

  const handleFeedback = async (isPositive: boolean) => {
    if (!contextId || feedbackStatus !== 'idle') return;
    setFeedbackStatus('submitting');
    try {
      await db.submitMemoryFeedback(
        contextId,
        rawTask,
        memory?.recent_task?.id || workset?.task_id || undefined,
        isPositive
      );
      setFeedbackStatus('submitted');
      if (onMemoryFeedbackRecorded) {
        onMemoryFeedbackRecorded();
      }
    } catch (err) {
      console.error('Failed to submit memory feedback:', err);
      setFeedbackStatus('idle');
    }
  };

  // Workset continuation items
  const worksetItems: WorksetItemReference[] = useMemo(() => {
    if (workset?.items && workset.items.length > 0) {
      return workset.items;
    }
    return [];
  }, [workset]);

  // 3-5 Recent items fallback
  const recentItems: TimelineItemSummary[] = useMemo(() => {
    if (overview?.recent_items && overview.recent_items.length > 0) {
      return overview.recent_items.slice(0, 5);
    }
    if (memory?.recent_items && memory.recent_items.length > 0) {
      return memory.recent_items.slice(0, 5).map((m) => ({
        id: m.id,
        item_type: m.item_type,
        title: m.title || null,
        content_snippet: m.title || 'Captured item',
        captured_at: m.timestamp,
        membership_type: m.provenance === 'confirmed' ? 'confirmed' : 'suggested',
        app_name: m.app_name || null,
        domain: m.domain || null,
      }));
    }
    return [];
  }, [overview, memory]);

  const recentItemsCount = recentItems.length;

  // Extract recent activity bullet items
  const recentActivityBullets: string[] = useMemo(() => {
    if (overview?.current_task?.recent_activity && overview.current_task.recent_activity.length > 0) {
      return overview.current_task.recent_activity;
    }
    if (workset?.recent_activity && workset.recent_activity.trim() !== '' && !workset.recent_activity.includes('No recent activity')) {
      return [workset.recent_activity];
    }
    if (memory?.recent_items && memory.recent_items.length > 0) {
      return memory.recent_items.slice(0, 3).map((item) => {
        if (item.title) {
          return item.title;
        }
        return `Saved ${item.item_type} in ${contextName}`;
      });
    }
    return [];
  }, [overview, memory, workset, contextName]);

  // Memory explanation structures (without raw scores or technical internals)
  const explanation = memory?.explanation || workset?.explanation;
  const whyReasons: string[] = useMemo(() => {
    if (explanation?.why_reasons && explanation.why_reasons.length > 0) {
      return explanation.why_reasons;
    }
    const reasons: string[] = [];
    if (worksetItems.length > 0) {
      reasons.push(`${worksetItems.length} active workset item${worksetItems.length === 1 ? '' : 's'}`);
    } else if (recentItemsCount > 0) {
      reasons.push(`${recentItemsCount} related recent item${recentItemsCount === 1 ? '' : 's'}`);
    }
    if (recentItemsCount > 1 || worksetItems.length > 1) {
      reasons.push('semantic similarity between active items');
    }
    if (contextName) {
      reasons.push(`same ${contextName} context`);
    }
    if (isContextReturn) {
      reasons.push(`returned to ${contextName} context`);
    }
    if (isTaskAvailable && taskProvenance === 'Inferred') {
      reasons.push('task candidate remained active across sessions');
    }
    return reasons;
  }, [explanation, worksetItems.length, recentItemsCount, contextName, isContextReturn, isTaskAvailable, taskProvenance]);

  const explanationSummary = explanation?.summary || overview?.current_task?.explanation || memory?.memory_statement || (
    isTaskAvailable
      ? `Reconstructed from recent activity patterns and related resources in ${contextName}.`
      : `Based on activity signals recorded in ${contextName}.`
  );

  const uncertaintyNote = explanation?.uncertainty_note;
  const confidenceBand = explanation?.confidence_band || (isTaskAvailable ? (taskProvenance === 'Confirmed' ? 'Confirmed' : 'Inferred') : 'Contextual signal');

  return (
    <div className="space-y-6 animate-in fade-in duration-150">
      {/* =================================================================== */}
      {/* 1. RESUME WHERE I LEFT OFF CARD                                    */}
      {/* =================================================================== */}
      <div className="p-5 rounded-xl bg-white border border-gray-200/90 shadow-2xs space-y-4">
        {/* Muted header */}
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
            You were working on
          </span>
          {isContextReturn && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-indigo-50 text-indigo-700 border border-indigo-200/70">
              <RotateCcw size={10} />
              Returned to context
            </span>
          )}
        </div>

        {/* Focus & Subtitle & Direct [Why?] Trigger */}
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-2 flex-wrap">
              <h3 className="text-lg font-semibold text-gray-900 tracking-tight leading-snug">
                {taskTitle}
              </h3>
              <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-gray-100 text-gray-500 border border-gray-200/60">
                {isTaskAvailable ? taskProvenance : 'Confirmed'}
              </span>
            </div>

            {/* Direct [Why?] trigger button */}
            <button
              onClick={() => setShowWhy(!showWhy)}
              className={`inline-flex items-center gap-1 text-xs font-medium px-2.5 py-1 rounded-md transition-colors cursor-pointer border ${
                showWhy
                  ? 'bg-indigo-50 text-indigo-700 border-indigo-200'
                  : 'bg-gray-50 text-gray-600 hover:text-gray-900 hover:bg-gray-100 border-gray-200/80'
              }`}
              title="Explain how this memory was inferred"
            >
              <HelpCircle size={12} className={showWhy ? 'text-indigo-600' : 'text-gray-400'} />
              <span>Why?</span>
              {showWhy ? <ChevronUp size={11} /> : <ChevronDown size={11} />}
            </button>
          </div>

          <div className="text-xs text-gray-500 flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="font-medium text-gray-700">{contextName}</span>
              <span>•</span>
              <span>{formattedLastActive}</span>
            </div>

            {/* Subtle Memory Feedback Action */}
            {isTaskAvailable && (
              feedbackStatus === 'idle' ? (
                <div className="flex items-center gap-1.5">
                  <button
                    onClick={() => handleFeedback(true)}
                    className="inline-flex items-center gap-1 text-[11px] font-medium text-gray-500 hover:text-emerald-700 hover:bg-emerald-50 px-2 py-0.5 rounded border border-gray-200/80 hover:border-emerald-300 transition-colors cursor-pointer"
                    title="Confirm this was what you were working on"
                  >
                    <Check size={11} className="text-emerald-600" />
                    <span>Correct</span>
                  </button>
                  <button
                    onClick={() => handleFeedback(false)}
                    className="inline-flex items-center gap-1 text-[11px] font-medium text-gray-500 hover:text-red-700 hover:bg-red-50 px-2 py-0.5 rounded border border-gray-200/80 hover:border-red-300 transition-colors cursor-pointer"
                    title="Not what I was doing"
                  >
                    <X size={11} className="text-gray-400 hover:text-red-500" />
                    <span>Not quite</span>
                  </button>
                </div>
              ) : feedbackStatus === 'submitted' ? (
                <div className="inline-flex items-center gap-1 text-[11px] font-medium text-emerald-700 bg-emerald-50 px-2 py-0.5 rounded border border-emerald-200/80 animate-in fade-in duration-150">
                  <Check size={11} className="text-emerald-600" />
                  <span>Got it</span>
                </div>
              ) : null
            )}
          </div>

          {!isTaskAvailable && (
            <p className="text-xs text-gray-600 pt-0.5">
              Recent activity suggests you were active here.
            </p>
          )}

          {/* Explainable Memory Section (Toggled by [Why?]) */}
          {showWhy && (
            <div className="mt-3 p-3.5 bg-slate-50/95 rounded-lg border border-slate-200/90 text-xs space-y-2.5 animate-in fade-in duration-150">
              <div className="flex items-center justify-between">
                <span className="text-[11px] font-semibold text-slate-700 tracking-wide uppercase flex items-center gap-1.5">
                  <Sparkles size={12} className="text-indigo-500" />
                  Why?
                </span>
                <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-white text-slate-600 border border-slate-200 shadow-2xs">
                  {confidenceBand}
                </span>
              </div>

              {/* Supporting Reasons */}
              {whyReasons.length > 0 && (
                <ul className="space-y-1 text-slate-700 pl-0.5">
                  {whyReasons.map((reason, idx) => (
                    <li key={idx} className="flex items-start gap-2">
                      <span className="text-indigo-500 font-bold">•</span>
                      <span>{reason}</span>
                    </li>
                  ))}
                </ul>
              )}

              {/* Grounded Summary Text */}
              {explanationSummary && (
                <p className="text-[11px] text-slate-600 leading-relaxed pt-1 border-t border-slate-200/70">
                  {explanationSummary}
                </p>
              )}

              {/* Uncertainty Note (Honest epistemic confidence, never overclaiming) */}
              {uncertaintyNote && (
                <div className="flex items-start gap-1.5 pt-1 text-[11px] text-amber-800 bg-amber-50/80 p-2 rounded-md border border-amber-200/60">
                  <Info size={12} className="flex-shrink-0 mt-0.5 text-amber-600" />
                  <span>{uncertaintyNote}</span>
                </div>
              )}
            </div>
          )}
        </div>

        {/* =================================================================== */}
        {/* CONTINUE WITH (Context Workset Continuation List)                  */}
        {/* =================================================================== */}
        <div className="pt-3 border-t border-gray-100 space-y-2.5">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
              Continue with
            </span>
            {worksetItems.length > 0 && (
              <button
                onClick={onOpenWorksetDetail}
                className="inline-flex items-center gap-1 text-[11px] font-medium text-indigo-600 hover:text-indigo-800 transition-colors cursor-pointer"
              >
                <span>Working set detail</span>
                <ArrowRight size={11} />
              </button>
            )}
          </div>

          {worksetItems.length > 0 ? (
            <div className="space-y-1.5">
              {(isViewAllWorkset ? worksetItems : worksetItems.slice(0, 3)).map((item) => (
                <WorksetItemRow
                  key={item.id}
                  item={item}
                  onClick={() => onSelectItem(item.id)}
                />
              ))}

              <div className="flex items-center justify-between pt-1">
                {worksetItems.length > 3 && (
                  <button
                    onClick={() => setIsViewAllWorkset(!isViewAllWorkset)}
                    className="inline-flex items-center gap-1 text-xs font-medium text-gray-600 hover:text-gray-900 transition-colors cursor-pointer"
                  >
                    <span>{isViewAllWorkset ? 'Show less' : `View all (${worksetItems.length})`}</span>
                    {isViewAllWorkset ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
                  </button>
                )}

                <button
                  onClick={onOpenWorksetDetail}
                  className="inline-flex items-center gap-1 text-xs font-medium text-gray-600 hover:text-gray-900 transition-colors cursor-pointer ml-auto"
                >
                  <span>Working set view</span>
                  <ArrowRight size={11} />
                </button>
              </div>
            </div>
          ) : recentItemsCount > 0 ? (
            /* Fallback to recent items if workset not yet derived */
            <div className="space-y-1.5">
              {recentItems.slice(0, 3).map((item) => (
                <WorksetItemRow
                  key={item.id}
                  item={{
                    id: item.id,
                    title: item.title,
                    item_type: item.item_type,
                    relevance: 'recent',
                    relevance_score: 0.5,
                    reasons: ['recently active in context'],
                    provenance: item.membership_type === 'confirmed' ? 'confirmed' : 'inferred',
                    created_at: item.captured_at || '',
                    app_name: item.app_name,
                    domain: item.domain,
                    content_snippet: item.content_snippet || '',
                    is_suggested_addition: false,
                  }}
                  onClick={() => onSelectItem(item.id)}
                />
              ))}
            </div>
          ) : (
            <p className="text-xs text-gray-400 italic">No working items recorded yet.</p>
          )}
        </div>

        {/* Action Button: View recent work details toggle */}
        <div className="pt-1">
          <button
            onClick={() => setShowRecentWork(!showRecentWork)}
            className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-gray-900 hover:bg-gray-800 text-white text-xs font-medium transition-colors cursor-pointer"
          >
            <span>{showRecentWork ? 'Hide recent work details' : 'View recent work details'}</span>
            {showRecentWork ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
          </button>
        </div>

        {/* Revealed Recent Work Details */}
        {showRecentWork && (
          <div className="pt-4 border-t border-gray-100 space-y-4 animate-in fade-in duration-150">
            {/* Recent activity bullet points */}
            {recentActivityBullets.length > 0 && (
              <div className="space-y-1.5">
                <div className="text-xs font-semibold text-gray-700">
                  Recent activity:
                </div>
                <ul className="space-y-1 text-xs text-gray-600 pl-1">
                  {recentActivityBullets.map((bullet, idx) => (
                    <li key={idx} className="flex items-start gap-2">
                      <span className="text-gray-400 font-bold">•</span>
                      <span className="truncate">{bullet}</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* Recent items preview (3-5 items) */}
            <div className="space-y-2">
              <div className="text-xs font-semibold text-gray-700 flex items-center gap-1.5">
                <span>Recent items ({recentItemsCount})</span>
              </div>
              {recentItemsCount > 0 ? (
                <div className="space-y-1.5">
                  {recentItems.map((item) => (
                    <RecentItemRow
                      key={item.id}
                      item={item}
                      onClick={() => onSelectItem(item.id)}
                    />
                  ))}
                </div>
              ) : (
                <p className="text-xs text-gray-400 italic">No recent items captured.</p>
              )}
            </div>

            {/* Related applications & domains in quiet tags */}
            {((overview?.related_applications && overview.related_applications.length > 0) ||
              (overview?.important_domains && overview.important_domains.length > 0)) && (
              <div className="pt-2 border-t border-gray-100 flex flex-wrap gap-1.5">
                {overview?.related_applications?.map((app) => (
                  <span
                    key={app.name}
                    className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] bg-gray-50 border border-gray-200 text-gray-600"
                  >
                    <Laptop size={10} className="text-gray-400" />
                    <span>{app.name}</span>
                    <span className="text-gray-400">({app.item_count})</span>
                  </span>
                ))}
                {overview?.important_domains?.map((dom) => (
                  <span
                    key={dom.domain}
                    className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] bg-gray-50 border border-gray-200 text-gray-600"
                  >
                    <Globe size={10} className="text-gray-400" />
                    <span>{dom.domain}</span>
                    <span className="text-gray-400">({dom.item_count})</span>
                  </span>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {/* =================================================================== */}
      {/* 2. CONTEXT ITEMS OVERVIEW (When top is collapsed & no workset)     */}
      {/* =================================================================== */}
      {!showRecentWork && worksetItems.length === 0 && recentItemsCount > 0 && (
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="text-xs font-semibold text-gray-900 flex items-center gap-1.5">
              <FileText size={14} className="text-gray-600" />
              <span>Recent items</span>
            </div>
            <span className="text-[11px] text-gray-400">
              {recentItemsCount} item{recentItemsCount === 1 ? '' : 's'}
            </span>
          </div>

          <div className="space-y-2">
            {recentItems.map((item) => (
              <RecentItemRow
                key={item.id}
                item={item}
                onClick={() => onSelectItem(item.id)}
              />
            ))}
          </div>
        </div>
      )}

      {/* =================================================================== */}
      {/* 3. CONTEXT ACTIONS & TIMELINE LINK                                 */}
      {/* =================================================================== */}
      <div className="pt-2 border-t border-gray-100 flex items-center justify-between gap-3">
        <button
          onClick={onToggleArchive}
          disabled={isUpdatingLifecycle || !overview}
          className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border border-gray-200 hover:bg-gray-50 text-gray-600 transition-colors cursor-pointer disabled:opacity-50"
        >
          {overview?.lifecycle_state === 'archived' ? (
            <>
              <ArchiveRestore size={13} />
              <span>Restore Context</span>
            </>
          ) : (
            <>
              <Archive size={13} />
              <span>Archive Context</span>
            </>
          )}
        </button>

        <button
          onClick={onViewFullTimeline}
          className="flex items-center gap-1.5 text-xs font-medium text-gray-600 hover:text-gray-900 transition-colors cursor-pointer"
        >
          <span>View Activity Timeline</span>
          <ExternalLink size={12} />
        </button>
      </div>
    </div>
  );
}

function WorksetItemRow({
  item,
  onClick,
}: {
  item: WorksetItemReference;
  onClick: () => void;
}) {
  const getItemIcon = (type: string, appName?: string | null) => {
    const lowerApp = (appName || '').toLowerCase();
    if (lowerApp.includes('figma')) return <Layers size={13} className="text-purple-500" />;
    if (lowerApp.includes('github') || lowerApp.includes('code') || lowerApp.includes('terminal')) {
      return <CodeIcon size={13} className="text-gray-700" />;
    }
    switch (type) {
      case 'url':
        return <Globe size={13} className="text-blue-500" />;
      case 'code':
        return <CodeIcon size={13} className="text-emerald-600" />;
      case 'image':
        return <ImageIcon size={13} className="text-pink-500" />;
      default:
        return <FileText size={13} className="text-gray-500" />;
    }
  };

  const displayTitle = item.title || item.content_snippet || 'Untitled Item';
  const primaryReason = item.reasons && item.reasons.length > 0 ? item.reasons[0] : null;

  return (
    <div
      onClick={onClick}
      className="group p-2.5 rounded-lg border border-gray-200/70 bg-gray-50/40 hover:bg-white hover:border-gray-300 hover:shadow-2xs transition-all flex items-center justify-between gap-3 cursor-pointer"
      title={primaryReason ? `${displayTitle} — ${primaryReason}` : displayTitle}
    >
      <div className="flex items-center gap-2.5 min-w-0">
        <div className="w-6 h-6 rounded bg-white border border-gray-200 group-hover:border-gray-300 flex items-center justify-center flex-shrink-0 shadow-2xs">
          {getItemIcon(item.item_type, item.app_name)}
        </div>
        <div className="min-w-0">
          <div className="text-xs font-medium text-gray-800 group-hover:text-gray-900 truncate">
            {displayTitle}
          </div>
          <div className="flex items-center gap-2 text-[10px] text-gray-400 mt-0.5">
            {item.app_name && <span>{item.app_name}</span>}
            {item.domain && <span>{item.domain}</span>}
            {item.last_active_at ? (
              <span>{formatRelativeDate(item.last_active_at)}</span>
            ) : item.created_at ? (
              <span>{formatRelativeDate(item.created_at)}</span>
            ) : null}
          </div>
        </div>
      </div>

      <div className="flex items-center gap-1.5 flex-shrink-0">
        {item.relevance === 'primary' && (
          <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-blue-50 text-blue-700 border border-blue-200/60">
            Primary
          </span>
        )}
        {item.relevance === 'supporting' && (
          <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-gray-100 text-gray-600 border border-gray-200/60">
            Supporting
          </span>
        )}
        {item.relevance === 'reference' && (
          <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-amber-50 text-amber-700 border border-amber-200/60">
            Reference
          </span>
        )}
        <ExternalLink size={12} className="text-gray-300 group-hover:text-gray-600 transition-colors ml-0.5" />
      </div>
    </div>
  );
}

function RecentItemRow({
  item,
  onClick,
}: {
  item: TimelineItemSummary;
  onClick?: () => void;
}) {
  const getItemIcon = (type: string) => {
    switch (type) {
      case 'url':
        return <Globe size={13} className="text-gray-500" />;
      case 'code':
        return <CodeIcon size={13} className="text-gray-500" />;
      case 'image':
        return <ImageIcon size={13} className="text-gray-500" />;
      default:
        return <FileText size={13} className="text-gray-500" />;
    }
  };

  const displayTitle = item.title || item.content_snippet || 'Untitled Item';

  return (
    <div
      onClick={onClick}
      className={`p-2.5 rounded-lg border border-gray-100 bg-white hover:border-gray-200 transition-colors flex items-center justify-between gap-3 ${onClick ? 'cursor-pointer' : ''}`}
    >
      <div className="flex items-center gap-2.5 min-w-0">
        <div className="w-6 h-6 rounded bg-gray-50 flex items-center justify-center flex-shrink-0">
          {getItemIcon(item.item_type)}
        </div>
        <div className="min-w-0">
          <div className="text-xs font-medium text-gray-800 truncate">
            {displayTitle}
          </div>
          <div className="flex items-center gap-2 text-[10px] text-gray-400 mt-0.5">
            {item.app_name && <span>{item.app_name}</span>}
            {item.domain && <span>{item.domain}</span>}
            {item.captured_at && <span>{formatRelativeDate(item.captured_at)}</span>}
          </div>
        </div>
      </div>

      <div className="flex items-center gap-1.5 flex-shrink-0">
        {item.membership_type === 'suggested' ? (
          <span className="inline-flex items-center gap-1 text-[10px] font-medium px-2 py-0.5 rounded-full bg-amber-50 text-amber-700 border border-amber-200/60">
            <Inbox size={9} />
            Suggested
          </span>
        ) : (
          <span className="inline-flex items-center gap-1 text-[10px] font-medium px-2 py-0.5 rounded-full bg-gray-100 text-gray-600 border border-gray-200/60">
            <Folder size={9} />
            Confirmed
          </span>
        )}
      </div>
    </div>
  );
}

function ContextItemDetailModal({
  itemId,
  onClose,
}: {
  itemId: string;
  onClose: () => void;
}) {
  const [item, setItem] = useState<Item | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let isMounted = true;
    setIsLoading(true);
    db.getItem(itemId)
      .then((data) => {
        if (isMounted) {
          setItem(data);
          setIsLoading(false);
        }
      })
      .catch((err) => {
        console.error('Failed to load item details:', err);
        if (isMounted) setIsLoading(false);
      });
    return () => {
      isMounted = false;
    };
  }, [itemId]);

  const handleCopy = () => {
    if (!item) return;
    const textToCopy = item.content || item.title || '';
    navigator.clipboard.writeText(textToCopy);
    setCopied(true);
    toast.success('Copied to clipboard');
    setTimeout(() => setCopied(false), 2000);
  };

  const parseMetadata = (rawMeta?: string | null): Record<string, any> => {
    if (!rawMeta) return {};
    try {
      return JSON.parse(rawMeta);
    } catch {
      return {};
    }
  };

  const meta = parseMetadata(item?.metadata);
  const displayTitle = item?.title || meta.title || meta.window_title || 'Untitled Item';

  return createPortal(
    <div className="fixed inset-0 z-60 flex items-center justify-center bg-black/40 backdrop-blur-xs p-4 animate-in fade-in duration-150">
      <div className="fixed inset-0" onClick={onClose} />
      <div className="relative w-full max-w-lg bg-white rounded-2xl shadow-2xl border border-gray-200 overflow-hidden flex flex-col max-h-[85vh] animate-in zoom-in-95 duration-150 z-10">
        {/* Modal Header */}
        <div className="px-5 py-4 border-b border-gray-100 flex items-center justify-between bg-gray-50/50">
          <div className="flex items-center gap-2.5 min-w-0">
            <span className="px-2 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wider bg-gray-200 text-gray-700">
              {item?.item_type || 'item'}
            </span>
            <h3 className="text-sm font-semibold text-gray-900 truncate">
              {isLoading ? 'Loading item...' : displayTitle}
            </h3>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors cursor-pointer"
          >
            <X size={16} />
          </button>
        </div>

        {/* Modal Content */}
        <div className="p-5 overflow-y-auto space-y-4 flex-1">
          {isLoading ? (
            <div className="h-40 flex items-center justify-center text-gray-400">
              <RefreshCw size={18} className="animate-spin text-gray-500 mr-2" />
              <span className="text-xs">Loading item details...</span>
            </div>
          ) : item ? (
            <>
              {/* Metadata tags */}
              <div className="flex flex-wrap items-center gap-2 text-xs text-gray-500">
                {meta.app_name && (
                  <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-gray-100 text-gray-700">
                    <Laptop size={11} />
                    {meta.app_name}
                  </span>
                )}
                {meta.domain && (
                  <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-gray-100 text-gray-700">
                    <Globe size={11} />
                    {meta.domain}
                  </span>
                )}
                {item.created_at && (
                  <span className="inline-flex items-center gap-1 text-[11px] text-gray-400">
                    <Calendar size={11} />
                    {formatRelativeDate(item.created_at)}
                  </span>
                )}
              </div>

              {/* Item Content Preview */}
              <div className="space-y-1.5">
                <div className="text-xs font-semibold text-gray-700">Content</div>
                {item.item_type === 'url' ? (
                  <div className="p-3 bg-blue-50/50 rounded-lg border border-blue-100 text-xs text-blue-900 break-all font-mono">
                    <a
                      href={item.content}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="hover:underline flex items-center gap-1.5 text-blue-700"
                    >
                      <Globe size={13} />
                      <span>{item.content}</span>
                      <ExternalLink size={11} />
                    </a>
                  </div>
                ) : item.item_type === 'code' ? (
                  <pre className="p-3 bg-gray-900 text-gray-100 rounded-lg text-xs font-mono overflow-x-auto max-h-60">
                    <code>{item.content}</code>
                  </pre>
                ) : (
                  <div className="p-3.5 bg-gray-50 rounded-lg border border-gray-100 text-xs text-gray-800 whitespace-pre-wrap leading-relaxed max-h-60 overflow-y-auto">
                    {item.content || 'No text content recorded.'}
                  </div>
                )}
              </div>
            </>
          ) : (
            <div className="p-4 text-center text-xs text-gray-400">
              Item details could not be found.
            </div>
          )}
        </div>

        {/* Modal Footer */}
        <div className="px-5 py-3 border-t border-gray-100 bg-gray-50/50 flex items-center justify-between gap-3">
          <button
            onClick={handleCopy}
            disabled={!item || !item.content}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border border-gray-200 hover:bg-white text-gray-700 transition-colors cursor-pointer disabled:opacity-50"
          >
            {copied ? <Check size={13} className="text-emerald-600" /> : <Copy size={13} />}
            <span>{copied ? 'Copied' : 'Copy Content'}</span>
          </button>

          <button
            onClick={onClose}
            className="px-3 py-1.5 text-xs font-medium rounded-lg bg-gray-900 hover:bg-gray-800 text-white transition-colors cursor-pointer"
          >
            Close
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}

function TimelineTabContent({
  events,
  searchQuery,
  onSearchChange,
}: {
  events: TimelineEvent[];
  searchQuery: string;
  onSearchChange: (q: string) => void;
}) {
  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <div className="relative">
        <Search size={14} className="absolute left-3 top-2.5 text-gray-400" />
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder="Filter chronological events..."
          className="w-full pl-9 pr-3 py-1.5 text-xs bg-gray-50 rounded-lg border border-gray-200 focus:outline-none focus:ring-1 focus:ring-gray-400"
        />
      </div>

      {events.length > 0 ? (
        <div className="space-y-3 relative before:absolute before:top-2 before:bottom-2 before:left-[17px] before:w-[2px] before:bg-gray-100">
          {events.map((ev) => (
            <div key={ev.id} className="flex items-start gap-3 relative">
              <div className="w-8 h-8 rounded-full bg-white border border-gray-200 flex items-center justify-center flex-shrink-0 text-gray-600 shadow-2xs z-10">
                <Clock size={13} />
              </div>
              <div className="flex-1 p-3 bg-gray-50/70 rounded-lg border border-gray-100">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-semibold text-gray-800">{ev.title}</span>
                  <span className="text-[10px] text-gray-400">{formatRelativeDate(ev.timestamp)}</span>
                </div>
                <p className="text-xs text-gray-600 mt-1">{ev.description}</p>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="p-8 text-center text-xs text-gray-400">
          No matching timeline events found.
        </div>
      )}
    </div>
  );
}

function formatLastActive(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHours = Math.floor(diffMin / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffSec < 60) return 'Active just now';
    if (diffMin === 1) return 'Last active 1 minute ago';
    if (diffMin < 60) return `Last active ${diffMin}m ago`;
    if (diffHours === 1) return 'Last active 1h ago';
    if (diffHours < 24) return `Last active ${diffHours}h ago`;
    if (diffDays === 1) return 'Last active yesterday';
    if (diffDays < 7) return `Last active ${diffDays}d ago`;
    return `Last active on ${d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}`;
  } catch {
    return `Last active ${dateStr}`;
  }
}

function formatRelativeDate(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHours = Math.floor(diffMin / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffSec < 60) return 'Just now';
    if (diffMin === 1) return '1m ago';
    if (diffMin < 60) return `${diffMin}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays === 1) return 'Yesterday';
    if (diffDays < 7) return `${diffDays}d ago`;
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  } catch {
    return dateStr;
  }
}
