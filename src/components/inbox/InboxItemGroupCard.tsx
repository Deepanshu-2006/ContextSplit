import { useState } from 'react';
import {
  Layers,
  Sparkles,
  ChevronDown,
  ChevronRight,
  FolderPlus,
  CheckCheck,
  XCircle,
  Lightbulb,
} from 'lucide-react';
import { Workspace } from '../../services/db';
import { DerivedInboxItem } from '../../store/inboxStore';
import { useDiscoveryStore } from '../../store/discoveryStore';
import { InboxItemCard } from './InboxItemCard';
import { QuickContextAssignmentMenu } from './QuickContextAssignmentMenu';

export interface InboxItemGroup {
  id: string;
  type: 'candidate' | 'workspace_suggestion' | 'relationship_cluster';
  name: string;
  itemCount: number;
  confidence: number;
  strongestExplanation: string;
  items: DerivedInboxItem[];
  candidateId?: string;
  suggestedWorkspaceId?: string;
}

interface InboxItemGroupCardProps {
  group: InboxItemGroup;
  workspaces: Workspace[];
  onMoveToWorkspace: (itemId: string, workspaceId: string) => void;
  onBulkAssignGroup?: (itemIds: string[], workspaceId: string) => Promise<void>;
  onBulkDismissGroup?: (itemIds: string[], workspaceId?: string) => Promise<void>;
}

