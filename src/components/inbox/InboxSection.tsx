import { MouseEvent, ComponentType } from 'react';
import {
  AlertTriangle,
  Sparkles,
  Layers,
  Clock,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  CheckCheck,
} from 'lucide-react';
import { Workspace } from '../../services/db';
import { DerivedInboxItem, InboxSectionKey, useInboxStore } from '../../store/inboxStore';
import { InboxItemCard } from './InboxItemCard';

interface InboxSectionProps {
  sectionKey: InboxSectionKey;
  items: DerivedInboxItem[];
  workspaces: Workspace[];
  onMoveToWorkspace: (itemId: string, workspaceId: string) => void;
}

const SECTION_CONFIG: Record<
  InboxSectionKey,
  {
    title: string;
    description: string;
    icon: ComponentType<{ size?: number; className?: string }>;
    accentColor: string;
    badgeColor: string;
    bgLinear: string;
    emptyText: string;
  }
> = {
  needs_attention: {
    title: 'Needs Attention',
    description: 'Items requiring manual clarification, ambiguity resolution, or user review',
    icon: AlertTriangle,
    accentColor: 'text-amber-600',
    badgeColor: 'bg-amber-100 text-amber-800 border-amber-200',
    bgLinear: 'from-amber-500/5 to-transparent',
    emptyText: 'No items currently require immediate attention.',
  },
  new_candidates: {
    title: 'New Context Candidates',
    description: 'Items belonging to newly discovered context clusters',
    icon: Layers,
    accentColor: 'text-cyan-600',
    badgeColor: 'bg-cyan-100 text-cyan-800 border-cyan-200',
    bgLinear: 'from-cyan-500/5 to-transparent',
    emptyText: 'No emerging context clusters found.',
  },
  context_suggestions: {
    title: 'Context Suggestions',
    description: 'Items with high-confidence workspace recommendations ready for organization',
    icon: Sparkles,
    accentColor: 'text-indigo-600',
    badgeColor: 'bg-indigo-100 text-indigo-800 border-indigo-200',
    bgLinear: 'from-indigo-500/5 to-transparent',
    emptyText: 'No pending workspace suggestions.',
  },
  recently_captured: {
    title: 'Recently Captured',
    description: 'Fresh captures in inbox waiting for context emergence',
    icon: Clock,
    accentColor: 'text-sky-600',
    badgeColor: 'bg-sky-100 text-sky-800 border-sky-200',
    bgLinear: 'from-sky-500/5 to-transparent',
    emptyText: 'No recently captured items.',
  },
  already_understood: {
    title: 'Already Understood',
    description: 'Items whose contextual relationships have been analyzed and processed',
    icon: CheckCircle2,
    accentColor: 'text-slate-500',
    badgeColor: 'bg-slate-100 text-slate-700 border-slate-200',
    bgLinear: 'from-slate-500/5 to-transparent',
    emptyText: 'No items in understood state.',
  },
};

export function InboxSection({
  sectionKey,
  items,
  workspaces,
  onMoveToWorkspace,
}: InboxSectionProps) {
  const { collapsedSections, toggleSection, acceptAllHighConfidenceSuggestions, isBatchProcessing } =
    useInboxStore();
  const isCollapsed = collapsedSections[sectionKey] ?? false;
  const config = SECTION_CONFIG[sectionKey];
  const IconComponent = config.icon;

  const handleBatchAccept = async (e: MouseEvent) => {
    e.stopPropagation();
    const rawItems = items.map((d) => d.item);
    await acceptAllHighConfidenceSuggestions(rawItems);
    // Reload items in workspaceStore
    window.dispatchEvent(new CustomEvent('contextsplit://refresh-inbox'));
  };

  return (
    <section className="flex flex-col gap-3">
      {/* Section Header Accordion Bar */}
      <div
        onClick={() => toggleSection(sectionKey)}
        className="flex items-center justify-between px-4 py-2.5 bg-white hover:bg-gray-50/80 border border-black/5 rounded-xl cursor-pointer transition-colors shadow-2xs select-none"
      >
        <div className="flex items-center gap-3 min-w-0">
          <button className="text-gray-400 hover:text-gray-700 transition-colors">
            {isCollapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
          </button>

          <div className="flex items-center gap-2">
            <IconComponent size={16} className={config.accentColor} />
            <h2 className="text-[13px] font-semibold text-gray-800 tracking-tight">
              {config.title}
            </h2>
          </div>

          <span
            className={`text-[11px] font-bold px-2 py-0.5 rounded-full border ${config.badgeColor}`}
          >
            {items.length}
          </span>

          <span className="text-[12px] text-gray-400 truncate hidden md:inline">
            {config.description}
          </span>
        </div>

        {/* Section Header Actions */}
        <div className="flex items-center gap-2">
          {sectionKey === 'context_suggestions' && items.length > 1 && (
            <button
              onClick={handleBatchAccept}
              disabled={isBatchProcessing}
              className="flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium text-indigo-700 bg-indigo-50 hover:bg-indigo-100 border border-indigo-200 rounded-lg transition-colors cursor-pointer"
              title="Accept all high-confidence suggestions"
            >
              <CheckCheck size={13} />
              <span>Accept All</span>
            </button>
          )}
        </div>
      </div>

      {/* Collapsible Content Area */}
      {!isCollapsed && (
        <div className="pt-1 pb-3">
          {items.length === 0 ? (
            <div className="px-6 py-8 border border-dashed border-black/5 rounded-xl text-center bg-gray-50/30">
              <p className="text-[12px] text-gray-400">{config.emptyText}</p>
            </div>
          ) : (
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3.5">
              {items.map((derived) => (
                <InboxItemCard
                  key={derived.item.id}
                  derived={derived}
                  workspaces={workspaces}
                  onMoveToWorkspace={onMoveToWorkspace}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}
