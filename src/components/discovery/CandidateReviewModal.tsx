import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  Sparkles,
  Layers,
  Edit2,
  Check,
  FileText,
  Link2,
  Image,
  FileCode,
  ShieldCheck,
  CheckCircle2,
  Trash2,
  FolderPlus,
  ArrowRight,
  AlertCircle,
  HelpCircle,
  MessageSquare,
  Quote,
  AlertTriangle,
  Loader2,
  Info,
} from 'lucide-react';
import { useDiscoveryStore } from '../../store/discoveryStore';
import { ContextReasoningResult } from '../../services/db';

// ─── Confidence helpers ────────────────────────────────────────────────────────

function getConfidenceMeta(confidence: number) {
  if (confidence >= 0.75) {
    return {
      label: 'High Confidence',
      color: 'text-emerald-700',
      bg: 'bg-emerald-50',
      border: 'border-emerald-200',
      dot: 'bg-emerald-500',
      bar: 'bg-emerald-500',
    };
  } else if (confidence >= 0.50) {
    return {
      label: 'Moderate Confidence',
      color: 'text-amber-700',
      bg: 'bg-amber-50',
      border: 'border-amber-200',
      dot: 'bg-amber-500',
      bar: 'bg-amber-500',
    };
  } else if (confidence >= 0.25) {
    return {
      label: 'Uncertain',
      color: 'text-orange-700',
      bg: 'bg-orange-50',
      border: 'border-orange-200',
      dot: 'bg-orange-400',
      bar: 'bg-orange-400',
    };
  } else {
    return {
      label: 'Low Evidence',
      color: 'text-gray-600',
      bg: 'bg-gray-50',
      border: 'border-gray-200',
      dot: 'bg-gray-400',
      bar: 'bg-gray-400',
    };
  }
}

function getDecisionLabel(decision: ContextReasoningResult['decision']) {
  switch (decision) {
    case 'existing_context': return 'Matches Existing Context';
    case 'new_context': return 'Suggests New Context';
    case 'uncertain': return 'Uncertain';
    case 'no_context': return 'No Clear Context';
  }
}

// ─── Sub-components ────────────────────────────────────────────────────────────

