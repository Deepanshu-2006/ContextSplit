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
} from 'lucide-react';
import { useDiscoveryStore } from '../../store/discoveryStore';

export function CandidateReviewModal() {
  const {
    isReviewOpen,
    selectedCandidateDetails,
    selectedCandidateScore,
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

  // Convert technical evidence into human-friendly, plain English phrases
  const formatFriendlyEvidence = (rawEvidence: string): string => {
    const lower = rawEvidence.toLowerCase();
    if (lower.includes('semantically related')) {
      return rawEvidence.replace(/semantically related/gi, 'content-related');
    }
    if (lower.includes('session')) {
      return rawEvidence.replace(/captured in the same session/gi, 'worked on during the same focused session');
    }
    if (lower.includes('domain')) {
      return rawEvidence;
    }
    if (lower.includes('mutual relationships')) {
      return 'Items frequently connect to the same activity';
    }
    return rawEvidence;
  };

  const confidencePct = Math.round((candidate?.confidence || 0) * 100);

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="candidate-review-title"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs font-sans animate-in fade-in duration-200"
    >
      <div className="relative w-full max-w-2xl max-h-[88vh] bg-white rounded-2xl shadow-2xl border border-gray-200 flex flex-col overflow-hidden animate-in zoom-in-95 duration-200">
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-100 flex items-center justify-between bg-gray-50/50">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-xl bg-amber-50 border border-amber-200/70 flex items-center justify-center shadow-2xs">
              <Sparkles size={16} className="text-amber-600" />
            </div>
            <div>
              <h2 id="candidate-review-title" className="text-[15px] font-semibold text-gray-900 leading-tight">
                Review Discovered Context
              </h2>
              <p className="text-[12px] text-gray-500">
                Organize related items into a dedicated workspace
              </p>
            </div>
          </div>
          <button
            onClick={closeReview}
            className="p-1.5 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors cursor-pointer"
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
              {/* Context Name & Confidence Section */}
              <div className="bg-gray-50/90 border border-gray-200/80 rounded-xl p-4.5">
                <div className="flex items-center justify-between gap-4 mb-3">
                  <div className="flex-1 min-w-0">
                    <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block mb-1">
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
                      <div className="flex items-center gap-2.5 group flex-wrap">
                        <span className="text-[16px] font-semibold text-gray-900 truncate max-w-[280px]">
                          {candidate?.label || 'New Context'}
                        </span>
                        <button
                          onClick={() => setIsEditingName(true)}
                          className="p-1 text-gray-400 hover:text-gray-600 opacity-60 group-hover:opacity-100 transition-opacity rounded cursor-pointer"
                          title="Click to rename context"
                        >
                          <Edit2 size={13} />
                        </button>
                        <button
                          onClick={handleAiSuggestName}
                          disabled={isAiSuggesting}
                          className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-medium bg-purple-50 text-purple-700 hover:bg-purple-100 border border-purple-200 transition-colors disabled:opacity-50 cursor-pointer"
                          title="Suggest a concise 2–4 word name grounded in evidence"
                        >
                          <Sparkles size={11} className={isAiSuggesting ? "animate-spin" : "text-purple-500"} />
                          {isAiSuggesting ? "Suggesting..." : "AI Suggest"}
                        </button>
                      </div>
                    )}
                  </div>

                  <div className="text-right shrink-0">
                    <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block mb-1">
                      Match Confidence
                    </span>
                    <div className="flex items-center gap-2">
                      <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-[12px] font-bold bg-emerald-50 text-emerald-700 border border-emerald-200/80">
                        {confidencePct}%
                      </span>
                    </div>
                  </div>
                </div>

                {/* Plain-English Context Reasons */}
                {score?.evidence && score.evidence.length > 0 && (
                  <div className="border-t border-gray-200/60 pt-3 mt-3">
                    <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block mb-2">
                      Why these items belong together
                    </span>
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                      {score.evidence.map((ev, idx) => (
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
                )}
              </div>

              {/* Discovered Items List */}
              <div>
                <div className="flex items-center justify-between mb-2.5">
                  <div className="flex items-center gap-2">
                    <Layers size={14} className="text-gray-500" />
                    <h3 className="text-[13px] font-semibold text-gray-900">
                      Included Items ({items.length})
                    </h3>
                  </div>
                  <span className="text-[11.5px] text-gray-400">
                    You can remove items that don't belong
                  </span>
                </div>

                {items.length === 0 ? (
                  <div className="p-8 text-center bg-gray-50 rounded-xl border border-dashed border-gray-200 text-gray-400 text-xs">
                    No items left in this context candidate.
                  </div>
                ) : (
                  <div className="space-y-2 max-h-56 overflow-y-auto pr-1">
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
                                {it.title || it.content.slice(0, 45) || 'Untitled Item'}
                              </span>
                              <span className="text-[11px] text-gray-400 block truncate">
                                {it.content.slice(0, 65)}
                              </span>
                            </div>
                          </div>

                          <div className="flex items-center gap-2 shrink-0">
                            <button
                              onClick={() => candidate && removeItemFromCandidate(candidate.id, it.id)}
                              title="Remove item from this context candidate"
                              className="p-1.5 text-gray-400 hover:text-red-600 hover:bg-red-50 rounded-lg transition-colors cursor-pointer"
                            >
                              <Trash2 size={13} />
                            </button>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>

              {/* Confirmation Box (when Create Workspace clicked) */}
              {showCreateConfirm && (
                <div className="p-4 rounded-xl bg-amber-50 border border-amber-200 text-amber-900 text-[12.5px] animate-in fade-in duration-150">
                  <div className="flex items-start gap-2.5">
                    <AlertCircle size={17} className="text-amber-600 shrink-0 mt-0.5" />
                    <div className="flex-1">
                      <h4 className="font-semibold text-gray-900 mb-0.5">
                        Create Workspace "{editedName.trim() || candidate?.label || 'New Workspace'}"?
                      </h4>
                      <p className="text-gray-600 text-[12px] mb-3 leading-relaxed">
                        This will create a new workspace and move the {items.length} included items into it. Your other items will remain untouched.
                      </p>
                      <div className="flex items-center gap-2">
                        <button
                          onClick={handleConfirmCreateWorkspace}
                          disabled={isCreatingWorkspace}
                          className="px-3.5 py-1.5 bg-gray-900 hover:bg-black text-white text-xs font-semibold rounded-lg shadow-sm transition-colors cursor-pointer"
                        >
                          {isCreatingWorkspace ? 'Creating...' : 'Yes, Create Workspace'}
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

              {/* Privacy and Control Guarantee Note */}
              <div className="flex items-center gap-2.5 p-3 rounded-xl bg-blue-50/60 border border-blue-100 text-blue-900 text-[12px]">
                <ShieldCheck size={16} className="text-blue-600 shrink-0" />
                <span>
                  Items will not be moved or modified until you explicitly choose to create a workspace.
                </span>
              </div>
            </>
          )}
        </div>

        {/* Footer Actions */}
        <div className="px-6 py-4 bg-gray-50 border-t border-gray-100 flex items-center justify-between">
          <button
            onClick={() => candidate && dismissCandidate(candidate.id)}
            disabled={isLoading || isCreatingWorkspace}
            className="px-3 py-1.5 text-[12.5px] font-medium text-red-600 hover:bg-red-50 rounded-lg transition-colors cursor-pointer"
          >
            Dismiss Context
          </button>

          <div className="flex items-center gap-2.5">
            <button
              onClick={closeReview}
              disabled={isLoading || isCreatingWorkspace}
              className="px-3.5 py-1.5 text-[12.5px] font-medium text-gray-600 hover:bg-gray-200/70 rounded-lg transition-colors cursor-pointer"
            >
              Keep for Later
            </button>
            <button
              onClick={() => setShowCreateConfirm(true)}
              disabled={isLoading || isCreatingWorkspace || items.length === 0}
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
