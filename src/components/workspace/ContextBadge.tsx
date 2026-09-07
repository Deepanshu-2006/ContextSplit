import { useState, useEffect, MouseEvent } from 'react';
import {
  CheckCircle2,
  Sparkles,
  Layers,
  AlertTriangle,
  HelpCircle,
  ChevronRight,
  Brain,
} from 'lucide-react';
import { db, ItemAttention, InboxItemState, Workspace } from '../../services/db';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { useInboxStore } from '../../store/inboxStore';
import { ContextInspector } from './ContextInspector';

export type ContextBadgeState =
  | 'confirmed'
  | 'understood'
  | 'suggested'
  | 'candidate'
  | 'uncertain'
  | 'no_context';

export interface ContextBadgeProps {
  itemId: string;
  workspaceId?: string | null;
  attention?: ItemAttention;
  inboxState?: InboxItemState;
  candidateInfo?: {
    candidateId: string;
    candidateName: string;
    confidence: number;
  };
  workspaces?: Workspace[];
  className?: string;
  onOpenInspector?: () => void;
}

export function getContextBadgeData(
  workspaceId?: string | null,
  attention?: ItemAttention,
  inboxState?: InboxItemState,
  candidateInfo?: { candidateId: string; candidateName: string; confidence: number },
  workspaces: Workspace[] = []
): {
  state: ContextBadgeState;
  label: string;
  tooltip: string;
  icon: typeof CheckCircle2 | typeof Brain;
  classes: {
    badge: string;
    icon: string;
    dot: string;
  };
} {
  const isPhysicallyInWorkspace = workspaceId != null && workspaceId !== 'inbox' && workspaceId !== '';
  const isOrganized = isPhysicallyInWorkspace || inboxState?.state === 'organized' || attention?.factors?.is_organized;

  // 1. CONFIRMED CONTEXT (Explicit user confirmation or manual filing)
  if (isOrganized) {
    const ws = workspaces.find((w) => w.id === workspaceId);
    const wsName = ws?.name || 'Workspace';
    return {
      state: 'confirmed',
      label: `${wsName} • Confirmed`,
      tooltip: `Confirmed membership in ${wsName}. User explicitly accepted organization.`,
      icon: CheckCircle2,
      classes: {
        badge:
          'bg-emerald-50 text-emerald-800 border-emerald-200/90 hover:bg-emerald-100 hover:border-emerald-300',
        icon: 'text-emerald-600',
        dot: 'bg-emerald-500',
      },
    };
  }

  // 2. UNCERTAIN / AMBIGUOUS
  // If ambiguous between multiple contexts or marked for review
  if (
    attention?.factors?.is_ambiguous ||
    inboxState?.state === 'needs_review'
  ) {
    return {
      state: 'uncertain',
      label: 'Ambiguous • Needs Review',
      tooltip:
        attention?.reason ||
        'Competing affinities across multiple contexts. Click to inspect ambiguity.',
      icon: AlertTriangle,
      classes: {
        badge:
          'bg-amber-50 text-amber-900 border-amber-200/90 hover:bg-amber-100 hover:border-amber-300',
        icon: 'text-amber-600',
        dot: 'bg-amber-500 animate-pulse',
      },
    };
  }

  // 3. UNDERSTOOD CONTEXT (High-confidence interpretation by ContextSplit, still unassigned in Inbox)
  const confidence = attention?.factors?.contextual_confidence ?? 0;
  const isStateUnderstood = inboxState?.state === 'understood';
  const hasHighConfidenceSuggestion =
    (attention?.factors?.has_suggested_membership || attention?.factors?.suggested_workspace_id) &&
    confidence >= 0.70;

  if (isStateUnderstood || (hasHighConfidenceSuggestion && !attention?.factors?.is_ambiguous)) {
    const targetWsId = attention?.factors?.suggested_workspace_id;
    const ws = workspaces.find((w) => w.id === targetWsId);
    const wsName = attention?.factors?.suggested_workspace_name || ws?.name || (candidateInfo ? candidateInfo.candidateName : 'Context');

    return {
      state: 'understood',
      label: `${wsName} • Understood`,
      tooltip: `Understood by ContextSplit: High-confidence contextual interpretation (${wsName}). Unassigned in Inbox until confirmed.`,
      icon: Brain,
      classes: {
        badge:
          'bg-teal-50 text-teal-900 border-teal-200/90 hover:bg-teal-100 hover:border-teal-300',
        icon: 'text-teal-600',
        dot: 'bg-teal-500',
      },
    };
  }

  // 4. NEW CONTEXT CANDIDATE
  // If item belongs to a detected/suggested context candidate cluster
  if (
    candidateInfo != null ||
    attention?.factors?.has_new_candidate
  ) {
    const candName = candidateInfo?.candidateName || 'New Context';
    return {
      state: 'candidate',
      label: `${candName} • Candidate`,
      tooltip: `Member of candidate cluster "${candName}". Click to inspect evidence.`,
      icon: Layers,
      classes: {
        badge:
          'bg-cyan-50 text-cyan-900 border-cyan-200/90 hover:bg-cyan-100 hover:border-cyan-300',
        icon: 'text-cyan-600',
        dot: 'bg-cyan-500',
      },
    };
  }

  // 5. SUGGESTED CONTEXT (Emerging / Moderate recommendation)
  if (
    attention?.factors?.has_suggested_membership ||
    (attention?.factors?.suggested_workspace_id != null && attention.factors.suggested_workspace_id !== '') ||
    inboxState?.state === 'suggested'
  ) {
    const targetWsId = attention?.factors?.suggested_workspace_id;
    const ws = workspaces.find((w) => w.id === targetWsId);
    const wsName = attention?.factors?.suggested_workspace_name || ws?.name || 'Context';

    return {
      state: 'suggested',
      label: `${wsName} • Suggested`,
      tooltip: `Suggested for ${wsName}. Click to inspect explanation or assign.`,
      icon: Sparkles,
      classes: {
        badge:
          'bg-indigo-50 text-indigo-900 border-indigo-200/90 hover:bg-indigo-100 hover:border-indigo-300',
        icon: 'text-indigo-600',
        dot: 'bg-indigo-500',
      },
    };
  }

  // 6. NO CONTEXT
  // Default fallback for items without strong evidence
  return {
    state: 'no_context',
    label: 'No strong context',
    tooltip: 'Insufficient contextual evidence. Click to inspect context evaluation.',
    icon: HelpCircle,
    classes: {
      badge:
        'bg-gray-50/90 text-gray-600 border-gray-200/70 hover:bg-gray-100 hover:text-gray-800 hover:border-gray-300',
      icon: 'text-gray-400',
      dot: 'bg-gray-400',
    },
  };
}

