import { useMemo } from 'react';
import { Sparkles, Layers, Clock, X, ArrowRight } from 'lucide-react';
import { useDiscoveryStore, HIGH_CONFIDENCE_THRESHOLD } from '../../store/discoveryStore';
import { ContextCandidate } from '../../services/db';

function formatRelativeTime(dateStr: string): string {
  try {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / (1000 * 60));
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays === 1) return 'Yesterday';
    return `${diffDays}d ago`;
  } catch {
    return 'Recently';
  }
}

function getItemCount(candidate: ContextCandidate): number {
  try {
    const meta = JSON.parse(candidate.metadata || '{}');
    if (meta.item_count) return meta.item_count;
    if (Array.isArray(meta.contributing_items)) return meta.contributing_items.length;
    return 0;
  } catch {
    return 0;
  }
}

export function DiscoverySection() {
  const { candidates, reviewCandidate, dismissCandidate } = useDiscoveryStore();

  // Filter only active, high-confidence candidates
  const highConfidenceCandidates = useMemo(() => {
    return candidates.filter(
      (c) =>
        c.confidence >= HIGH_CONFIDENCE_THRESHOLD &&
        (c.status === 'detected' || c.status === 'suggested')
    );
  }, [candidates]);

  if (highConfidenceCandidates.length === 0) {
    return null;
  }

  return (
    <div className="px-4 mb-5 select-none font-sans">
      {/* Section Header */}
      <div className="flex items-center justify-between px-2 mb-2">
        <div className="flex items-center gap-1.5">
          <Sparkles size={13} className="text-amber-500" />
          <span className="text-[11px] font-semibold text-gray-700 tracking-wider uppercase">
            Discovered Contexts
          </span>
        </div>
        <span className="px-1.5 py-0.5 rounded-md text-[10px] font-bold bg-amber-100/70 text-amber-800">
          {highConfidenceCandidates.length}
        </span>
      </div>

      {/* Candidate Cards List */}
      <div className="flex flex-col gap-2">
        {highConfidenceCandidates.map((cand) => {
          const itemCount = getItemCount(cand);
          const confidencePct = Math.round(cand.confidence * 100);
          const timeLabel = formatRelativeTime(cand.updated_at || cand.created_at);
          const name = cand.label || 'New Context';

          return (
            <div
              key={cand.id}
              className="bg-white/90 hover:bg-white border border-amber-200/60 rounded-xl p-2.5 shadow-2xs transition-all duration-150 group"
            >
              {/* Top Row: Name & Status */}
              <div className="flex items-start justify-between gap-1.5 mb-1.5">
                <span className="text-[13px] font-semibold text-gray-900 truncate flex-1" title={name}>
                  {name}
                </span>
                <span
                  className={`px-1.5 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wider shrink-0 ${
                    cand.status === 'suggested'
                      ? 'bg-blue-50 text-blue-700 border border-blue-200/60'
                      : 'bg-amber-50 text-amber-700 border border-amber-200/60'
                  }`}
                >
                  {cand.status}
                </span>
              </div>

              {/* Middle Row: Metrics (Confidence, Items, Time) */}
              <div className="flex items-center gap-2 text-[11px] text-gray-500 mb-2">
                <span className="font-semibold text-emerald-700 bg-emerald-50 px-1.5 py-0.5 rounded text-[10.5px]">
                  {confidencePct}%
                </span>
                <span className="flex items-center gap-1">
                  <Layers size={11} className="text-gray-400" />
                  {itemCount} {itemCount === 1 ? 'item' : 'items'}
                </span>
                <span className="flex items-center gap-1 ml-auto text-gray-400">
                  <Clock size={10} />
                  {timeLabel}
                </span>
              </div>

              {/* Action Buttons */}
              <div className="flex items-center justify-between gap-1 pt-1.5 border-t border-gray-100">
                <button
                  onClick={() => dismissCandidate(cand.id)}
                  title="Dismiss this discovered context"
                  className="p-1 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-md text-[11.5px] transition-colors cursor-pointer"
                >
                  <X size={13} />
                </button>

                <button
                  onClick={() => reviewCandidate(cand.id)}
                  className="flex items-center gap-1 px-2.5 py-1 text-[11.5px] font-medium text-gray-800 bg-gray-100 hover:bg-gray-200/80 rounded-md transition-all cursor-pointer"
                >
                  <span>Review</span>
                  <ArrowRight size={11} />
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
