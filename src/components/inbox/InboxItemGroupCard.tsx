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
  AlertTriangle,
  CheckCircle2,
  HelpCircle,
  RefreshCw,
  Quote,
} from 'lucide-react';
import { Workspace } from '../../services/db';
import { DerivedInboxItem, useInboxStore } from '../../store/inboxStore';
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
  const [showAiReasoning, setShowAiReasoning] = useState(false);
  const [assignmentMenuOpen, setAssignmentMenuOpen] = useState(false);
  const [assignmentMenuPos, setAssignmentMenuPos] = useState<{ top: number; left?: number } | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);

  const { reviewCandidate } = useDiscoveryStore();
  const { groupReasoningMap, fetchGroupReasoning } = useInboxStore();

  const groupAiState = groupReasoningMap[group.id];
  const isHighConfidence = group.confidence >= 0.70;
  const confidencePercent = Math.round(group.confidence * 100);
  const isCandidate = group.type === 'candidate';

  const handleToggleAiReasoning = async () => {
    const nextState = !showAiReasoning;
    setShowAiReasoning(nextState);
    if (nextState && !groupAiState?.hasRun) {
      await fetchGroupReasoning(group.id, {
        candidateId: group.candidateId,
        itemIds: group.items.map((d) => d.item.id),
        confidence: group.confidence,
      });
    }
  };

  const handleRefreshAiReasoning = async (e: React.MouseEvent) => {
    e.stopPropagation();
    await fetchGroupReasoning(
      group.id,
      {
        candidateId: group.candidateId,
        itemIds: group.items.map((d) => d.item.id),
        confidence: group.confidence,
      },
      true // explicit user re-run
    );
  };

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

  const aiResult = groupAiState?.result;
  const isAiLoading = groupAiState?.isLoading ?? false;
  const isAiUnavailable = groupAiState?.isUnavailable ?? false;

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
            {/* AI Insights Toggle Button for High Confidence Groups */}
            {isHighConfidence && (
              <button
                onClick={handleToggleAiReasoning}
                className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-semibold border transition-all cursor-pointer ${
                  showAiReasoning
                    ? 'bg-purple-600 text-white border-purple-700 shadow-2xs'
                    : 'bg-purple-50 hover:bg-purple-100 text-purple-700 border-purple-200/80'
                }`}
                title={showAiReasoning ? 'Hide AI reasoning explanation' : 'View AI context reasoning'}
              >
                <Sparkles size={12} className={isAiLoading ? 'animate-spin' : ''} />
                <span>{showAiReasoning ? 'AI Explanation' : 'Explain with AI'}</span>
              </button>
            )}

            {isCandidate ? (
              <>
                <button
                  onClick={handleReview}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-cyan-600 hover:bg-cyan-700 text-white rounded-lg text-[11px] font-semibold shadow-2xs transition-colors cursor-pointer group/btn"
                  title="Review context candidate (R)"
                >
                  <Sparkles size={13} />
                  <span>Review Context</span>
                  <kbd className="text-[9px] font-mono text-cyan-200 bg-cyan-800/50 px-1 py-0.2 rounded border border-cyan-400/30 opacity-80 group-hover/btn:opacity-100">
                    R
                  </kbd>
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
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-indigo-600 hover:bg-indigo-700 text-white rounded-lg text-[11px] font-semibold shadow-2xs transition-colors cursor-pointer disabled:opacity-50 group/btn"
                  title={`Add all items to ${group.name} (A)`}
                >
                  <CheckCheck size={13} />
                  <span>Add All to Context</span>
                  <kbd className="text-[9px] font-mono text-indigo-200 bg-indigo-800/50 px-1 py-0.2 rounded border border-indigo-400/30 opacity-80 group-hover/btn:opacity-100">
                    A
                  </kbd>
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

        {/* AI Context Reasoning Card (When Toggled / Expanded) */}
        {showAiReasoning && (
          <div className="rounded-xl border border-purple-200/90 bg-purple-50/40 overflow-hidden transition-all">
            {/* Header */}
            <div className="flex items-center justify-between px-3.5 py-2 bg-purple-100/60 border-b border-purple-200/80">
              <div className="flex items-center gap-2">
                <Sparkles size={13} className="text-purple-600 shrink-0" />
                <span className="text-[11px] font-semibold text-purple-900 uppercase tracking-wider">
                  AI Context Explanation
                </span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-[10px] text-purple-600/80 font-medium hidden sm:inline">
                  Grounded in local capture evidence
                </span>
                <button
                  onClick={handleRefreshAiReasoning}
                  disabled={isAiLoading}
                  className="p-1 text-purple-600 hover:text-purple-800 hover:bg-purple-200/60 rounded transition-colors cursor-pointer disabled:opacity-50"
                  title="Re-run AI context reasoning"
                >
                  <RefreshCw size={11} className={isAiLoading ? 'animate-spin' : ''} />
                </button>
              </div>
            </div>

            {/* AI Body */}
            <div className="p-3.5 space-y-3">
              {isAiLoading ? (
                <div className="flex items-center gap-2.5 py-2 text-[12px] text-purple-700">
                  <RefreshCw size={14} className="animate-spin text-purple-600 shrink-0" />
                  <span>Analyzing contextual evidence and relationship structure...</span>
                </div>
              ) : aiResult ? (
                <>
                  {/* Concise AI Explanation */}
                  <div className="flex items-start gap-2.5">
                    <Quote size={13} className="text-purple-400 shrink-0 mt-0.5" />
                    <p className="text-[12.5px] text-gray-800 leading-relaxed font-normal">
                      {aiResult.explanation}
                    </p>
                  </div>

                  {/* Uncertainty Note (if applicable) */}
                  {aiResult.uncertainty_reason && (
                    <div className="flex items-start gap-2 px-2.5 py-2 rounded-lg bg-amber-50 border border-amber-200/80 text-[11.5px] text-amber-900">
                      <AlertTriangle size={13} className="text-amber-600 shrink-0 mt-0.5" />
                      <p className="leading-snug">
                        <span className="font-semibold">Uncertainty Note: </span>
                        {aiResult.uncertainty_reason}
                      </p>
                    </div>
                  )}

                  {/* Supporting Evidence References */}
                  {aiResult.evidence_references && aiResult.evidence_references.length > 0 && (
                    <div className="pt-1">
                      <p className="text-[10.5px] font-semibold text-gray-400 uppercase tracking-wider mb-1.5">
                        Supporting Evidence
                      </p>
                      <div className="space-y-1">
                        {aiResult.evidence_references.map((ref, idx) => (
                          <div key={idx} className="flex items-start gap-2 text-[11.5px] text-gray-700">
                            <CheckCircle2 size={12} className="text-purple-500 shrink-0 mt-0.5" />
                            <span className="leading-snug">{ref}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Alternative Interpretations (if any) */}
                  {aiResult.alternative_interpretations && aiResult.alternative_interpretations.length > 0 && (
                    <div className="border-t border-purple-200/60 pt-2">
                      <p className="text-[10.5px] font-semibold text-gray-400 uppercase tracking-wider mb-1">
                        Alternative Possibilities
                      </p>
                      <div className="space-y-1">
                        {aiResult.alternative_interpretations.map((alt, idx) => (
                          <div
                            key={idx}
                            className="flex items-center gap-1.5 text-[11px] text-gray-600 bg-white/70 px-2 py-1 rounded border border-purple-100"
                          >
                            <HelpCircle size={11} className="text-gray-400 shrink-0" />
                            <span className="font-medium text-gray-800">{alt.context_name}</span>
                            {alt.rationale && <span className="text-gray-400">— {alt.rationale}</span>}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </>
              ) : isAiUnavailable ? (
                /* Fallback to Deterministic Context Explanation */
                <div className="space-y-2">
                  <div className="flex items-start gap-2 text-[12px] text-gray-700">
                    <Lightbulb size={13} className="text-amber-500 shrink-0 mt-0.5" />
                    <div>
                      <span className="font-medium text-gray-800">Deterministic Context Assessment: </span>
                      <span className="text-gray-600">{group.strongestExplanation || 'Items strongly correlated through contextual signals and co-occurrence.'}</span>
                    </div>
                  </div>
                  <p className="text-[10.5px] text-gray-400 italic">
                    AI reasoning is currently unavailable or disabled; falling back to deterministic explanation.
                  </p>
                </div>
              ) : null}
            </div>
          </div>
        )}

        {/* Deterministic Strongest Explanation Callout (Default when AI is not open) */}
        {!showAiReasoning && group.strongestExplanation && (
          <div className="flex items-start justify-between gap-2 px-3 py-2 bg-white/80 rounded-xl border border-black/4 text-[12px] text-gray-600">
            <div className="flex items-start gap-2">
              <Lightbulb size={14} className="text-amber-500 shrink-0 mt-0.5" />
              <p className="leading-relaxed">{group.strongestExplanation}</p>
            </div>
            {isHighConfidence && (
              <button
                onClick={handleToggleAiReasoning}
                className="text-[11px] font-medium text-purple-700 hover:text-purple-900 shrink-0 underline decoration-purple-300 underline-offset-2 cursor-pointer ml-2"
              >
                Explain with AI
              </button>
            )}
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

