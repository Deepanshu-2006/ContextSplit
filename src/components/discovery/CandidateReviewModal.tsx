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
  } = useDiscoveryStore();

  const [isEditingName, setIsEditingName] = useState(false);
  const [editedName, setEditedName] = useState('');

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

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="candidate-review-title"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs font-sans animate-in fade-in duration-200"
    >
      <div className="relative w-full max-w-2xl max-h-[85vh] bg-white rounded-2xl shadow-2xl border border-gray-200 flex flex-col overflow-hidden animate-in zoom-in-95 duration-200">
        {/* Header */}
        <div className="px-6 py-4.5 border-b border-gray-100 flex items-center justify-between bg-gray-50/50">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-xl bg-amber-50 border border-amber-200/70 flex items-center justify-center shadow-2xs">
              <Sparkles size={16} className="text-amber-600" />
            </div>
            <div>
              <h2 id="candidate-review-title" className="text-[15px] font-semibold text-gray-900 leading-tight">
                Review Context Candidate
              </h2>
              <p className="text-[12px] text-gray-500">
                Discovered from item relationships, sessions, and semantic patterns
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
          {isLoading ? (
            <div className="flex items-center justify-center py-12 text-sm text-gray-400">
              Loading candidate details...
            </div>
          ) : (
            <>
              {/* Context Name & Confidence Section */}
              <div className="bg-gray-50 border border-gray-200/70 rounded-xl p-4">
                <div className="flex items-center justify-between gap-4 mb-3">
                  <div className="flex-1 min-w-0">
                    <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block mb-1">
                      Suggested Context Name
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
                          className="flex-1 px-2.5 py-1 text-[14px] font-semibold bg-white border border-blue-400 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/20 shadow-2xs"
                        />
                        <button
                          onClick={handleSaveName}
                          className="px-2.5 py-1 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-xs font-medium flex items-center gap-1 cursor-pointer"
                        >
                          <Check size={12} />
                          Save
                        </button>
                        <button
                          onClick={() => setIsEditingName(false)}
                          className="px-2 py-1 text-gray-500 hover:text-gray-700 text-xs font-medium cursor-pointer"
                        >
                          Cancel
                        </button>
                      </div>
                    ) : (
                      <div className="flex items-center gap-2 group">
                        <span className="text-[15px] font-semibold text-gray-900 truncate">
                          {candidate?.label || 'New Context'}
                        </span>
                        <button
                          onClick={() => setIsEditingName(true)}
                          className="p-1 text-gray-400 hover:text-gray-600 opacity-0 group-hover:opacity-100 transition-opacity rounded cursor-pointer"
                          title="Edit suggested name"
                        >
                          <Edit2 size={12} />
                        </button>
                      </div>
                    )}
                  </div>

                  <div className="text-right shrink-0">
                    <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block mb-1">
                      Confidence
                    </span>
                    <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-[12px] font-bold bg-emerald-50 text-emerald-700 border border-emerald-200">
                      {score?.confidence_percentage || `${Math.round((candidate?.confidence || 0) * 100)}%`}
                    </span>
                  </div>
                </div>

                {/* Evidence Summary List */}
                {score?.evidence && score.evidence.length > 0 && (
                  <div className="border-t border-gray-200/60 pt-3 mt-3">
                    <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block mb-2">
                      Context Evidence
                    </span>
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                      {score.evidence.map((ev, idx) => (
                        <div
                          key={idx}
                          className="flex items-start gap-2 text-[12px] text-gray-700 bg-white border border-gray-100 rounded-lg p-2 shadow-2xs"
                        >
                          <CheckCircle2 size={13} className="text-emerald-500 shrink-0 mt-0.5" />
                          <span className="leading-snug">{ev}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>

              {/* Related Items List */}
              <div>
                <div className="flex items-center justify-between mb-2.5">
                  <div className="flex items-center gap-2">
                    <Layers size={14} className="text-gray-500" />
                    <h3 className="text-[13px] font-semibold text-gray-900">
                      Discovered Items ({items.length})
                    </h3>
                  </div>
                  <span className="text-[11px] text-gray-400">
                    Ranked by intra-cluster relevance
                  </span>
                </div>

                <div className="space-y-2 max-h-56 overflow-y-auto pr-1">
                  {items.map((member) => {
                    const it = member.item;
                    return (
                      <div
                        key={it.id}
                        className="flex items-center justify-between gap-3 p-3 bg-white hover:bg-gray-50/80 border border-gray-200/80 rounded-xl transition-colors"
                      >
                        <div className="flex items-center gap-3 min-w-0">
                          <div className="w-7 h-7 rounded-lg bg-gray-100 flex items-center justify-center shrink-0">
                            {getItemIcon(it.item_type)}
                          </div>
                          <div className="min-w-0">
                            <span className="text-[13px] font-medium text-gray-900 block truncate">
                              {it.title || it.content.slice(0, 40) || 'Untitled Item'}
                            </span>
                            <span className="text-[11px] text-gray-400 block truncate">
                              {it.content.slice(0, 60)}
                            </span>
                          </div>
                        </div>

                        <div className="flex items-center gap-2 shrink-0">
                          <span className="text-[11px] font-mono text-gray-500 bg-gray-100 px-2 py-0.5 rounded-md">
                            {Math.round(member.membership.relevance_score * 100)}% rel
                          </span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>

              {/* Privacy and Control Note */}
              <div className="flex items-center gap-2.5 p-3 rounded-xl bg-blue-50/60 border border-blue-100 text-blue-900 text-[12px]">
                <ShieldCheck size={16} className="text-blue-600 shrink-0" />
                <span>
                  ContextSplit will not automatically move your items or modify existing workspaces.
                </span>
              </div>
            </>
          )}
        </div>

        {/* Footer Actions */}
        <div className="px-6 py-4 bg-gray-50 border-t border-gray-100 flex items-center justify-between">
          <button
            onClick={() => candidate && dismissCandidate(candidate.id)}
            className="px-3 py-1.5 text-[12.5px] font-medium text-red-600 hover:bg-red-50 rounded-lg transition-colors cursor-pointer"
          >
            Dismiss Context
          </button>
          <div className="flex items-center gap-2">
            <button
              onClick={closeReview}
              className="px-4 py-1.5 text-[12.5px] font-medium text-gray-700 hover:bg-gray-200/70 bg-gray-100 rounded-lg transition-colors cursor-pointer"
            >
              Done
            </button>
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
}

