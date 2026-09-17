import { useMemo } from 'react';
import { Layers, X, ArrowRight } from 'lucide-react';
import { useDiscoveryStore } from '../../store/discoveryStore';

export function DiscoveryNotificationCard() {
  const { highConfidenceCandidate, reviewCandidate, dismissCandidate } = useDiscoveryStore();

  const parsedData = useMemo(() => {
    if (!highConfidenceCandidate) return null;
    try {
      const meta = JSON.parse(highConfidenceCandidate.metadata || '{}');
      const count: number =
        meta.item_count ||
        (Array.isArray(meta.contributing_items) ? meta.contributing_items.length : 0);
      return {
        count,
        confidencePct: Math.round(highConfidenceCandidate.confidence * 100),
      };
    } catch {
      return {
        count: 0,
        confidencePct: Math.round(highConfidenceCandidate.confidence * 100),
      };
    }
  }, [highConfidenceCandidate]);

  if (!highConfidenceCandidate || !parsedData) {
    return null;
  }

  const { count, confidencePct } = parsedData;
  const suggestedName = highConfidenceCandidate.label || 'New Context';

  return (
    <div
      role="region"
      aria-label="New context notification"
      className="fixed bottom-6 right-6 z-40 w-[330px] bg-white/95 backdrop-blur-2xl border border-gray-200/80 rounded-2xl shadow-[0_12px_32px_-4px_rgba(0,0,0,0.12)] p-3.5 animate-in fade-in slide-in-from-bottom-4 duration-300 transition-all font-sans select-none"
    >
      {/* Top Meta Header */}
      <div className="flex items-center justify-between gap-2 mb-2">
        <div className="flex items-center gap-1.5 text-gray-500">
          <div className="w-5 h-5 rounded-md bg-indigo-50 border border-indigo-100 flex items-center justify-center">
            <Layers size={11} className="text-indigo-600" />
          </div>
          <span className="text-[11px] font-semibold text-gray-600 tracking-tight">
            Suggested Context
          </span>
        </div>

        <div className="flex items-center gap-1.5">
          <span className="px-2 py-0.5 rounded-full text-[10.5px] font-semibold bg-emerald-50 text-emerald-700 border border-emerald-200/60">
            {confidencePct}% match
          </span>
          <button
            onClick={() => dismissCandidate(highConfidenceCandidate.id)}
            title="Dismiss"
            className="p-1 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-md transition-colors cursor-pointer"
          >
            <X size={13} />
          </button>
        </div>
      </div>

      {/* Main Content */}
      <div className="mb-3 px-0.5">
        <h4 className="text-[13.5px] font-semibold text-gray-900 tracking-tight truncate leading-snug">
          {suggestedName}
        </h4>
        <p className="text-[11.5px] text-gray-500 mt-0.5">
          {count > 0 ? `${count} related ${count === 1 ? 'item' : 'items'} detected` : 'Cohesive activity detected'}
        </p>
      </div>

      {/* Action Footer */}
      <div className="flex items-center justify-end gap-1.5 pt-2 border-t border-gray-100/90">
        <button
          onClick={() => dismissCandidate(highConfidenceCandidate.id)}
          className="px-2.5 py-1.5 text-[11.5px] font-medium text-gray-500 hover:text-gray-800 hover:bg-gray-100/80 rounded-lg transition-colors cursor-pointer"
        >
          Dismiss
        </button>
        <button
          onClick={() => reviewCandidate(highConfidenceCandidate.id)}
          className="flex items-center gap-1.5 px-3 py-1.5 text-[11.5px] font-medium text-white bg-gray-900 hover:bg-black rounded-lg shadow-xs transition-all cursor-pointer"
        >
          <span>Review</span>
          <ArrowRight size={11} />
        </button>
      </div>
    </div>
  );
}
