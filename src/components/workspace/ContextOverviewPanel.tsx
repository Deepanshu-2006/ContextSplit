import { useState, useEffect, useMemo } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  Clock,
  Layers,
  Globe,
  Search,
  RefreshCw,
  Archive,
  ArchiveRestore,
  ExternalLink,
  Laptop,
  AlertCircle,
  ArrowRight,
  Calendar,
  Check,
  Copy,
  ChevronDown,
  ChevronUp,
  Info,
} from 'lucide-react';
import { db, ContextOverview, TimelineEvent, Item } from '../../services/db';
import { contextMemoryService, ContextMemoryResponse } from '../../services/contextMemory';
import { contextWorksetService, ContextWorkset } from '../../services/contextWorkset';
import { contextStateService, ContextState, ContextStateValue } from '../../services/contextState';
import { contextSnapshotService, ContextSnapshotDiff } from '../../services/contextSnapshot';
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
  const [contextState, setContextState] = useState<ContextState | null>(null);
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
      setContextState(null);
      setSelectedItemId(null);
    }
  }, [isOverviewOpen, overviewWorkspaceId]);

  const loadOverviewAndMemory = async (wsId: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const [overviewData, memoryData, worksetData, stateData] = await Promise.all([
        db.getContextOverview(wsId).catch(() => null),
        contextMemoryService.getContextMemory(wsId).catch(() => null),
        contextWorksetService.getContextWorkset(wsId).catch(() => null),
        contextStateService.getContextState(wsId).catch(() => null),
      ]);
      setOverview(overviewData);
      setMemory(memoryData);
      setWorkset(worksetData);
      setContextState(stateData || overviewData?.context_state || null);
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
            </div>
          </div>

          <div className="flex items-center gap-1.5">
            <button
              onClick={() => overviewWorkspaceId && loadOverviewAndMemory(overviewWorkspaceId)}
              disabled={isLoading}
              className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-md transition-colors cursor-pointer"
              title="Refresh Overview"
            >
              <RefreshCw size={14} className={isLoading ? 'animate-spin text-gray-700' : ''} />
            </button>
            <button
              onClick={closeOverview}
              className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-md transition-colors cursor-pointer"
              title="Close panel (Esc)"
            >
              <X size={16} />
            </button>
          </div>
        </div>

        {/* Tab switcher */}
        <div className="flex border-b border-gray-100 px-6 bg-white text-xs">
          <button
            onClick={() => setActiveTab('overview')}
            className={`py-2.5 px-3 font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
              activeTab === 'overview'
                ? 'border-gray-900 text-gray-900'
                : 'border-transparent text-gray-400 hover:text-gray-700'
            }`}
          >
            <span>Overview</span>
          </button>
          <button
            onClick={() => setActiveTab('workset')}
            className={`py-2.5 px-3 font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
              activeTab === 'workset'
                ? 'border-gray-900 text-gray-900'
                : 'border-transparent text-gray-400 hover:text-gray-700'
            }`}
          >
            <span>Working Set</span>
          </button>
          <button
            onClick={() => setActiveTab('timeline')}
            className={`py-2.5 px-3 font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
              activeTab === 'timeline'
                ? 'border-gray-900 text-gray-900'
                : 'border-transparent text-gray-400 hover:text-gray-700'
            }`}
          >
            <span>Timeline</span>
          </button>
        </div>

        {/* Content Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {isLoading && !overview && !memory && !contextState ? (
            <div className="h-64 flex flex-col items-center justify-center text-gray-400 gap-2">
              <RefreshCw size={18} className="animate-spin text-gray-400" />
              <p className="text-xs">Loading context...</p>
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
                contextState={contextState}
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
                  contextState?.task_title ||
                  memory?.task ||
                  workset?.task_title ||
                  (overview?.current_task?.is_confident && overview.current_task.title !== 'Not clear yet'
                    ? overview.current_task.title
                    : null)
                }
                onBack={() => setActiveTab('overview')}
                onSelectItem={(itemId) => setSelectedItemId(itemId)}
                onWorksetUpdated={() => {
                  if (overviewWorkspaceId) {
                    loadOverviewAndMemory(overviewWorkspaceId);
                  }
                }}
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
  contextState: ContextState | null;
  targetWorkspaceName?: string;
  onToggleArchive: () => void;
  isUpdatingLifecycle: boolean;
  onSelectItem: (itemId: string) => void;
  onOpenWorksetDetail: () => void;
  onViewFullTimeline: () => void;
  onMemoryFeedbackRecorded?: () => void;
}

function formatStateLabel(state?: ContextStateValue | string | null): string {
  switch (state) {
    case 'active':
      return 'Active';
    case 'recently_active':
      return 'Recently active';
    case 'paused':
      return 'Paused';
    case 'dormant':
      return 'Dormant';
    case 'uncertain':
      return 'Uncertain';
    case 'insufficient_evidence':
      return 'Insufficient evidence';
    default:
      return 'Active';
  }
}

function ResumeMemoryOverviewContent({
  overview,
  memory,
  workset,
  contextState,
  targetWorkspaceName,
  onToggleArchive,
  isUpdatingLifecycle,
  onSelectItem,
  onOpenWorksetDetail,
  onViewFullTimeline,
}: ResumeMemoryOverviewContentProps) {
  const contextName =
    contextState?.context_name ||
    overview?.name ||
    memory?.context ||
    workset?.context_name ||
    targetWorkspaceName ||
    'Workspace';

  // 1. Context Working State
  const stateValue: ContextStateValue =
    contextState?.state ||
    (overview?.lifecycle_state === 'archived' ? 'dormant' : overview?.lifecycle_state === 'dormant' ? 'dormant' : 'active');
  const stateLabel = formatStateLabel(stateValue);

  // 2. YOU WERE WORKING ON
  const rawTask =
    contextState?.task_title ||
    memory?.task ||
    workset?.task_title ||
    (overview?.current_task?.is_confident && overview.current_task.title !== 'Not clear yet'
      ? overview.current_task.title
      : null);
  const isTaskAvailable = Boolean(rawTask && rawTask.trim() !== '' && rawTask.toLowerCase() !== 'not clear yet');
  const taskTitle = isTaskAvailable && rawTask ? rawTask : `Recent work in ${contextName}`;

  // Last active timestamp
  const lastActiveTimestamp =
    contextState?.last_active_at ||
    workset?.last_active_at ||
    memory?.last_active_at ||
    overview?.recent_activity;
  const formattedLastActive =
    (contextState?.freshness_display && contextState.freshness_display !== 'No prior activity'
      ? contextState.freshness_display
      : null) ||
    (lastActiveTimestamp ? formatSessionTime(lastActiveTimestamp) : 'Active recently');

  // Progressive Disclosure: "What changed?" and "Why?"
  const [showWhy, setShowWhy] = useState(false);
  const [showWhatChanged, setShowWhatChanged] = useState(false);
  const [snapshotDiff, setSnapshotDiff] = useState<ContextSnapshotDiff | null>(null);
  const [isLoadingDiff, setIsLoadingDiff] = useState(false);

  const contextId =
    contextState?.context_id ||
    overview?.workspace_id ||
    memory?.context_id ||
    workset?.context_id;

  const handleToggleWhatChanged = async () => {
    const nextState = !showWhatChanged;
    setShowWhatChanged(nextState);
    if (nextState && !snapshotDiff && contextId) {
      setIsLoadingDiff(true);
      try {
        const diff = await contextSnapshotService.getSnapshotDiff(contextId);
        setSnapshotDiff(diff);
      } catch (err) {
        console.error('Failed to load snapshot diff:', err);
      } finally {
        setIsLoadingDiff(false);
      }
    }
  };

  // 3. WHERE YOU LEFT OFF
  const isStateUncertain = stateValue === 'uncertain' || stateValue === 'insufficient_evidence';
  const primaryWorksetItem = workset?.items.find((i) => i.relevance === 'primary');
  
  let whereLeftOff: string;
  if (isStateUncertain) {
    whereLeftOff = `Recent activity in ${contextName}`;
  } else if (primaryWorksetItem?.title || primaryWorksetItem?.content_snippet) {
    whereLeftOff = primaryWorksetItem.title || primaryWorksetItem.content_snippet;
  } else if (
    contextState?.last_meaningful_activity &&
    contextState.last_meaningful_activity.trim().length > 0 &&
    !contextState.last_meaningful_activity.toLowerCase().includes('unknown') &&
    contextState.last_meaningful_activity !== `Recent activity in ${contextName}`
  ) {
    whereLeftOff = contextState.last_meaningful_activity;
  } else if (contextState?.resume_state?.last_meaningful_activity) {
    whereLeftOff = contextState.resume_state.last_meaningful_activity;
  } else if (contextState?.resume_state?.left_off_on) {
    whereLeftOff = contextState.resume_state.left_off_on;
  } else {
    whereLeftOff = `Recent activity in ${contextName}`;
  }

  // 4. CONTINUE WITH items
  const continueWithItems: { id: string; title: string; app_name?: string | null; domain?: string | null }[] = useMemo(() => {
    if (workset?.items && workset.items.length > 0) {
      return workset.items.map((item) => ({
        id: item.id,
        title: item.title || item.content_snippet || 'Untitled item',
        app_name: item.app_name,
        domain: item.domain,
      }));
    }
    if (contextState?.resume_state?.recent_work && contextState.resume_state.recent_work.length > 0) {
      return contextState.resume_state.recent_work.map((title, idx) => ({
        id: contextState.recent_item_ids[idx] || `recent-${idx}`,
        title,
      }));
    }
    if (overview?.recent_items && overview.recent_items.length > 0) {
      return overview.recent_items.map((item) => ({
        id: item.id,
        title: item.title || item.content_snippet || 'Untitled item',
        app_name: item.app_name,
        domain: item.domain,
      }));
    }
    return [];
  }, [workset, contextState, overview]);

  // Clean explanation reasons without badge clutter
  const explanationReasons = useMemo(() => {
    if (contextState?.explanation?.reasons && contextState.explanation.reasons.length > 0) {
      return contextState.explanation.reasons.map(r => r.text);
    }
    const list: string[] = [];
    if (taskTitle && taskTitle !== `Recent work in ${contextName}`) {
      list.push(`${taskTitle} was the most recent active task`);
    }
    if (continueWithItems.length > 0) {
      list.push(`${continueWithItems.length} relevant item${continueWithItems.length === 1 ? ' was' : 's were'} recently active`);
    }
    list.push(`${contextName} remained the active context`);
    return list;
  }, [contextState, taskTitle, continueWithItems.length, contextName]);

  const uncertaintyNote =
    contextState?.explanation?.uncertainty_note ||
    (isStateUncertain
      ? 'Context signals are emerging or sparse. State will update as more activity occurs.'
      : null);

  return (
    <div className="space-y-7 animate-in fade-in duration-150">
      {/* 1. Context & Subtle Status */}
      <div className="flex items-center justify-between pb-3 border-b border-gray-100">
        <div className="flex items-center gap-2 text-sm font-semibold text-gray-900">
          <span>{contextName}</span>
          <span className="text-gray-300 font-normal">·</span>
          <span className="text-xs font-normal text-gray-500">
            {stateLabel}
          </span>
        </div>
      </div>

      {/* 2. YOU WERE WORKING ON */}
      <div className="space-y-1.5">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
            You were working on
          </span>
          <button
            onClick={handleToggleWhatChanged}
            className={`inline-flex items-center gap-1 text-[11px] font-medium px-2 py-0.5 rounded transition-colors cursor-pointer ${
              showWhatChanged
                ? 'bg-gray-100 text-gray-900'
                : 'text-gray-400 hover:text-gray-700 hover:bg-gray-50'
            }`}
            title="What changed since your last session?"
          >
            <span>What changed?</span>
            {showWhatChanged ? <ChevronUp size={10} /> : <ChevronDown size={10} />}
          </button>
        </div>

        <div className="text-base font-medium text-gray-900 tracking-tight">
          {taskTitle}
        </div>
        <div className="text-xs text-gray-400 font-normal">
          Last active: {formattedLastActive}
        </div>

        {/* Progressive Disclosure: "What changed?" */}
        {showWhatChanged && (
          <div className="mt-3 p-3.5 bg-gray-50/80 rounded-lg border border-gray-100 text-xs space-y-2 animate-in fade-in duration-150">
            <div className="text-[11px] font-medium text-gray-700">
              Since your last session:
            </div>
            {isLoadingDiff ? (
              <div className="flex items-center gap-2 text-gray-400 text-xs py-1">
                <RefreshCw size={12} className="animate-spin text-gray-500" />
                <span>Comparing snapshots...</span>
              </div>
            ) : snapshotDiff && snapshotDiff.has_meaningful_changes ? (
              <ul className="space-y-1.5 pl-0.5 text-xs text-gray-700">
                {snapshotDiff.task_changed && snapshotDiff.previous_task && snapshotDiff.current_task && (
                  <li className="flex items-start gap-1.5">
                    <span className="text-gray-400 select-none">•</span>
                    <span>
                      Task changed from <strong className="font-medium text-gray-900">{snapshotDiff.previous_task}</strong> to <strong className="font-medium text-gray-900">{snapshotDiff.current_task}</strong>
                    </span>
                  </li>
                )}
                {snapshotDiff.task_changed && !snapshotDiff.previous_task && snapshotDiff.current_task && (
                  <li className="flex items-start gap-1.5">
                    <span className="text-gray-400 select-none">•</span>
                    <span>
                      Started task <strong className="font-medium text-gray-900">{snapshotDiff.current_task}</strong>
                    </span>
                  </li>
                )}
                {snapshotDiff.added_items.map((item) => (
                  <li key={item.id} className="flex items-start gap-1.5">
                    <span className="text-gray-400 select-none">•</span>
                    <span>
                      <strong className="font-medium text-gray-900">{item.title || item.content_snippet || 'New item'}</strong> became part of the workset
                    </span>
                  </li>
                ))}
                {snapshotDiff.removed_items.map((item) => (
                  <li key={item.id} className="flex items-start gap-1.5 text-gray-500">
                    <span className="text-gray-400 select-none">•</span>
                    <span>
                      <strong className="font-medium text-gray-700">{item.title || item.content_snippet || 'Previous item'}</strong> was removed from active workset
                    </span>
                  </li>
                ))}
                {snapshotDiff.state_changed && snapshotDiff.previous_state && (
                  <li className="flex items-start gap-1.5">
                    <span className="text-gray-400 select-none">•</span>
                    <span>
                      State transitioned from <span className="font-medium">{formatStateLabel(snapshotDiff.previous_state)}</span> to <span className="font-medium">{formatStateLabel(snapshotDiff.current_state)}</span>
                    </span>
                  </li>
                )}
              </ul>
            ) : (
              <p className="text-xs text-gray-500 italic">
                Nothing significant changed.
              </p>
            )}
          </div>
        )}
      </div>

      {/* 3. WHERE YOU LEFT OFF */}
      <div className="space-y-1.5">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
            Where you left off
          </span>
          <button
            onClick={() => setShowWhy(!showWhy)}
            className={`inline-flex items-center gap-1 text-[11px] font-medium px-2 py-0.5 rounded transition-colors cursor-pointer ${
              showWhy
                ? 'bg-gray-100 text-gray-900'
                : 'text-gray-400 hover:text-gray-700 hover:bg-gray-50'
            }`}
            title="Explain why ContextSplit suggests this"
          >
            <span>Why?</span>
            {showWhy ? <ChevronUp size={10} /> : <ChevronDown size={10} />}
          </button>
        </div>

        <div className="text-sm font-medium text-gray-800">
          {whereLeftOff}
        </div>

        {/* Progressive Disclosure: "Why?" */}
        {showWhy && (
          <div className="mt-3 p-3.5 bg-gray-50/80 rounded-lg border border-gray-100 text-xs space-y-2 animate-in fade-in duration-150">
            <div className="text-[11px] font-medium text-gray-700">
              Why ContextSplit suggests this:
            </div>

            <ul className="space-y-1.5 pl-0.5">
              {explanationReasons.map((text, idx) => (
                <li key={idx} className="flex items-start gap-1.5 text-xs text-gray-700">
                  <span className="text-gray-400 select-none">•</span>
                  <span>{text}</span>
                </li>
              ))}
            </ul>

            {uncertaintyNote && (
              <div className="pt-2 border-t border-gray-100 text-[11px] text-gray-500 flex items-start gap-1.5">
                <Info size={12} className="flex-shrink-0 mt-0.5 text-gray-400" />
                <span>{uncertaintyNote}</span>
              </div>
            )}
          </div>
        )}
      </div>

      {/* 4. CONTINUE WITH */}
      <div className="space-y-2.5">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
            Continue with
          </span>
          {continueWithItems.length > 0 && (
            <span className="text-[11px] text-gray-400 font-normal">
              {continueWithItems.length} relevant item{continueWithItems.length === 1 ? '' : 's'}
            </span>
          )}
        </div>

        {continueWithItems.length > 0 ? (
          <div className="space-y-1">
            {continueWithItems.slice(0, 4).map((item) => (
              <div
                key={item.id}
                onClick={() => onSelectItem(item.id)}
                className="group flex items-center justify-between py-1.5 px-2 rounded-md hover:bg-gray-50 text-xs text-gray-800 transition-colors cursor-pointer"
              >
                <div className="flex items-center gap-2 min-w-0">
                  <span className="text-gray-300 text-xs select-none">•</span>
                  <span className="truncate group-hover:text-gray-900 font-normal">
                    {item.title}
                  </span>
                </div>
                {(item.app_name || item.domain) && (
                  <span className="text-[10px] text-gray-400 flex-shrink-0 ml-2 font-normal">
                    {item.app_name || item.domain}
                  </span>
                )}
              </div>
            ))}
          </div>
        ) : (
          <div className="text-xs text-gray-400 italic">
            No pending items
          </div>
        )}

        {/* Primary Continue Action */}
        <div className="pt-2 flex items-center gap-2">
          <button
            onClick={onOpenWorksetDetail}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-gray-900 bg-gray-50 hover:bg-gray-100 rounded-md border border-gray-200 transition-colors cursor-pointer"
            title="Navigate to existing contextual workset"
          >
            <span>Continue</span>
            <ArrowRight size={12} />
          </button>
        </div>
      </div>

      {/* Secondary Actions & View Timeline */}
      <div className="pt-4 border-t border-gray-100 flex items-center justify-between gap-3 text-xs text-gray-500">
        <button
          onClick={onToggleArchive}
          disabled={isUpdatingLifecycle || !overview}
          className="flex items-center gap-1.5 px-2 py-1 text-xs font-normal text-gray-500 hover:text-gray-800 transition-colors cursor-pointer disabled:opacity-50"
        >
          {overview?.lifecycle_state === 'archived' ? (
            <>
              <ArchiveRestore size={12} />
              <span>Restore context</span>
            </>
          ) : (
            <>
              <Archive size={12} />
              <span>Archive context</span>
            </>
          )}
        </button>

        <button
          onClick={onViewFullTimeline}
          className="flex items-center gap-1 text-xs font-normal text-gray-500 hover:text-gray-800 transition-colors cursor-pointer"
        >
          <span>View timeline</span>
          <ExternalLink size={11} />
        </button>
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

function formatSessionTime(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    if (isNaN(d.getTime())) return dateStr;
    const now = new Date();
    const isToday =
      d.getDate() === now.getDate() &&
      d.getMonth() === now.getMonth() &&
      d.getFullYear() === now.getFullYear();

    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    const isYesterday =
      d.getDate() === yesterday.getDate() &&
      d.getMonth() === yesterday.getMonth() &&
      d.getFullYear() === yesterday.getFullYear();

    const timeStr = d.toLocaleTimeString(undefined, {
      hour: 'numeric',
      minute: '2-digit',
    });

    if (isToday) {
      return `Today · ${timeStr}`;
    }
    if (isYesterday) {
      return `Yesterday · ${timeStr}`;
    }
    const datePart = d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
    return `${datePart} · ${timeStr}`;
  } catch {
    return dateStr;
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
