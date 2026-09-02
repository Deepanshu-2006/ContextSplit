import { useState, useEffect, useMemo } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  Clock,
  Calendar,
  Layers,
  FileText,
  Globe,
  Image as ImageIcon,
  Code as CodeIcon,
  Sparkles,
  FolderInput,
  Search,
  Activity,
  RefreshCw,
} from 'lucide-react';
import { db, ContextTimeline, TimelineEvent, TimelineSession } from '../../services/db';
import { useWorkspaceStore } from '../../store/workspaceStore';

export function ContextTimelineDrawer() {
  const { isTimelineOpen, timelineWorkspaceId, closeTimeline, workspaces } = useWorkspaceStore();
  const [timeline, setTimeline] = useState<ContextTimeline | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeFilter, setActiveFilter] = useState<'all' | 'sessions' | 'items' | 'actions'>('all');
  const [searchQuery, setSearchQuery] = useState('');

  const targetWorkspace = workspaces.find((w) => w.id === timelineWorkspaceId);

  useEffect(() => {
    if (isTimelineOpen && timelineWorkspaceId) {
      loadTimeline(timelineWorkspaceId);
    } else {
      setTimeline(null);
    }
  }, [isTimelineOpen, timelineWorkspaceId]);

  const loadTimeline = async (wsId: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await db.getContextTimeline(wsId);
      setTimeline(data);
    } catch (err: any) {
      setError(err?.toString() || 'Failed to load timeline');
    } finally {
      setIsLoading(false);
    }
  };

  // Close on Escape
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isTimelineOpen) {
        closeTimeline();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isTimelineOpen, closeTimeline]);

  // Filter events based on activeFilter and searchQuery
  const filteredEvents = useMemo(() => {
    if (!timeline) return [];

    return timeline.chronological_activity.filter((ev) => {
      // Type filter
      if (activeFilter === 'items' && ev.event_type !== 'item_captured') return false;
      if (activeFilter === 'sessions' && !ev.event_type.startsWith('session_')) return false;
      if (activeFilter === 'actions' && ev.event_type !== 'user_action') return false;

      // Text search
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchesTitle = ev.title.toLowerCase().includes(q);
        const matchesDesc = ev.description.toLowerCase().includes(q);
        return matchesTitle || matchesDesc;
      }

      return true;
    });
  }, [timeline, activeFilter, searchQuery]);

  // Group events by day
  const groupedByDay = useMemo(() => {
    const groups: { [dateStr: string]: TimelineEvent[] } = {};

    for (const ev of filteredEvents) {
      const dateKey = formatDateKey(ev.timestamp);
      if (!groups[dateKey]) {
        groups[dateKey] = [];
      }
      groups[dateKey].push(ev);
    }

    return Object.entries(groups).map(([dateLabel, events]) => ({
      dateLabel,
      events,
    }));
  }, [filteredEvents]);

  // Map of sessions for quick metadata lookup
  const sessionMap = useMemo(() => {
    const map = new Map<string, TimelineSession>();
    if (timeline) {
      for (const s of timeline.sessions) {
        map.set(s.id, s);
      }
    }
    return map;
  }, [timeline]);

  if (!isTimelineOpen) return null;

  return createPortal(
    <div className="fixed inset-0 z-50 flex justify-end bg-black/20 backdrop-blur-xs transition-opacity duration-200 select-none">
      {/* Backdrop click to close */}
      <div className="absolute inset-0" onClick={closeTimeline} />

      {/* Drawer */}
      <div className="relative w-full max-w-lg h-full bg-white shadow-2xl border-l border-black/10 flex flex-col z-10 animate-in slide-in-from-right duration-250 ease-out">
        {/* Header */}
        <div className="px-5 py-4 border-b border-black/5 bg-slate-50/70 flex items-center justify-between gap-3">
          <div className="flex items-center gap-2.5 min-w-0">
            <div className="w-8 h-8 rounded-lg bg-indigo-50 border border-indigo-100/80 flex items-center justify-center text-indigo-600 shrink-0 shadow-xs">
              <Clock size={16} />
            </div>
            <div className="min-w-0">
              <h2 className="text-sm font-semibold text-slate-900 truncate flex items-center gap-1.5">
                <span>Context Timeline</span>
                <span className="text-slate-400 font-normal">•</span>
                <span className="text-indigo-600 font-medium truncate">
                  {targetWorkspace?.name || timeline?.workspace_name || 'Workspace'}
                </span>
              </h2>
              <p className="text-[11px] text-slate-500 truncate">
                {timeline ? `${timeline.total_items} items • ${timeline.total_sessions} sessions recorded` : 'Loading activity history...'}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-1">
            <button
              onClick={() => timelineWorkspaceId && loadTimeline(timelineWorkspaceId)}
              disabled={isLoading}
              title="Refresh timeline"
              className="p-1.5 text-slate-400 hover:text-slate-700 hover:bg-slate-200/60 rounded-md transition-colors cursor-pointer disabled:opacity-40"
            >
              <RefreshCw size={14} className={isLoading ? 'animate-spin' : ''} />
            </button>
            <button
              onClick={closeTimeline}
              className="p-1.5 text-slate-400 hover:text-slate-700 hover:bg-slate-200/60 rounded-md transition-colors cursor-pointer"
              title="Close timeline (Esc)"
            >
              <X size={16} />
            </button>
          </div>
        </div>

        {/* Filter bar & Search */}
        <div className="px-5 py-2.5 border-b border-black/5 bg-white flex flex-col gap-2">
          <div className="relative">
            <Search size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-400" />
            <input
              type="text"
              placeholder="Search contextual items or actions..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-slate-50 hover:bg-slate-100/70 focus:bg-white border border-slate-200/70 rounded-md py-1.5 pl-8 pr-3 text-[12px] text-slate-800 placeholder-slate-400 focus:outline-none focus:ring-1 focus:ring-indigo-500 transition-all"
            />
          </div>

          <div className="flex items-center gap-1.5">
            {(
              [
                { id: 'all', label: 'All Activity' },
                { id: 'sessions', label: 'Sessions' },
                { id: 'items', label: 'Items' },
                { id: 'actions', label: 'User Actions' },
              ] as const
            ).map((filter) => (
              <button
                key={filter.id}
                onClick={() => setActiveFilter(filter.id)}
                className={`px-2.5 py-1 rounded-md text-[11px] font-medium transition-colors cursor-pointer ${
                  activeFilter === filter.id
                    ? 'bg-indigo-50 text-indigo-700 font-semibold border border-indigo-200/60 shadow-2xs'
                    : 'text-slate-500 hover:text-slate-800 hover:bg-slate-100'
                }`}
              >
                {filter.label}
              </button>
            ))}
          </div>
        </div>

        {/* Timeline Content */}
        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-6">
          {isLoading && (
            <div className="py-12 flex flex-col items-center justify-center text-center">
              <RefreshCw size={24} className="text-indigo-500 animate-spin mb-3" />
              <p className="text-xs text-slate-500">Reconstructing context timeline...</p>
            </div>
          )}

          {error && (
            <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-xs text-red-600">
              {error}
            </div>
          )}

          {!isLoading && !error && groupedByDay.length === 0 && (
            <div className="py-16 flex flex-col items-center justify-center text-center">
              <div className="w-12 h-12 rounded-xl bg-slate-50 border border-slate-100 flex items-center justify-center text-slate-400 mb-3">
                <Activity size={22} />
              </div>
              <h3 className="text-sm font-medium text-slate-700">No activity found</h3>
              <p className="text-xs text-slate-400 max-w-xs mt-1">
                {searchQuery
                  ? 'No activity matches your search filter.'
                  : 'Items and sessions associated with this workspace will appear chronologically here.'}
              </p>
            </div>
          )}

          {!isLoading &&
            !error &&
            groupedByDay.map(({ dateLabel, events }) => (
              <div key={dateLabel} className="space-y-3">
                {/* Date header */}
                <div className="sticky top-0 bg-white/95 backdrop-blur-xs py-1 z-5 flex items-center gap-2">
                  <span className="text-[11px] font-semibold text-slate-500 uppercase tracking-wider">
                    {dateLabel}
                  </span>
                  <div className="h-px flex-1 bg-slate-100" />
                  <span className="text-[10px] text-slate-400 font-medium">
                    {events.length} {events.length === 1 ? 'event' : 'events'}
                  </span>
                </div>

                {/* Event stream */}
                <div className="relative pl-5 space-y-3 border-l border-slate-200/70 ml-2.5">
                  {events.map((event) => {
                    return (
                      <TimelineEventRow
                        key={event.id}
                        event={event}
                        session={event.session_id ? sessionMap.get(event.session_id) : undefined}
                      />
                    );
                  })}
                </div>
              </div>
            ))}
        </div>

        {/* Footer Summary */}
        {timeline && (
          <div className="p-3 border-t border-black/5 bg-slate-50/50 flex items-center justify-between text-[11px] text-slate-500 px-5">
            <span className="flex items-center gap-1.5">
              <Calendar size={12} className="text-slate-400" />
              <span>Created {formatDate(timeline.created_at)}</span>
            </span>
            {timeline.latest_activity_at && (
              <span>Last activity {formatTimeAgo(timeline.latest_activity_at)}</span>
            )}
          </div>
        )}
      </div>
    </div>,
    document.body
  );
}

