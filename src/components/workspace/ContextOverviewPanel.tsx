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
} from 'lucide-react';
import { db, ContextOverview, TimelineEvent, TimelineItemSummary } from '../../services/db';
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
  const [isLoading, setIsLoading] = useState(false);
  const [isUpdatingLifecycle, setIsUpdatingLifecycle] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'overview' | 'timeline'>('overview');
  const [timelineSearch, setTimelineSearch] = useState('');

  const targetWorkspace = workspaces.find((w) => w.id === overviewWorkspaceId);

  useEffect(() => {
    if (isOverviewOpen && overviewWorkspaceId) {
      loadOverview(overviewWorkspaceId);
    } else {
      setOverview(null);
    }
  }, [isOverviewOpen, overviewWorkspaceId]);

  const loadOverview = async (wsId: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await db.getContextOverview(wsId);
      setOverview(data);
    } catch (err: any) {
      setError(err?.toString() || 'Failed to load context overview');
    } finally {
      setIsLoading(false);
    }
  };

  // Close on Escape
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOverviewOpen) {
        closeOverview();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOverviewOpen, closeOverview]);

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
      await loadOverview(overview.workspace_id);
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
    <div className="fixed inset-0 z-50 flex justify-end bg-black/30 backdrop-blur-sm animate-in fade-in duration-200">
      {/* Backdrop click to close */}
      <div className="flex-1" onClick={closeOverview} />

      {/* Slide-over Drawer Panel */}
      <div className="w-full max-w-2xl h-full bg-white shadow-2xl flex flex-col border-l border-gray-200/80 animate-in slide-in-from-right duration-200">
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-100 flex items-center justify-between bg-gray-50/50">
          <div className="flex items-center gap-3 min-w-0">
            <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-indigo-500 to-violet-600 flex items-center justify-center text-white shadow-sm flex-shrink-0">
              <Layers size={20} />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <h2 className="text-lg font-semibold text-gray-900 truncate">
                  {overview?.name || targetWorkspace?.name || 'Context Overview'}
                </h2>
                {overview && (
                  <LifecycleBadge state={overview.lifecycle_state} />
                )}
              </div>
              <p className="text-xs text-gray-500 flex items-center gap-2 mt-0.5">
                <span>Context Overview & History</span>
                <span>•</span>
                <span>Local-only</span>
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => overviewWorkspaceId && loadOverview(overviewWorkspaceId)}
              disabled={isLoading}
              className="p-2 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors cursor-pointer"
              title="Refresh Overview"
            >
              <RefreshCw size={16} className={isLoading ? 'animate-spin text-indigo-600' : ''} />
            </button>
            <button
              onClick={closeOverview}
              className="p-2 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors cursor-pointer"
              title="Close panel (Esc)"
            >
              <X size={18} />
            </button>
          </div>
        </div>

        {/* Tab switcher */}
        <div className="flex border-b border-gray-100 px-6 bg-white">
          <button
            onClick={() => setActiveTab('overview')}
            className={`py-3 px-4 text-xs font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-2 ${
              activeTab === 'overview'
                ? 'border-indigo-600 text-indigo-600'
                : 'border-transparent text-gray-500 hover:text-gray-800'
            }`}
          >
            <Activity size={14} />
            <span>Context Summary</span>
          </button>
          <button
            onClick={() => setActiveTab('timeline')}
            className={`py-3 px-4 text-xs font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-2 ${
              activeTab === 'timeline'
                ? 'border-indigo-600 text-indigo-600'
                : 'border-transparent text-gray-500 hover:text-gray-800'
            }`}
          >
            <Clock size={14} />
            <span>Activity Timeline ({overview?.timeline.chronological_activity.length ?? 0})</span>
          </button>
        </div>

        {/* Content Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {isLoading && !overview ? (
            <div className="h-64 flex flex-col items-center justify-center text-gray-400 gap-3">
              <RefreshCw size={24} className="animate-spin text-indigo-600" />
              <p className="text-xs">Gathering contextual insights...</p>
            </div>
          ) : error ? (
            <div className="p-4 bg-red-50 text-red-700 rounded-xl border border-red-200 text-sm flex items-start gap-3">
              <AlertCircle size={18} className="flex-shrink-0 mt-0.5" />
              <div>
                <div className="font-semibold">Unable to load overview</div>
                <div className="text-xs text-red-600 mt-1">{error}</div>
              </div>
            </div>
          ) : overview ? (
            activeTab === 'overview' ? (
              <OverviewContent
                overview={overview}
                onToggleArchive={handleToggleArchive}
                isUpdatingLifecycle={isUpdatingLifecycle}
                onViewFullTimeline={() => {
                  closeOverview();
                  openTimeline(overview.workspace_id);
                }}
              />
            ) : (
              <TimelineTabContent
                events={filteredTimelineEvents}
                searchQuery={timelineSearch}
                onSearchChange={setTimelineSearch}
              />
            )
          ) : null}
        </div>

        {/* Footer Privacy Note */}
        <div className="px-6 py-3 border-t border-gray-100 bg-gray-50/50 flex items-center justify-between text-[11px] text-gray-400">
          <span className="flex items-center gap-1.5">
            <CheckCircle2 size={12} className="text-emerald-500" />
            No technical signals or raw telemetry exposed.
          </span>
          <span>Deterministic Context Engine</span>
        </div>
      </div>
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
      <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-emerald-50 text-emerald-700 border border-emerald-200/80">
        <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
        Active
      </span>
    );
  }
  if (state === 'dormant') {
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-amber-50 text-amber-700 border border-amber-200/80">
        <span className="w-1.5 h-1.5 rounded-full bg-amber-500" />
        Dormant
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-gray-100 text-gray-600 border border-gray-200">
      <Archive size={11} />
      Archived
    </span>
  );
}