function AIExplanationSection({ result }: { result: ContextReasoningResult }) {
  const meta = getConfidenceMeta(result.confidence);
  const pct = Math.round(result.confidence * 100);

  return (
    <div className="rounded-xl border border-purple-200 bg-purple-50/40 overflow-hidden">
      {/* Section header — clearly labels this as AI-generated */}
      <div className="flex items-center gap-2 px-4 py-2.5 bg-purple-100/60 border-b border-purple-200">
        <Sparkles size={13} className="text-purple-600 shrink-0" />
        <span className="text-[11px] font-semibold text-purple-800 uppercase tracking-wider">
          AI-Generated Analysis
        </span>
        <span className="ml-auto text-[10.5px] text-purple-500 font-medium italic">
          Based on your captured evidence only
        </span>
      </div>

      <div className="p-4 space-y-3.5">
        {/* Decision + Confidence row */}
        <div className="flex items-center gap-3 flex-wrap">
          <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11.5px] font-semibold border ${meta.bg} ${meta.border} ${meta.color}`}>
            <span className={`w-1.5 h-1.5 rounded-full ${meta.dot}`} />
            {meta.label}
          </span>
          <span className="text-[11.5px] text-gray-500">
            {getDecisionLabel(result.decision)}
          </span>
          {/* Confidence bar */}
          <div className="flex items-center gap-2 ml-auto">
            <div className="w-20 h-1.5 bg-gray-200 rounded-full overflow-hidden">
              <div
                className={`h-full rounded-full transition-all ${meta.bar}`}
                style={{ width: `${pct}%` }}
              />
            </div>
            <span className={`text-[12px] font-bold tabular-nums ${meta.color}`}>
              {pct}%
            </span>
          </div>
        </div>

        {/* AI Explanation */}
        <div className="flex items-start gap-2.5">
          <Quote size={14} className="text-purple-400 shrink-0 mt-0.5" />
          <p className="text-[13px] text-gray-800 leading-relaxed font-normal">
            {result.explanation}
          </p>
        </div>

        {/* Uncertainty note (if applicable) */}
        {result.uncertainty_reason && (
          <div className="flex items-start gap-2 px-3 py-2 rounded-lg bg-orange-50 border border-orange-200">
            <AlertTriangle size={13} className="text-orange-600 shrink-0 mt-0.5" />
            <p className="text-[12px] text-orange-800 leading-snug">
              <span className="font-semibold">Note: </span>{result.uncertainty_reason}
            </p>
          </div>
        )}

        {/* Supporting evidence references */}
        {result.evidence_references.length > 0 && (
          <div>
            <p className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-2">
              Supporting Evidence
            </p>
            <div className="space-y-1.5">
              {result.evidence_references.map((ref, i) => (
                <div
                  key={i}
                  className="flex items-start gap-2 text-[12px] text-gray-700"
                >
                  <CheckCircle2 size={13} className="text-purple-400 shrink-0 mt-0.5" />
                  <span className="leading-snug">{ref}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Alternative interpretations (if any) */}
        {result.alternative_interpretations && result.alternative_interpretations.length > 0 && (
          <div className="border-t border-purple-200/60 pt-3">
            <p className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-2">
              Alternative Interpretations
            </p>
            <div className="space-y-1.5">
              {result.alternative_interpretations.map((alt, i) => (
                <div
                  key={i}
                  className="flex items-start gap-2 px-2.5 py-1.5 rounded-lg bg-white border border-gray-100 text-[12px] text-gray-600"
                >
                  <HelpCircle size={12} className="text-gray-400 shrink-0 mt-0.5" />
                  <span>
                    <span className="font-medium text-gray-800">{alt.context_name}</span>
                    {alt.rationale ? <span className="text-gray-400 ml-1.5">— {alt.rationale}</span> : null}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function AIReasoningLoadingSection() {
  return (
    <div className="rounded-xl border border-purple-200 bg-purple-50/40 overflow-hidden">
      <div className="flex items-center gap-2 px-4 py-2.5 bg-purple-100/60 border-b border-purple-200">
        <Sparkles size={13} className="text-purple-600 shrink-0" />
        <span className="text-[11px] font-semibold text-purple-800 uppercase tracking-wider">
          AI-Generated Analysis
        </span>
      </div>
      <div className="flex items-center gap-3 px-4 py-5 text-[12.5px] text-gray-500">
        <Loader2 size={15} className="text-purple-500 animate-spin shrink-0" />
        Analyzing evidence for this context…
      </div>
    </div>
  );
}

function DeterministicEvidenceSection({ evidence }: { evidence: string[] }) {
  const formatFriendlyEvidence = (rawEvidence: string): string => {
    const lower = rawEvidence.toLowerCase();
    if (lower.includes('semantically related')) {
      return rawEvidence.replace(/semantically related/gi, 'content-related');
    }
    if (lower.includes('captured in the same session')) {
      return rawEvidence.replace(/captured in the same session/gi, 'worked on during the same focused session');
    }
    if (lower.includes('mutual relationships')) {
      return 'Items frequently connect to the same activity';
    }
    return rawEvidence;
  };

  if (!evidence || evidence.length === 0) return null;

  return (
    <div>
      <div className="flex items-center gap-1.5 mb-2">
        <MessageSquare size={13} className="text-gray-400" />
        <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          Why these items belong together
        </span>
      </div>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
        {evidence.map((ev, idx) => (
          <div
            key={idx}
            className="flex items-start gap-2 text-[12px] text-gray-700 bg-white border border-gray-100 rounded-lg p-2.5 shadow-2xs"
          >
            <CheckCircle2 size={14} className="text-emerald-500 shrink-0 mt-0.5" />
            <span className="leading-snug">{formatFriendlyEvidence(ev)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Main Component ─────────────────────────────────────────────────────────

export function CandidateReviewModal() {
  const {
    isReviewOpen,
    selectedCandidateDetails,
    selectedCandidateScore,
    aiReasoningResult,
    isAiReasoningLoading,
    isLoading,
    closeReview,
    dismissCandidate,
    updateCandidateName,
    suggestCandidateNameAI,
    removeItemFromCandidate,
    createWorkspaceFromCandidate,
  } = useDiscoveryStore();

  const [isEditingName, setIsEditingName] = useState(false);
  const [editedName, setEditedName] = useState('');
  const [isAiSuggesting, setIsAiSuggesting] = useState(false);
  const [showCreateConfirm, setShowCreateConfirm] = useState(false);
  const [isCreatingWorkspace, setIsCreatingWorkspace] = useState(false);

  useEffect(() => {
    if (selectedCandidateDetails?.candidate) {
      setEditedName(selectedCandidateDetails.candidate.label || 'New Context');
    }
  }, [selectedCandidateDetails]);

  if (!isReviewOpen) return null;

  const candidate = selectedCandidateDetails?.candidate;
  const items = selectedCandidateDetails?.items || [];
  const score = selectedCandidateScore;

  const handleSaveName = async () => {
    if (candidate && editedName.trim()) {
      await updateCandidateName(candidate.id, editedName.trim());
      setIsEditingName(false);
    }
  };

  const handleAiSuggestName = async () => {
    if (!candidate) return;
    setIsAiSuggesting(true);
    try {
      const suggested = await suggestCandidateNameAI(candidate.id);
      if (suggested) {
        setEditedName(suggested);
        await updateCandidateName(candidate.id, suggested);
      }
    } finally {
      setIsAiSuggesting(false);
    }
  };

  const handleConfirmCreateWorkspace = async () => {
    if (!candidate) return;
    try {
      setIsCreatingWorkspace(true);
      await createWorkspaceFromCandidate(candidate.id, editedName.trim() || candidate.label || 'New Workspace');
    } finally {
      setIsCreatingWorkspace(false);
      setShowCreateConfirm(false);
    }
  };

  const getItemIcon = (type: string) => {
    switch (type) {
      case 'link':
        return <Link2 size={14} className="text-blue-500" />;
      case 'image':
        return <Image size={14} className="text-purple-500" />;
      case 'file':
        return <FileCode size={14} className="text-amber-500" />;
      default:
        return <FileText size={14} className="text-gray-500" />;
    }
  };

  const confidencePct = Math.round((candidate?.confidence || 0) * 100);
  const confidenceMeta = getConfidenceMeta(candidate?.confidence || 0);

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="candidate-review-title"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs font-sans animate-in fade-in duration-200"
    >
      <div className="relative w-full max-w-2xl max-h-[90vh] bg-white rounded-2xl shadow-2xl border border-gray-200 flex flex-col overflow-hidden animate-in zoom-in-95 duration-200">

        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-100 flex items-center justify-between bg-gray-50/50 shrink-0">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-xl bg-amber-50 border border-amber-200/70 flex items-center justify-center shadow-2xs">
              <Sparkles size={16} className="text-amber-600" />
            </div>
            <div>
              <h2 id="candidate-review-title" className="text-[15px] font-semibold text-gray-900 leading-tight">
                Review Discovered Context
              </h2>
              <p className="text-[12px] text-gray-500">
                Review AI analysis and decide whether to create a workspace
              </p>
            </div>
          </div>
          <button
            onClick={closeReview}
            className="p-1.5 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors cursor-pointer"
            aria-label="Close"
          >
            <X size={16} />
          </button>
        </div>

        {/* Content Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-5">
          {isLoading && !candidate ? (
            <div className="flex items-center justify-center py-16 text-sm text-gray-400">
              Loading context details...
            </div>
          ) : (
            <>
              {/* ── Context Name & Confidence ──────────────────────── */}
              <div className="bg-gray-50/90 border border-gray-200/80 rounded-xl p-4.5">
                <div className="flex items-start justify-between gap-4 mb-0.5">
                  <div className="flex-1 min-w-0">
                    <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block mb-1.5">
                      Context Name
                    </span>
                    {isEditingName ? (
                      <div className="flex items-center gap-2">
                        <input
                          type="text"
                          value={editedName}
                          onChange={(e) => setEditedName(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') handleSaveName();
                            if (e.key === 'Escape') setIsEditingName(false);
                          }}
                          autoFocus
                          placeholder="Enter context name..."
                          className="flex-1 px-3 py-1.5 text-[14px] font-semibold bg-white border border-blue-400 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/20 shadow-2xs"
                        />
                        <button
                          onClick={handleSaveName}
                          className="px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-xs font-medium flex items-center gap-1 cursor-pointer"
                        >
                          <Check size={12} />
                          Save
                        </button>
                        <button
                          onClick={() => setIsEditingName(false)}
                          className="px-2.5 py-1.5 text-gray-500 hover:text-gray-700 text-xs font-medium cursor-pointer"
                        >
                          Cancel
                        </button>
                      </div>
                    ) : (
                      <div className="flex items-center gap-2.5 flex-wrap group">
                        <span className="text-[17px] font-semibold text-gray-900 leading-tight">
                          {candidate?.label || 'New Context'}
                        </span>
                        <button
                          onClick={() => setIsEditingName(true)}
                          className="p-1 text-gray-400 hover:text-gray-600 opacity-60 group-hover:opacity-100 transition-opacity rounded cursor-pointer"
                          title="Rename this context"
                          aria-label="Rename context"
                        >
                          <Edit2 size={13} />
                        </button>
                        <button
                          onClick={handleAiSuggestName}
                          disabled={isAiSuggesting}
                          className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-medium bg-purple-50 text-purple-700 hover:bg-purple-100 border border-purple-200 transition-colors disabled:opacity-50 cursor-pointer"
                          title="Suggest a concise 2–4 word name grounded in evidence"
                        >
                          <Sparkles size={11} className={isAiSuggesting ? 'animate-spin' : 'text-purple-500'} />
                          {isAiSuggesting ? 'Suggesting…' : 'AI Suggest'}
                        </button>
                      </div>
                    )}
                  </div>

                  {/* Confidence badge */}
                  <div className="text-right shrink-0">
                    <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block mb-1.5">
                      Match Confidence
                    </span>
                    <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[12px] font-bold border ${confidenceMeta.bg} ${confidenceMeta.border} ${confidenceMeta.color}`}>
                      <span className={`w-1.5 h-1.5 rounded-full ${confidenceMeta.dot}`} />
                      {confidencePct}%
                    </span>
                    <p className={`text-[10.5px] mt-0.5 font-medium ${confidenceMeta.color}`}>
                      {confidenceMeta.label}
                    </p>
                  </div>
                </div>

                {/* Deterministic evidence (from score) */}
                {score?.evidence && score.evidence.length > 0 && (
                  <div className="border-t border-gray-200/60 pt-3.5 mt-4">
                    <DeterministicEvidenceSection evidence={score.evidence} />
                  </div>
                )}
              </div>

              {/* ── AI-Generated Analysis ──────────────────────────── */}
              {isAiReasoningLoading && <AIReasoningLoadingSection />}
              {!isAiReasoningLoading && aiReasoningResult && (
                <AIExplanationSection result={aiReasoningResult} />
              )}

              {/* ── Included Items ─────────────────────────────────── */}
              <div>
                <div className="flex items-center justify-between mb-2.5">
                  <div className="flex items-center gap-2">
                    <Layers size={14} className="text-gray-500" />
                    <h3 className="text-[13px] font-semibold text-gray-900">
                      Included Items ({items.length})
                    </h3>
                  </div>
                  <span className="text-[11.5px] text-gray-400">
                    Remove items that don't belong
                  </span>
                </div>

                {items.length === 0 ? (
                  <div className="p-8 text-center bg-gray-50 rounded-xl border border-dashed border-gray-200 text-gray-400 text-xs">
                    No items left in this context candidate.
                  </div>
                ) : (
                  <div className="space-y-2 max-h-52 overflow-y-auto pr-1">
                    {items.map((member) => {
                      const it = member.item;
                      return (
                        <div
                          key={it.id}
                          className="flex items-center justify-between gap-3 p-3 bg-white hover:bg-gray-50/80 border border-gray-200/80 rounded-xl transition-all group"
                        >
                          <div className="flex items-center gap-3 min-w-0 flex-1">
                            <div className="w-8 h-8 rounded-lg bg-gray-100 flex items-center justify-center shrink-0">
                              {getItemIcon(it.item_type)}
                            </div>
                            <div className="min-w-0 flex-1">
                              <span className="text-[13px] font-medium text-gray-900 block truncate">
                                {it.title || it.content.slice(0, 50) || 'Untitled Item'}
                              </span>
                              <span className="text-[11px] text-gray-400 block truncate">
                                {it.content.slice(0, 70)}
                              </span>
                            </div>
                          </div>

                          <button
                            onClick={() => candidate && removeItemFromCandidate(candidate.id, it.id)}
                            title="Remove item from this context candidate"
                            aria-label={`Remove ${it.title || 'item'} from candidate`}
                            className="p-1.5 text-gray-400 hover:text-red-600 hover:bg-red-50 rounded-lg transition-colors cursor-pointer opacity-0 group-hover:opacity-100 shrink-0"
                          >
                            <Trash2 size={13} />
                          </button>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>

              {/* ── Workspace Creation Confirmation ───────────────── */}
              {showCreateConfirm && (
                <div
                  id="create-confirm-section"
                  className="p-4 rounded-xl bg-amber-50 border border-amber-200 text-amber-900 text-[12.5px] animate-in fade-in duration-150"
                >
                  <div className="flex items-start gap-2.5">
                    <AlertCircle size={17} className="text-amber-600 shrink-0 mt-0.5" />
                    <div className="flex-1">
                      <h4 className="font-semibold text-gray-900 mb-0.5">
                        Create Workspace "{editedName.trim() || candidate?.label || 'New Workspace'}"?
                      </h4>
                      <p className="text-gray-600 text-[12px] mb-1 leading-relaxed">
                        This will create a new workspace and move the {items.length} included {items.length === 1 ? 'item' : 'items'} into it.
                        Your other items and workspaces remain untouched.
                      </p>
                      {aiReasoningResult && (
                        <p className="text-[11.5px] text-gray-500 italic mb-3">
                          You are creating this workspace based on your own review. The AI analysis is informational only.
                        </p>
                      )}
                      <div className="flex items-center gap-2">
                        <button
                          onClick={handleConfirmCreateWorkspace}
                          disabled={isCreatingWorkspace}
                          id="confirm-create-workspace-btn"
                          className="px-3.5 py-1.5 bg-gray-900 hover:bg-black text-white text-xs font-semibold rounded-lg shadow-sm transition-colors cursor-pointer disabled:opacity-60"
                        >
                          {isCreatingWorkspace ? 'Creating…' : 'Yes, Create Workspace'}
                        </button>
                        <button
                          onClick={() => setShowCreateConfirm(false)}
                          disabled={isCreatingWorkspace}
                          className="px-3 py-1.5 text-gray-600 hover:bg-amber-100/60 rounded-lg text-xs font-medium transition-colors cursor-pointer"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  </div>
                </div>
              )}

              {/* ── Privacy + Control Guarantee ───────────────────── */}
              <div className="flex items-start gap-2.5 p-3 rounded-xl bg-blue-50/60 border border-blue-100 text-blue-900 text-[12px]">
                <ShieldCheck size={15} className="text-blue-600 shrink-0 mt-0.5" />
                <div className="space-y-0.5">
                  <p className="font-semibold text-[12px] text-blue-800">Your data, your decision</p>
                  <p>
                    Items are not moved or modified until you explicitly create a workspace. AI analysis is read-only and advisory.
                  </p>
                </div>
              </div>

              {/* ── AI Disclaimer footer note ─────────────────────── */}
              {aiReasoningResult && (
                <div className="flex items-start gap-2 px-3 py-2.5 rounded-lg bg-gray-50 border border-gray-200 text-gray-500 text-[11.5px]">
                  <Info size={13} className="shrink-0 mt-0.5 text-gray-400" />
                  <span>
                    AI explanations are based strictly on your captured evidence. No personal data or credentials are sent externally. AI analysis may be imperfect — your judgment takes precedence.
                  </span>
                </div>
              )}
            </>
          )}
        </div>

        {/* Footer Actions */}
        <div className="px-6 py-4 bg-gray-50 border-t border-gray-100 flex items-center justify-between shrink-0">
          <button
            onClick={() => candidate && dismissCandidate(candidate.id)}
            disabled={isLoading || isCreatingWorkspace}
            className="px-3 py-1.5 text-[12.5px] font-medium text-red-600 hover:bg-red-50 rounded-lg transition-colors cursor-pointer disabled:opacity-50"
          >
            Dismiss Context
          </button>

          <div className="flex items-center gap-2.5">
            <button
              onClick={closeReview}
              disabled={isLoading || isCreatingWorkspace}
              className="px-3.5 py-1.5 text-[12.5px] font-medium text-gray-600 hover:bg-gray-200/70 rounded-lg transition-colors cursor-pointer disabled:opacity-50"
            >
              Keep for Later
            </button>
            <button
              onClick={() => setShowCreateConfirm(true)}
              disabled={isLoading || isCreatingWorkspace || items.length === 0 || showCreateConfirm}
              id="create-workspace-btn"
              className="flex items-center gap-1.5 px-4 py-1.5 text-[12.5px] font-medium text-white bg-gray-900 hover:bg-gray-800 disabled:opacity-50 rounded-lg shadow-sm hover:shadow transition-all cursor-pointer"
            >
              <FolderPlus size={14} />
              <span>Create Workspace</span>
              <ArrowRight size={13} />
            </button>
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
}