// ---------------------------------------------------------------------------
// Single Event Row
// ---------------------------------------------------------------------------

function TimelineEventRow({
  event,
  session,
}: {
  event: TimelineEvent;
  session?: TimelineSession;
}) {
  const timeFormatted = formatTime(event.timestamp);

  if (event.event_type === 'item_captured') {
    const itemType = (event.metadata?.item_type as string) || 'text';
    const appName = event.metadata?.app_name as string | undefined;
    const domain = event.metadata?.domain as string | undefined;
    const membershipType = event.metadata?.membership_type as string | undefined;

    return (
      <div className="relative group flex items-start gap-3 bg-white p-2.5 rounded-lg border border-slate-100 hover:border-slate-200 shadow-2xs hover:shadow-xs transition-all">
        {/* Node point on timeline */}
        <div className="absolute -left-[27px] top-3.5 w-2 h-2 rounded-full bg-indigo-500 ring-4 ring-white" />

        <div className="w-7 h-7 rounded-md bg-indigo-50/70 border border-indigo-100/60 flex items-center justify-center text-indigo-600 shrink-0 mt-0.5">
          <ItemTypeIcon type={itemType} />
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-baseline justify-between gap-2">
            <h4 className="text-[12px] font-medium text-slate-800 truncate">{event.title}</h4>
            <span className="text-[10px] text-slate-400 shrink-0 font-medium">{timeFormatted}</span>
          </div>

          <p className="text-[11px] text-slate-500 truncate mt-0.5">{event.description}</p>

          <div className="flex items-center gap-1.5 mt-1.5 flex-wrap">
            {(appName || domain) && (
              <span className="inline-flex items-center gap-1 text-[10px] font-medium bg-slate-100 text-slate-600 px-1.5 py-0.5 rounded">
                <Globe size={10} className="text-slate-400" />
                <span>{appName && domain ? `${appName} (${domain})` : appName || domain}</span>
              </span>
            )}

            {membershipType && (
              <span
                className={`text-[9px] font-medium uppercase tracking-wider px-1.5 py-0.5 rounded ${
                  membershipType === 'confirmed'
                    ? 'bg-emerald-50 text-emerald-700 border border-emerald-200/50'
                    : 'bg-amber-50 text-amber-700 border border-amber-200/50'
                }`}
              >
                {membershipType}
              </span>
            )}

            {session && (
              <span className="text-[10px] text-slate-400">
                • in session #{session.id.slice(0, 6)}
              </span>
            )}
          </div>
        </div>
      </div>
    );
  }

  if (event.event_type.startsWith('session_')) {
    return (
      <div className="relative group flex items-start gap-3 bg-gradient-to-r from-blue-50/40 via-indigo-50/30 to-white p-2.5 rounded-lg border border-blue-100/60 transition-all">
        <div className="absolute -left-[27px] top-3.5 w-2 h-2 rounded-full bg-blue-500 ring-4 ring-white" />

        <div className="w-7 h-7 rounded-md bg-blue-100/60 border border-blue-200/60 flex items-center justify-center text-blue-700 shrink-0 mt-0.5">
          <Layers size={13} />
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-baseline justify-between gap-2">
            <h4 className="text-[12px] font-semibold text-blue-950 truncate">{event.title}</h4>
            <span className="text-[10px] text-blue-500 shrink-0 font-medium">{timeFormatted}</span>
          </div>
          <p className="text-[11px] text-blue-800/80 mt-0.5">{event.description}</p>
        </div>
      </div>
    );
  }

  if (event.event_type === 'user_action') {
    return (
      <div className="relative group flex items-start gap-3 bg-slate-50/80 p-2.5 rounded-lg border border-slate-200/60 shadow-2xs transition-all">
        <div className="absolute -left-[27px] top-3.5 w-2 h-2 rounded-full bg-slate-500 ring-4 ring-white" />

        <div className="w-7 h-7 rounded-md bg-slate-200/70 border border-slate-300/60 flex items-center justify-center text-slate-700 shrink-0 mt-0.5">
          <FolderInput size={13} />
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-baseline justify-between gap-2">
            <h4 className="text-[12px] font-medium text-slate-900 truncate">{event.title}</h4>
            <span className="text-[10px] text-slate-400 shrink-0 font-medium">{timeFormatted}</span>
          </div>
          <p className="text-[11px] text-slate-500 mt-0.5">{event.description}</p>
        </div>
      </div>
    );
  }

  // Relationship formed or default
  return (
    <div className="relative group flex items-start gap-3 bg-white p-2.5 rounded-lg border border-slate-100 transition-all">
      <div className="absolute -left-[27px] top-3.5 w-2 h-2 rounded-full bg-purple-500 ring-4 ring-white" />

      <div className="w-7 h-7 rounded-md bg-purple-50 border border-purple-100 flex items-center justify-center text-purple-600 shrink-0 mt-0.5">
        <Sparkles size={13} />
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-baseline justify-between gap-2">
          <h4 className="text-[12px] font-medium text-slate-800 truncate">{event.title}</h4>
          <span className="text-[10px] text-slate-400 shrink-0 font-medium">{timeFormatted}</span>
        </div>
        <p className="text-[11px] text-slate-500 mt-0.5">{event.description}</p>
      </div>
    </div>
  );
}