interface OverviewContentProps {
  overview: ContextOverview;
  onToggleArchive: () => void;
  isUpdatingLifecycle: boolean;
  onViewFullTimeline: () => void;
}

function OverviewContent({
  overview,
  onToggleArchive,
  isUpdatingLifecycle,
  onViewFullTimeline,
}: OverviewContentProps) {
  return (
    <div className="space-y-6 animate-in fade-in duration-150">
      {/* 1. Context Metric Cards */}
      <div className="grid grid-cols-3 gap-3">
        <div className="p-4 rounded-xl bg-gray-50/80 border border-gray-100 flex flex-col justify-between">
          <div className="flex items-center justify-between text-gray-500">
            <span className="text-xs font-medium">Context Items</span>
            <Layers size={15} className="text-indigo-500" />
          </div>
          <div className="mt-2 text-2xl font-bold text-gray-900">
            {overview.item_count}
          </div>
          <span className="text-[11px] text-gray-400 mt-0.5">Physical & Confirmed</span>
        </div>

        <div className="p-4 rounded-xl bg-gray-50/80 border border-gray-100 flex flex-col justify-between">
          <div className="flex items-center justify-between text-gray-500">
            <span className="text-xs font-medium">Context Sessions</span>
            <Activity size={15} className="text-violet-500" />
          </div>
          <div className="mt-2 text-2xl font-bold text-gray-900">
            {overview.session_count}
          </div>
          <span className="text-[11px] text-gray-400 mt-0.5">Activity windows</span>
        </div>

        <div className="p-4 rounded-xl bg-gray-50/80 border border-gray-100 flex flex-col justify-between">
          <div className="flex items-center justify-between text-gray-500">
            <span className="text-xs font-medium">Recent Activity</span>
            <Clock size={15} className="text-emerald-500" />
          </div>
          <div className="mt-2 text-xs font-semibold text-gray-800 leading-tight">
            {overview.recent_activity ? formatRelativeDate(overview.recent_activity) : 'None yet'}
          </div>
          <span className="text-[10px] text-gray-400 mt-0.5 truncate">
            {overview.recent_activity ? new Date(overview.recent_activity).toLocaleDateString() : 'No activity'}
          </span>
        </div>
      </div>

      {/* 2. Lifecycle Status Banner & Action */}
      <div className="p-4 rounded-xl border border-gray-200/80 bg-white flex items-center justify-between shadow-xs">
        <div className="flex items-center gap-3">
          <div
            className={`w-9 h-9 rounded-lg flex items-center justify-center ${
              overview.lifecycle_state === 'active'
                ? 'bg-emerald-50 text-emerald-600'
                : overview.lifecycle_state === 'dormant'
                ? 'bg-amber-50 text-amber-600'
                : 'bg-gray-100 text-gray-600'
            }`}
          >
            {overview.lifecycle_state === 'active' ? (
              <CheckCircle2 size={18} />
            ) : overview.lifecycle_state === 'dormant' ? (
              <Clock size={18} />
            ) : (
              <Archive size={18} />
            )}
          </div>
          <div>
            <div className="text-xs font-semibold text-gray-900 capitalize">
              {overview.lifecycle_state} Context
            </div>
            <p className="text-[11px] text-gray-500 mt-0.5">
              {overview.lifecycle_state === 'active'
                ? 'Full contextual suggestion ranking and active relevance.'
                : overview.lifecycle_state === 'dormant'
                ? 'No recent activity; suggestion ranking discounted but available.'
                : 'Context archived by user. Suppressed from automated suggestions.'}
            </p>
          </div>
        </div>

        <button
          onClick={onToggleArchive}
          disabled={isUpdatingLifecycle}
          className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border border-gray-200 hover:bg-gray-50 text-gray-700 transition-colors cursor-pointer disabled:opacity-50"
        >
          {overview.lifecycle_state === 'archived' ? (
            <>
              <ArchiveRestore size={14} className="text-emerald-600" />
              <span>Restore</span>
            </>
          ) : (
            <>
              <Archive size={14} className="text-gray-500" />
              <span>Archive</span>
            </>
          )}
        </button>
      </div>

      {/* 3. Related Applications & Important Domains */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Related Applications */}
        <div className="p-4 rounded-xl bg-gray-50/60 border border-gray-100 flex flex-col">
          <div className="flex items-center gap-2 mb-3 text-xs font-semibold text-gray-800">
            <Laptop size={14} className="text-indigo-600" />
            <span>Related Applications</span>
          </div>

          {overview.related_applications.length > 0 ? (
            <div className="flex flex-wrap gap-1.5">
              {overview.related_applications.map((app) => (
                <span
                  key={app.name}
                  className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs bg-white border border-gray-200/80 text-gray-700 shadow-2xs"
                >
                  <span className="font-medium">{app.name}</span>
                  <span className="text-[10px] text-gray-400 bg-gray-100 px-1.5 py-0.2 rounded-full">
                    {app.item_count}
                  </span>
                </span>
              ))}
            </div>
          ) : (
            <p className="text-xs text-gray-400 italic">No specific applications recorded.</p>
          )}
        </div>

        {/* Important Domains */}
        <div className="p-4 rounded-xl bg-gray-50/60 border border-gray-100 flex flex-col">
          <div className="flex items-center gap-2 mb-3 text-xs font-semibold text-gray-800">
            <Globe size={14} className="text-emerald-600" />
            <span>Important Domains</span>
          </div>

          {overview.important_domains.length > 0 ? (
            <div className="flex flex-wrap gap-1.5">
              {overview.important_domains.map((dom) => (
                <span
                  key={dom.domain}
                  className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs bg-white border border-gray-200/80 text-gray-700 shadow-2xs"
                >
                  <span className="font-medium">{dom.domain}</span>
                  <span className="text-[10px] text-gray-400 bg-gray-100 px-1.5 py-0.2 rounded-full">
                    {dom.item_count}
                  </span>
                </span>
              ))}
            </div>
          ) : (
            <p className="text-xs text-gray-400 italic">No web domains captured.</p>
          )}
        </div>
      </div>

      {/* 4. Recent Items Preview */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-xs font-semibold text-gray-900">
            <FileText size={15} className="text-indigo-600" />
            <span>Recent Context Items ({overview.recent_items.length})</span>
          </div>
          <span className="text-[11px] text-gray-400">Latest active artifacts</span>
        </div>

        {overview.recent_items.length > 0 ? (
          <div className="space-y-2">
            {overview.recent_items.map((item) => (
              <RecentItemCard key={item.id} item={item} />
            ))}
          </div>
        ) : (
          <div className="p-6 text-center border border-dashed border-gray-200 rounded-xl text-xs text-gray-400">
            No items in this context yet.
          </div>
        )}
      </div>

      {/* 5. Quick Timeline Teaser */}
      <div className="p-4 rounded-xl bg-indigo-50/40 border border-indigo-100/60 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Clock size={16} className="text-indigo-600 flex-shrink-0" />
          <div>
            <div className="text-xs font-medium text-indigo-950">Contextual Activity Timeline</div>
            <div className="text-[11px] text-indigo-700/80 mt-0.5">
              {overview.timeline.chronological_activity.length} chronological events recorded.
            </div>
          </div>
        </div>
        <button
          onClick={onViewFullTimeline}
          className="flex items-center gap-1.5 text-xs font-medium text-indigo-700 hover:text-indigo-900 hover:underline cursor-pointer"
        >
          <span>Open Full Timeline</span>
          <ExternalLink size={12} />
        </button>
      </div>
    </div>
  );
}

function RecentItemCard({ item }: { item: TimelineItemSummary }) {
  const getItemIcon = (type: string) => {
    switch (type) {
      case 'url':
        return <Globe size={13} className="text-blue-500" />;
      case 'code':
        return <CodeIcon size={13} className="text-purple-500" />;
      case 'image':
        return <ImageIcon size={13} className="text-amber-500" />;
      default:
        return <FileText size={13} className="text-gray-500" />;
    }
  };

  return (
    <div className="p-3 rounded-lg border border-gray-100 bg-white hover:border-gray-200 transition-colors flex items-center justify-between gap-3 shadow-2xs">
      <div className="flex items-center gap-2.5 min-w-0">
        <div className="w-6 h-6 rounded-md bg-gray-50 flex items-center justify-center flex-shrink-0">
          {getItemIcon(item.item_type)}
        </div>
        <div className="min-w-0">
          <div className="text-xs font-medium text-gray-800 truncate">
            {item.title || item.content_snippet || 'Untitled Item'}
          </div>
          <div className="flex items-center gap-2 text-[10px] text-gray-400 mt-0.5">
            {item.app_name && <span>App: {item.app_name}</span>}
            {item.domain && <span>Domain: {item.domain}</span>}
            <span>{formatRelativeDate(item.captured_at)}</span>
          </div>
        </div>
      </div>

      <div className="flex items-center gap-1.5 flex-shrink-0">
        {item.membership_type === 'suggested' ? (
          <span className="inline-flex items-center gap-1 text-[10px] font-medium px-2 py-0.5 rounded-full bg-amber-50 text-amber-700 border border-amber-200/60">
            <Inbox size={9} />
            Inbox (Suggested)
          </span>
        ) : (
          <span className="inline-flex items-center gap-1 text-[10px] font-medium px-2 py-0.5 rounded-full bg-indigo-50 text-indigo-700 border border-indigo-200/60">
            <Folder size={9} />
            Confirmed
          </span>
        )}
      </div>
    </div>
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
          className="w-full pl-9 pr-3 py-1.5 text-xs bg-gray-50 rounded-lg border border-gray-200 focus:outline-none focus:ring-1 focus:ring-indigo-500"
        />
      </div>

      {events.length > 0 ? (
        <div className="space-y-3 relative before:absolute before:top-2 before:bottom-2 before:left-[17px] before:w-[2px] before:bg-gray-100">
          {events.map((ev) => (
            <div key={ev.id} className="flex items-start gap-3 relative">
              <div className="w-9 h-9 rounded-full bg-white border-2 border-indigo-500/20 flex items-center justify-center flex-shrink-0 text-indigo-600 shadow-2xs z-10">
                <Clock size={14} />
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
    if (diffMin < 60) return `${diffMin}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays === 1) return 'Yesterday';
    if (diffDays < 7) return `${diffDays}d ago`;
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  } catch {
    return dateStr;
  }
}
