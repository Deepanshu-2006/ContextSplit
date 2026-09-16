import { useState, useEffect, useMemo } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  Clock,
  Layers,
  Globe,
  Search,
  Activity,
  RefreshCw,
  Archive,
  ArchiveRestore,
  ExternalLink,
  Laptop,
  CheckCircle2,
  AlertCircle,
  ArrowRight,
  Calendar,
  Check,
  Copy,
} from 'lucide-react';
import { db, ContextOverview, TimelineEvent, Item } from '../../services/db';
import { contextMemoryService, ContextMemoryResponse } from '../../services/contextMemory';
import { contextWorksetService, ContextWorkset } from '../../services/contextWorkset';
import { contextStateService, ContextState, ContextStateValue } from '../../services/contextState';
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
          {isLoading && !overview && !memory && !contextState ? (
            <div className="h-64 flex flex-col items-center justify-center text-gray-400 gap-2">
              <RefreshCw size={20} className="animate-spin text-gray-500" />
              <p className="text-xs">Reconstructing working state...</p>
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

        {/* Footer */}
        <div className="px-6 py-3 border-t border-gray-100 bg-gray-50/50 flex items-center justify-between text-[11px] text-gray-400">
          <span className="flex items-center gap-1.5">
            <CheckCircle2 size={12} className="text-emerald-500" />
            Human-readable contextual working state
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

  // 1. Context Working State (Active, Recently active, etc.)
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
  const isTaskInferred = Boolean(
    isTaskAvailable &&
      contextState?.provenance !== 'confirmed' &&
      workset?.provenance !== 'confirmed' &&
      memory?.recent_task?.provenance !== 'confirmed'
  );

  // Last active timestamp
  const lastActiveTimestamp =
    contextState?.last_active_at ||
    workset?.last_active_at ||
    memory?.last_active_at ||
    overview?.recent_activity;
  const formattedLastActive = lastActiveTimestamp ? formatLastActive(lastActiveTimestamp) : 'Active recently';

  // 3. WHERE YOU LEFT OFF
  // Conservative derivation: if state is uncertain or no specific activity, do not fabricate.
  const isStateUncertain = stateValue === 'uncertain' || stateValue === 'insufficient_evidence';
  let whereLeftOff: string;
  let isLeftOffInferred = false;

  if (isStateUncertain) {
    whereLeftOff = `Recent activity in ${contextName}`;
  } else if (
    contextState?.last_meaningful_activity &&
    contextState.last_meaningful_activity.trim().length > 0 &&
    !contextState.last_meaningful_activity.toLowerCase().includes('unknown') &&
    contextState.last_meaningful_activity !== `Recent activity in ${contextName}`
  ) {
    whereLeftOff = contextState.last_meaningful_activity;
    isLeftOffInferred =
      contextState.meaningful_activity?.provenance === 'inferred' || contextState.provenance === 'inferred';
  } else if (contextState?.resume_state?.last_meaningful_activity) {
    whereLeftOff = contextState.resume_state.last_meaningful_activity;
    isLeftOffInferred = true;
  } else if (contextState?.resume_state?.left_off_on) {
    whereLeftOff = contextState.resume_state.left_off_on;
    isLeftOffInferred = true;
  } else {
    whereLeftOff = `Recent activity in ${contextName}`;
  }

  // 4. Continue with items
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

  return (
    <div className="space-y-6 animate-in fade-in duration-150">
      {/* 1. Context · State Header */}
      <div className="flex items-center justify-between pb-3 border-b border-gray-100">
        <div className="flex items-center gap-2 text-sm font-semibold text-gray-900">
          <span>{contextName}</span>
          <span className="text-gray-300 font-normal">·</span>
          <span className="text-xs font-medium text-gray-600">
            {stateLabel}
          </span>
        </div>
      </div>

      {/* 2. YOU WERE WORKING ON */}
      <div className="space-y-1.5">
        <div className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          YOU WERE WORKING ON
        </div>
        <div className="flex items-baseline gap-2 flex-wrap">
          <h3 className="text-base font-medium text-gray-900 tracking-tight">
            {taskTitle}
          </h3>
          {isTaskInferred && taskTitle !== `Recent work in ${contextName}` && (
            <span className="text-[10px] text-gray-400 font-normal">
              Inferred
            </span>
          )}
        </div>
        <div className="text-xs text-gray-500">
          {formattedLastActive}
        </div>
      </div>

      {/* 3. WHERE YOU LEFT OFF */}
      <div className="space-y-1.5 pt-1">
        <div className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          WHERE YOU LEFT OFF
        </div>
        <div className="flex items-baseline gap-2 flex-wrap">
          <p className="text-sm font-medium text-gray-800">
            {whereLeftOff}
          </p>
          {isLeftOffInferred && whereLeftOff !== `Recent activity in ${contextName}` && (
            <span className="text-[10px] text-gray-400 font-normal">
              Inferred
            </span>
          )}
        </div>
      </div>

      {/* 4. Continue with */}
      <div className="space-y-2.5 pt-1">
        <div className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          Continue with
        </div>
        {continueWithItems.length > 0 ? (
          <div className="space-y-1">
            {continueWithItems.slice(0, 5).map((item) => (
              <div
                key={item.id}
                onClick={() => onSelectItem(item.id)}
                className="group flex items-center justify-between py-1.5 px-2 rounded-md hover:bg-gray-50 text-xs text-gray-800 transition-colors cursor-pointer"
              >
                <div className="flex items-center gap-2 min-w-0">
                  <span className="text-gray-400 text-xs select-none">•</span>
                  <span className="truncate group-hover:text-gray-900 font-normal">
                    {item.title}
                  </span>
                </div>
                {(item.app_name || item.domain) && (
                  <span className="text-[10px] text-gray-400 flex-shrink-0 ml-2">
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

        {/* [ View recent work ] */}
        <div className="pt-2">
          <button
            onClick={onOpenWorksetDetail}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-gray-700 bg-gray-50 hover:bg-gray-100 hover:text-gray-900 rounded-lg border border-gray-200/80 transition-colors cursor-pointer"
          >
            <span>View recent work</span>
            <ArrowRight size={12} />
          </button>
        </div>
      </div>

      {/* Bottom Context Actions */}
      <div className="pt-4 border-t border-gray-100 flex items-center justify-between gap-3 text-xs text-gray-500">
        <button
          onClick={onToggleArchive}
          disabled={isUpdatingLifecycle || !overview}
          className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded-md border border-gray-200 hover:bg-gray-50 text-gray-600 transition-colors cursor-pointer disabled:opacity-50"
        >
          {overview?.lifecycle_state === 'archived' ? (
            <>
              <ArchiveRestore size={12} />
              <span>Restore Context</span>
            </>
          ) : (
            <>
              <Archive size={12} />
              <span>Archive Context</span>
            </>
          )}
        </button>

        <button
          onClick={onViewFullTimeline}
          className="flex items-center gap-1 text-xs font-medium text-gray-500 hover:text-gray-800 transition-colors cursor-pointer"
        >
          <span>Timeline</span>
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