function ItemTypeIcon({ type }: { type: string }) {
  switch (type) {
    case 'url':
      return <Globe size={13} />;
    case 'image':
      return <ImageIcon size={13} />;
    case 'code':
      return <CodeIcon size={13} />;
    default:
      return <FileText size={13} />;
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  } catch {
    return '';
  }
}

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleDateString([], { month: 'short', day: 'numeric', year: 'numeric' });
  } catch {
    return iso;
  }
}

function formatDateKey(iso: string): string {
  try {
    const d = new Date(iso);
    const today = new Date();
    const yesterday = new Date();
    yesterday.setDate(today.getDate() - 1);

    if (d.toDateString() === today.toDateString()) {
      return 'Today';
    }
    if (d.toDateString() === yesterday.toDateString()) {
      return 'Yesterday';
    }
    return d.toLocaleDateString([], {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
      year: d.getFullYear() !== today.getFullYear() ? 'numeric' : undefined,
    });
  } catch {
    return 'Recent';
  }
}

function formatTimeAgo(iso: string): string {
  try {
    const d = new Date(iso);
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    if (diffMins < 1) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    const diffDays = Math.floor(diffHours / 24);
    if (diffDays === 1) return 'yesterday';
    return `${diffDays}d ago`;
  } catch {
    return '';
  }
}