export function ContextBadge({
  itemId,
  workspaceId,
  attention: propAttention,
  inboxState: propInboxState,
  candidateInfo: propCandidateInfo,
  workspaces: propWorkspaces,
  className = '',
  onOpenInspector,
}: ContextBadgeProps) {
  const storeWorkspaces = useWorkspaceStore((s) => s.workspaces);
  const workspaces = propWorkspaces || storeWorkspaces;

  const storeAttention = useInboxStore((s) => s.attentionMap[itemId]);
  const storeState = useInboxStore((s) => s.inboxStateMap[itemId]);
  const storeCandidate = useInboxStore((s) => s.candidateItemMap[itemId]);

  const [localAttention, setLocalAttention] = useState<ItemAttention | undefined>(
    propAttention || storeAttention
  );
  const [localState, setLocalState] = useState<InboxItemState | undefined>(
    propInboxState || storeState
  );
  const [localCandidate] = useState(
    propCandidateInfo || storeCandidate
  );
  const [inspectorOpen, setInspectorOpen] = useState(false);

  useEffect(() => {
    if (propAttention) setLocalAttention(propAttention);
    else if (storeAttention) setLocalAttention(storeAttention);
    else {
      db.evaluateInboxItemAttention(itemId)
        .then((att) => setLocalAttention(att))
        .catch(() => {});
    }
  }, [itemId, propAttention, storeAttention]);

  useEffect(() => {
    if (propInboxState) setLocalState(propInboxState);
    else if (storeState) setLocalState(storeState);
    else {
      db.getInboxItemState(itemId)
        .then((st) => setLocalState(st || undefined))
        .catch(() => {});
    }
  }, [itemId, propInboxState, storeState]);

  const badgeData = getContextBadgeData(
    workspaceId,
    localAttention,
    localState,
    localCandidate,
    workspaces
  );

  const Icon = badgeData.icon;

  const handleClick = (e: MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
    if (onOpenInspector) {
      onOpenInspector();
    } else {
      setInspectorOpen(true);
    }
  };

  return (
    <>
      <button
        onClick={handleClick}
        title={badgeData.tooltip}
        className={`group/badge inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-semibold border transition-all duration-150 cursor-pointer shadow-2xs select-none max-w-full truncate ${badgeData.classes.badge} ${className}`}
      >
        <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${badgeData.classes.dot}`} />
        <Icon size={12} className={`shrink-0 ${badgeData.classes.icon}`} />
        <span className="truncate">{badgeData.label}</span>
        <ChevronRight
          size={11}
          className="opacity-0 -ml-0.5 group-hover/badge:opacity-100 transition-opacity shrink-0"
        />
      </button>

      {inspectorOpen && (
        <ContextInspector itemId={itemId} onClose={() => setInspectorOpen(false)} />
      )}
    </>
  );
}
