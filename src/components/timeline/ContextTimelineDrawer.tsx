import { useState, useEffect, useMemo } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  Clock,
  Calendar,
  FileText,
  Globe,
  Image as ImageIcon,
  Code as CodeIcon,
  Search,
  Activity,
  RefreshCw,
  Layers,
  ShieldCheck,
  CheckCircle2,
} from 'lucide-react';
import { db, ContextTimeline, TimelineEvent, TimelineSession } from '../../services/db';
import { useWorkspaceStore } from '../../store/workspaceStore';

export function ContextTimelineDrawer() {
  const { isTimelineOpen, timelineWorkspaceId, closeTimeline, workspaces } = useWorkspaceStore();
  const [timeline, setTimeline] = useState<ContextTimeline | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeFilter, setActiveFilter] = useState<'all' | 'sessions' | 'items' | 'transitions' | 'actions'>('all');
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
      if (activeFilter === 'transitions' && ev.event_type !== 'context_transition') return false;
      if (activeFilter === 'items' && ev.event_type !== 'item_captured') return false;
      if (activeFilter === 'sessions' && !ev.event_type.startsWith('session_')) return false;
      if (activeFilter === 'actions' && ev.event_type !== 'user_action') return false;

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

  const filterTabs = [
    { id: 'all', label: 'All' },
    { id: 'sessions', label: 'Sessions' },
    { id: 'items', label: 'Captures' },
    { id: 'actions', label: 'Actions' },
    { id: 'transitions', label: 'Transitions' },
  ] as const;

  return createPortal(
    <div className="fixed inset-0 z-50 flex justify-end bg-black/15 backdrop-blur-[2px] transition-opacity duration-150 select-none">
      <div className="absolute inset-0" onClick={closeTimeline} />

      {/* Slide-over Timeline Panel */}
      <div className="relative w-full max-w-[460px] h-full bg-[#FAFAFA] border-l border-zinc-200/90 shadow-2xl flex flex-col z-10 animate-in slide-in-from-right duration-200 ease-out text-zinc-900">
        {/* Header */}
        <div className="h-13 px-4 border-b border-zinc-200/80 bg-white flex items-center justify-between shrink-0">
          <div className="flex items-center gap-2.5 min-w-0">
            <div className="w-7 h-7 rounded-md bg-zinc-100 flex items-center justify-center text-zinc-700 shrink-0">
              <Clock size={14} />
            </div>
            <div className="min-w-0">
              <h2 className="text-[13px] font-semibold text-zinc-900 truncate flex items-center gap-1.5 leading-tight">
                <span>Timeline</span>
                <span className="text-zinc-300 font-normal">/</span>
                <span className="text-zinc-700 font-medium truncate">
                  {targetWorkspace?.name || timeline?.workspace_name || 'Workspace'}
                </span>
              </h2>
              <p className="text-[11px] text-zinc-400 truncate mt-0.5">
                {timeline
                  ? `${timeline.total_items} ${timeline.total_items === 1 ? 'item' : 'items'} • ${timeline.total_sessions} ${timeline.total_sessions === 1 ? 'session' : 'sessions'}`
                  : 'Loading history...'}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-0.5">
            <button
              onClick={() => timelineWorkspaceId && loadTimeline(timelineWorkspaceId)}
              disabled={isLoading}
              title="Refresh timeline"
              className="w-7 h-7 flex items-center justify-center text-zinc-500 hover:text-zinc-900 hover:bg-zinc-100 rounded-md transition-colors cursor-pointer disabled:opacity-40"
            >
              <RefreshCw size={13} className={isLoading ? 'animate-spin' : ''} />
            </button>
            <div className="w-px h-3.5 bg-zinc-200 mx-1" />
            <button
              onClick={closeTimeline}
              className="w-7 h-7 flex items-center justify-center text-zinc-500 hover:text-zinc-900 hover:bg-zinc-100 rounded-md transition-colors cursor-pointer"
              title="Close (Esc)"
            >
              <X size={15} />
            </button>
          </div>
        </div>

        {/* Search & Segmented Filter Bar */}
        <div className="px-4 py-2.5 border-b border-zinc-200/70 bg-white flex flex-col gap-2">
          {/* Search Box */}
          <div className="relative">
            <Search size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-zinc-400" />
            <input
              type="text"
              placeholder="Search timeline..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-zinc-50 hover:bg-zinc-100/70 focus:bg-white border border-zinc-200/80 rounded-lg py-1.5 pl-8 pr-3 text-[12px] text-zinc-800 placeholder-zinc-400 focus:outline-none focus:ring-1 focus:ring-zinc-400 transition-all"
            />
          </div>

          {/* Segmented Control */}
          <div className="flex items-center p-0.5 bg-zinc-100 rounded-lg text-[11px] font-medium text-zinc-500">
            {filterTabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveFilter(tab.id)}
                className={`flex-1 py-1 text-center rounded-md transition-all cursor-pointer ${
                  activeFilter === tab.id
                    ? 'bg-white text-zinc-900 font-semibold shadow-2xs'
                    : 'hover:text-zinc-900'
                }`}
              >
                {tab.label}
              </button>
            ))}
          </div>
        </div>

        {/* Timeline Event Stream */}
        <div className="flex-1 overflow-y-auto px-4 py-3 space-y-5">
          {isLoading && (
            <div className="py-16 flex flex-col items-center justify-center text-center">
              <RefreshCw size={20} className="text-zinc-400 animate-spin mb-2.5" />
              <p className="text-[12px] text-zinc-500">Loading timeline events...</p>
            </div>
          )}

          {error && (
            <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-[12px] text-red-600">
              {error}
            </div>
          )}

          {!isLoading && !error && groupedByDay.length === 0 && (
            <div className="py-20 flex flex-col items-center justify-center text-center">
              <div className="w-10 h-10 rounded-xl bg-zinc-100 flex items-center justify-center text-zinc-400 mb-2.5">
                <Activity size={18} />
              </div>
              <h3 className="text-[13px] font-medium text-zinc-700">No activity recorded</h3>
              <p className="text-[11px] text-zinc-400 max-w-xs mt-1">
                {searchQuery
                  ? 'No activity matches your search filter.'
                  : 'Items, sessions, and activity associated with this workspace will appear chronologically here.'}
              </p>
            </div>
          )}

          {!isLoading &&
            !error &&
            groupedByDay.map(({ dateLabel, events }) => (
              <div key={dateLabel} className="space-y-2">
                {/* Date subheader */}
                <div className="sticky top-0 bg-[#FAFAFA]/95 backdrop-blur-xs py-1 z-5 flex items-center justify-between">
                  <span className="text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">
                    {dateLabel}
                  </span>
                  <span className="text-[10px] text-zinc-400 font-medium">
                    {events.length} {events.length === 1 ? 'event' : 'events'}
                  </span>
                </div>

                {/* Event stream container */}
                <div className="relative pl-4 space-y-2 border-l border-zinc-200/80 ml-2">
                  {events.map((event) => (
                    <TimelineEventRow
                      key={event.id}
                      event={event}
                      session={event.session_id ? sessionMap.get(event.session_id) : undefined}
                    />
                  ))}
                </div>
              </div>
            ))}
        </div>

        {/* Footer info */}
        {timeline && (
          <div className="h-10 px-4 border-t border-zinc-200/80 bg-white flex items-center justify-between text-[11px] text-zinc-400 shrink-0">
            <span className="flex items-center gap-1.5">
              <Calendar size={11} className="text-zinc-400" />
              <span>Created {formatDate(timeline.created_at)}</span>
            </span>
            {timeline.latest_activity_at && (
              <span>Last active {formatTimeAgo(timeline.latest_activity_at)}</span>
            )}
          </div>
        )}
      </div>
    </div>,
    document.body
  );
}

