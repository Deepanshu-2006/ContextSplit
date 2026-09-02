import { useMemo } from 'react';
import { Sparkles, X, Layers, ArrowRight, CheckCircle2 } from 'lucide-react';
import { useDiscoveryStore } from '../../store/discoveryStore';

export function DiscoveryNotificationCard() {
  const { highConfidenceCandidate, reviewCandidate, dismissCandidate } = useDiscoveryStore();

  const parsedData = useMemo(() => {
    if (!highConfidenceCandidate) return null;
    try {
      const meta = JSON.parse(highConfidenceCandidate.metadata || '{}');
      const evidenceList: string[] = Array.isArray(meta.evidence) ? meta.evidence : [];
      const count: number =
        meta.item_count ||
        (Array.isArray(meta.contributing_items) ? meta.contributing_items.length : 0);
      return {
        evidenceList,
        count,
        confidencePct: Math.round(highConfidenceCandidate.confidence * 100),
      };
    } catch {
      return {
        evidenceList: [],
        count: 0,
        confidencePct: Math.round(highConfidenceCandidate.confidence * 100),
      };
    }
  }, [highConfidenceCandidate]);

  if (!highConfidenceCandidate || !parsedData) {
    return null;
  }

  const { evidenceList, count, confidencePct } = parsedData;
  const suggestedName = highConfidenceCandidate.label || 'New Context';
  // Take up to 2 concise evidence bullets for the compact notification card
  const shortEvidence = evidenceList.slice(0, 2);

  return (
    <div
      role="region"
      aria-label="New context notification"
      className="fixed bottom-6 right-6 z-40 max-w-[360px] w-full bg-white/95 backdrop-blur-xl border border-gray-200/80 rounded-2xl shadow-xl p-4.5 animate-in fade-in slide-in-from-bottom-5 duration-300 transition-all font-sans select-none"
    >
      {/* Header */}
      <div className="flex items-center justify-between gap-2 mb-2.5">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg bg-amber-50 border border-amber-200/60 flex items-center justify-center shadow-xs">
            <Sparkles size={14} className="text-amber-600 animate-pulse" />
          </div>
          <div>
            <span className="text-[12px] font-semibold text-gray-900 tracking-tight block leading-tight">
              New context detected
            </span>
            <span className="text-[11px] text-gray-500 font-medium">
              Context Discovery Engine
            </span>
          </div>
        </div>

        <div className="flex items-center gap-1.5">
          <span className="px-2 py-0.5 rounded-full text-[11px] font-semibold bg-emerald-50 text-emerald-700 border border-emerald-200/70 shadow-2xs">
            {confidencePct}% match
          </span>
          <button
            onClick={() => dismissCandidate(highConfidenceCandidate.id)}
            title="Dismiss context"
            className="p-1 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-md transition-colors"
          >
            <X size={14} />
          </button>
        </div>
      </div>

      {/* Suggested Name & Item Count */}
      <div className="bg-gray-50/80 border border-gray-100 rounded-xl p-3 mb-3">
        <div className="flex items-center justify-between gap-2 mb-1.5">
          <h4 className="text-[13px] font-semibold text-gray-900 truncate">
            {suggestedName}
          </h4>
          <span className="flex items-center gap-1 text-[11px] font-medium text-gray-500 shrink-0">
            <Layers size={12} className="text-gray-400" />
            {count > 0 ? `${count} ${count === 1 ? 'Item' : 'Items'}` : 'Related items'}
          </span>
        </div>

        {/* Short Evidence Summary */}
        {shortEvidence.length > 0 ? (
          <ul className="space-y-1">
            {shortEvidence.map((ev, idx) => (
              <li key={idx} className="text-[11.5px] text-gray-600 flex items-start gap-1.5 leading-snug">
                <CheckCircle2 size={12} className="text-emerald-500 shrink-0 mt-0.5" />
                <span className="truncate">{ev}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-[11.5px] text-gray-500">
            Strong mutual relationships and cohesive activity detected.
          </p>
        )}
      </div>

      {/* Actions */}
      <div className="flex items-center justify-end gap-2">
        <button
          onClick={() => dismissCandidate(highConfidenceCandidate.id)}
          className="px-3 py-1.5 text-[12px] font-medium text-gray-600 hover:text-gray-900 hover:bg-gray-100/80 rounded-lg transition-colors cursor-pointer"
        >
          Dismiss
        </button>
        <button
          onClick={() => reviewCandidate(highConfidenceCandidate.id)}
          className="flex items-center gap-1.5 px-3.5 py-1.5 text-[12px] font-medium text-white bg-gray-900 hover:bg-gray-800 rounded-lg shadow-sm hover:shadow transition-all cursor-pointer"
        >
          <span>Review</span>
          <ArrowRight size={12} />
        </button>
      </div>
    </div>
  );
}