export function InboxItemGroupCard({
  group,
  workspaces,
  onMoveToWorkspace,
  onBulkAssignGroup,
  onBulkDismissGroup,
}: InboxItemGroupCardProps) {
  const [isExpanded, setIsExpanded] = useState(true);
  const [assignmentMenuOpen, setAssignmentMenuOpen] = useState(false);
  const [assignmentMenuPos, setAssignmentMenuPos] = useState<{ top: number; left?: number } | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);

  const { reviewCandidate } = useDiscoveryStore();

  const handleReview = () => {
    if (group.candidateId) {
      reviewCandidate(group.candidateId);
    }
  };

  const openAssignmentMenu = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    const rect = e.currentTarget.getBoundingClientRect();
    setAssignmentMenuPos({ top: rect.bottom + 6, left: rect.left });
    setAssignmentMenuOpen(true);
  };

  const handleAddAllToSuggested = async () => {
    if (!group.suggestedWorkspaceId || !onBulkAssignGroup) return;
    try {
      setIsProcessing(true);
      const itemIds = group.items.map((d) => d.item.id);
      await onBulkAssignGroup(itemIds, group.suggestedWorkspaceId);
    } catch (err) {
      console.error('Failed to add group to workspace:', err);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleDismissGroup = async () => {
    if (!onBulkDismissGroup) return;
    try {
      setIsProcessing(true);
      const itemIds = group.items.map((d) => d.item.id);
      await onBulkDismissGroup(itemIds, group.suggestedWorkspaceId);
    } catch (err) {
      console.error('Failed to dismiss group:', err);
    } finally {
      setIsProcessing(false);
    }
  };

  const isCandidate = group.type === 'candidate';
  const confidencePercent = Math.round(group.confidence * 100);

  return (
    <div
      className={`rounded-2xl border transition-all duration-200 overflow-hidden shadow-xs hover:shadow-md ${
        isCandidate
          ? 'border-cyan-200/90 bg-gradient-to-b from-cyan-50/30 via-white to-white'
          : 'border-indigo-200/90 bg-gradient-to-b from-indigo-50/25 via-white to-white'
      }`}
    >
      {/* Group Header Banner */}
      <div className="p-4 border-b border-black/4 flex flex-col gap-3">
        <div className="flex items-center justify-between flex-wrap gap-2">
          {/* Left: Title, Badges & Collapse Toggle */}
          <div className="flex items-center gap-2.5 min-w-0">
            <button
              onClick={() => setIsExpanded((v) => !v)}
              className="p-1 text-gray-400 hover:text-gray-700 hover:bg-black/5 rounded-lg transition-colors cursor-pointer"
              title={isExpanded ? 'Collapse group' : 'Expand group'}
            >
              {isExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
            </button>

            <div
              className={`w-7 h-7 rounded-lg flex items-center justify-center shrink-0 ${
                isCandidate ? 'bg-cyan-100 text-cyan-700' : 'bg-indigo-100 text-indigo-700'
              }`}
            >
              {isCandidate ? <Sparkles size={15} /> : <Layers size={15} />}
            </div>

            <div className="flex items-center gap-2 min-w-0">
              <h3 className="text-sm font-semibold text-gray-900 tracking-tight truncate max-w-xs sm:max-w-md">
                {group.name}
              </h3>

              <span
                className={`text-[11px] font-bold px-2 py-0.5 rounded-full border shrink-0 ${
                  isCandidate
                    ? 'bg-cyan-100/90 text-cyan-900 border-cyan-200'
                    : 'bg-indigo-100/90 text-indigo-900 border-indigo-200'
                }`}
              >
                {group.itemCount} items
              </span>

              <span className="text-[11px] font-semibold px-2 py-0.5 rounded-full bg-emerald-50 text-emerald-700 border border-emerald-200/80 shrink-0">
                {confidencePercent}% Match
              </span>
            </div>
          </div>

          {/* Right: Group Actions */}
          <div className="flex items-center gap-2">
            {isCandidate ? (
              <>
                <button
                  onClick={handleReview}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-cyan-600 hover:bg-cyan-700 text-white rounded-lg text-[11px] font-semibold shadow-2xs transition-colors cursor-pointer"
                  title="Review context candidate"
                >
                  <Sparkles size={13} />
                  <span>Review Context</span>
                </button>

                <button
                  onClick={openAssignmentMenu}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-white hover:bg-gray-50 text-gray-700 border border-black/10 rounded-lg text-[11px] font-semibold shadow-2xs transition-colors cursor-pointer"
                  title="Assign group items to a workspace"
                >
                  <FolderPlus size={13} className="text-indigo-600" />
                  <span>Add All to Workspace</span>
                </button>
              </>
            ) : group.suggestedWorkspaceId ? (
              <>
                <button
                  onClick={handleAddAllToSuggested}
                  disabled={isProcessing}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-indigo-600 hover:bg-indigo-700 text-white rounded-lg text-[11px] font-semibold shadow-2xs transition-colors cursor-pointer disabled:opacity-50"
                  title={`Add all items to ${group.name}`}
                >
                  <CheckCheck size={13} />
                  <span>Add All to Context</span>
                </button>

                <button
                  onClick={handleDismissGroup}
                  disabled={isProcessing}
                  className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-black/5 rounded-lg border border-black/5 transition-colors cursor-pointer disabled:opacity-50"
                  title="Dismiss suggestions for this group"
                >
                  <XCircle size={14} />
                </button>
              </>
            ) : null}
          </div>
        </div>

        {/* Strongest Contextual Explanation Callout */}
        {group.strongestExplanation && (
          <div className="flex items-start gap-2 px-3 py-2 bg-white/80 rounded-xl border border-black/4 text-[12px] text-gray-600">
            <Lightbulb size={14} className="text-amber-500 shrink-0 mt-0.5" />
            <p className="leading-relaxed">{group.strongestExplanation}</p>
          </div>
        )}
      </div>

      {/* Member Representative Items Grid */}
      {isExpanded && (
        <div className="p-4 bg-gray-50/40">
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3.5">
            {group.items.map((derived) => (
              <InboxItemCard
                key={derived.item.id}
                derived={derived}
                workspaces={workspaces}
                onMoveToWorkspace={onMoveToWorkspace}
              />
            ))}
          </div>
        </div>
      )}

      {/* Quick Context Assignment Menu for Group */}
      {assignmentMenuOpen && assignmentMenuPos && (
        <QuickContextAssignmentMenu
          itemId={group.items[0]?.item.id || ''}
          suggestedWorkspaceId={group.suggestedWorkspaceId}
          suggestedConfidence={group.confidence}
          workspaces={workspaces}
          isOpen={assignmentMenuOpen}
          position={assignmentMenuPos}
          onClose={() => setAssignmentMenuOpen(false)}
          onAssignSuccess={async (wsId) => {
            if (onBulkAssignGroup) {
              const itemIds = group.items.map((d) => d.item.id);
              await onBulkAssignGroup(itemIds, wsId);
            }
          }}
        />
      )}
    </div>
  );
}