// ---------------------------------------------------------------------------
// Event Row Component
// ---------------------------------------------------------------------------

function TimelineEventRow({
  event,
  session,
}: {
  event: TimelineEvent;
  session?: TimelineSession;
}) {
  const timeFormatted = formatTime(event.timestamp);

  // 1. Context Transition Event
  if (event.event_type === 'context_transition') {
    const toContextName =
      (event.metadata?.to_context_name as string) ||
      (event.metadata?.inferred_context as any)?.name;
    const confidence =
      typeof event.metadata?.confidence === 'number' ? event.metadata.confidence : 0.9;
    const confidencePct = Math.round(confidence * 100);

    return (
      <div className="relative group">
        <div className="absolute -left-[21px] top-3 w-2 h-2 rounded-full bg-zinc-400 ring-4 ring-[#FAFAFA]" />
        <div className="p-2.5 rounded-lg border border-zinc-200/80 bg-white hover:border-zinc-300 transition-colors">
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-1.5 flex-wrap">
                <span className="text-[12px] font-medium text-zinc-900">
                  {event.title}
                </span>
                {toContextName && (
                  <span className="text-[10px] font-medium px-1.5 py-0.2 rounded bg-zinc-100 text-zinc-700 border border-zinc-200/60">
                    {toContextName}
                  </span>
                )}
              </div>
              <p className="text-[11px] text-zinc-500 mt-0.5 leading-normal">
                {event.description}
              </p>
            </div>
            <span className="text-[10px] text-zinc-400 font-mono shrink-0 mt-0.5">
              {timeFormatted}
            </span>
          </div>

          {confidencePct > 0 && (
            <div className="mt-1.5 pt-1.5 border-t border-zinc-100 flex items-center gap-2 text-[10px] text-zinc-400">
              <ShieldCheck size={11} className="text-zinc-500" />
              <span>Inferred switch • {confidencePct}% confidence</span>
            </div>
          )}
        </div>
      </div>
    );
  }

  // 2. Captured Item Event
  if (event.event_type === 'item_captured') {
    const itemType = (event.metadata?.item_type as string) || 'text';
    const appName = event.metadata?.app_name as string | undefined;
    const domain = event.metadata?.domain as string | undefined;

    return (
      <div className="relative group">
        <div className="absolute -left-[21px] top-3 w-2 h-2 rounded-full bg-zinc-900 ring-4 ring-[#FAFAFA]" />
        <div className="p-2.5 rounded-lg border border-zinc-200/80 bg-white hover:border-zinc-300 transition-colors">
          <div className="flex items-start justify-between gap-2">
            <div className="flex items-start gap-2 min-w-0 flex-1">
              <div className="w-5 h-5 rounded bg-zinc-100 flex items-center justify-center text-zinc-600 shrink-0 mt-0.5">
                <ItemTypeIcon type={itemType} />
              </div>
              <div className="min-w-0 flex-1">
                <h4 className="text-[12px] font-medium text-zinc-900 truncate">
                  {event.title}
                </h4>
                <div className="flex items-center gap-1.5 text-[10px] text-zinc-400 mt-0.5 flex-wrap">
                  {(appName || domain) && (
                    <span className="flex items-center gap-1 text-zinc-600">
                      <Globe size={10} className="text-zinc-400" />
                      <span>{appName && domain ? `${appName} • ${domain}` : appName || domain}</span>
                    </span>
                  )}
                  {session && (
                    <span>• session #{session.id.slice(0, 6)}</span>
                  )}
                </div>
              </div>
            </div>
            <span className="text-[10px] text-zinc-400 font-mono shrink-0 mt-0.5">
              {timeFormatted}
            </span>
          </div>
        </div>
      </div>
    );
  }

  // 3. Context Session Event
  if (event.event_type.startsWith('session_')) {
    const isEnded = event.event_type === 'session_ended';

    return (
      <div className="relative group">
        <div className="absolute -left-[21px] top-3 w-2 h-2 rounded-full bg-zinc-400 ring-4 ring-[#FAFAFA]" />
        <div className="p-2.5 rounded-lg border border-zinc-200/80 bg-white hover:border-zinc-300 transition-colors flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0 flex-1">
            <div className="w-5 h-5 rounded bg-zinc-100 flex items-center justify-center text-zinc-600 shrink-0">
              <Layers size={11} />
            </div>
            <div className="min-w-0 flex-1">
              <h4 className="text-[12px] font-medium text-zinc-800 truncate">
                {isEnded ? 'Session ended' : 'Session active'}
              </h4>
              <p className="text-[11px] text-zinc-400 truncate">
                {event.description.replace('Session concluded with status: ended', 'Context session concluded')}
              </p>
            </div>
          </div>
          <span className="text-[10px] text-zinc-400 font-mono shrink-0">
            {timeFormatted}
          </span>
        </div>
      </div>
    );
  }

  // 4. User Action Event
  if (event.event_type === 'user_action') {
    // Format human-friendly action descriptions
    let actionLabel = event.title;
    if (actionLabel.startsWith('Moved ')) {
      actionLabel = event.title;
    } else if (actionLabel.includes('memory_task_confirmed')) {
      actionLabel = 'Confirmed context item';
    }

    return (
      <div className="relative group">
        <div className="absolute -left-[21px] top-3 w-2 h-2 rounded-full bg-zinc-400 ring-4 ring-[#FAFAFA]" />
        <div className="p-2.5 rounded-lg border border-zinc-200/80 bg-white hover:border-zinc-300 transition-colors flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0 flex-1">
            <div className="w-5 h-5 rounded bg-zinc-100 flex items-center justify-center text-zinc-600 shrink-0">
              <CheckCircle2 size={11} />
            </div>
            <div className="min-w-0 flex-1">
              <h4 className="text-[12px] font-medium text-zinc-800 truncate">
                {actionLabel}
              </h4>
              <p className="text-[11px] text-zinc-400 truncate">
                User action in workspace
              </p>
            </div>
          </div>
          <span className="text-[10px] text-zinc-400 font-mono shrink-0">
            {timeFormatted}
          </span>
        </div>
      </div>
    );
  }

  // Default fallback row
  return (
    <div className="relative group">
      <div className="absolute -left-[21px] top-3 w-2 h-2 rounded-full bg-zinc-300 ring-4 ring-[#FAFAFA]" />
      <div className="p-2.5 rounded-lg border border-zinc-200/80 bg-white flex items-center justify-between gap-2">
        <div className="min-w-0 flex-1">
          <h4 className="text-[12px] font-medium text-zinc-700 truncate">
            {event.title}
          </h4>
          <p className="text-[11px] text-zinc-400 truncate mt-0.5">
            {event.description}
          </p>
        </div>
        <span className="text-[10px] text-zinc-400 font-mono shrink-0">
          {timeFormatted}
        </span>
      </div>
    </div>
  );
}

function ItemTypeIcon({ type }: { type: string }) {
  switch (type) {
    case 'url':
      return <Globe size={11} />;
    case 'image':
      return <ImageIcon size={11} />;
    case 'code':
      return <CodeIcon size={11} />;
    default:
      return <FileText size={11} />;
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
