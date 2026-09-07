import { MouseEvent, ComponentType, useMemo } from 'react';
import {
  AlertTriangle,
  Sparkles,
  Layers,
  Clock,
  ChevronDown,
  ChevronRight,
  CheckCheck,
  Brain,
} from 'lucide-react';
import { Workspace } from '../../services/db';
import { DerivedInboxItem, InboxSectionKey, useInboxStore } from '../../store/inboxStore';
import { InboxItemCard } from './InboxItemCard';
import { InboxItemGroupCard, InboxItemGroup } from './InboxItemGroupCard';

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
    title: 'Understood by ContextSplit',
    description: 'High-confidence contextual interpretation reached. Items remain in your Inbox until confirmed.',
    icon: Brain,
    accentColor: 'text-teal-600',
    badgeColor: 'bg-teal-50 text-teal-800 border-teal-200',
    bgLinear: 'from-teal-500/5 to-transparent',
    emptyText: 'No items currently in understood state.',
  },
};

export function InboxSection({
  sectionKey,
  items,
  workspaces,
  onMoveToWorkspace,
}: InboxSectionProps) {
  const {
    collapsedSections,
    toggleSection,
    acceptAllHighConfidenceSuggestions,
    assignItemIdsToWorkspace,
    dismissItemIdsSuggestions,
    isBatchProcessing,
  } = useInboxStore();
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

  // Derive visual contextual clusters without duplicating any item records
  const { groups, standaloneItems } = useMemo(() => {
    if (sectionKey === 'new_candidates') {
      const candidateMap: Record<string, DerivedInboxItem[]> = {};
      const standalone: DerivedInboxItem[] = [];

      for (const item of items) {
        const candId = item.candidateInfo?.candidateId;
        if (candId) {
          if (!candidateMap[candId]) candidateMap[candId] = [];
          candidateMap[candId].push(item);
        } else {
          standalone.push(item);
        }
      }

      const derivedGroups: InboxItemGroup[] = Object.entries(candidateMap).map(
        ([candId, memberItems]) => {
          const first = memberItems[0];
          return {
            id: `cand-${candId}`,
            type: 'candidate',
            name: first.candidateInfo?.candidateName || 'Discovered Context Candidate',
            itemCount: memberItems.length,
            confidence: first.candidateInfo?.confidence ?? 0.85,
            strongestExplanation:
              first.candidateInfo?.explanation ||
              'Items discovered together with strong contextual and semantic correlation.',
            items: memberItems,
            candidateId: candId,
          };
        }
      );

      return { groups: derivedGroups, standaloneItems: standalone };
    }

    if (sectionKey === 'context_suggestions') {
      const wsMap: Record<string, DerivedInboxItem[]> = {};
      const standalone: DerivedInboxItem[] = [];

      for (const item of items) {
        const wsId = item.attention?.factors?.suggested_workspace_id;
        if (wsId) {
          if (!wsMap[wsId]) wsMap[wsId] = [];
          wsMap[wsId].push(item);
        } else {
          standalone.push(item);
        }
      }

      const derivedGroups: InboxItemGroup[] = [];
      for (const [wsId, memberItems] of Object.entries(wsMap)) {
        if (memberItems.length >= 2) {
          const ws = workspaces.find((w) => w.id === wsId);
          const maxConf = Math.max(
            ...memberItems.map((m) => m.attention?.factors?.contextual_confidence ?? 0.8)
          );
          const bestExplanation =
            memberItems.find((m) => m.attention?.reason)?.attention?.reason ||
            `Multiple items have strong affinity with "${ws?.name || 'Workspace'}".`;

          derivedGroups.push({
            id: `ws-${wsId}`,
            type: 'workspace_suggestion',
            name: ws?.name || 'Suggested Workspace',
            itemCount: memberItems.length,
            confidence: maxConf,
            strongestExplanation: bestExplanation,
            items: memberItems,
            suggestedWorkspaceId: wsId,
          });
        } else {
          standalone.push(...memberItems);
        }
      }

      return { groups: derivedGroups, standaloneItems: standalone };
    }

    return { groups: [], standaloneItems: items };
  }, [sectionKey, items, workspaces]);

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
            <div className="flex flex-col gap-4">
              {/* Grouped Cluster Cards */}
              {groups.map((group) => (
                <InboxItemGroupCard
                  key={group.id}
                  group={group}
                  workspaces={workspaces}
                  onMoveToWorkspace={onMoveToWorkspace}
                  onBulkAssignGroup={assignItemIdsToWorkspace}
                  onBulkDismissGroup={dismissItemIdsSuggestions}
                />
              ))}

              {/* Standalone Individual Cards Grid */}
              {standaloneItems.length > 0 && (
                <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3.5">
                  {standaloneItems.map((derived) => (
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
        </div>
      )}
    </section>
  );
}

