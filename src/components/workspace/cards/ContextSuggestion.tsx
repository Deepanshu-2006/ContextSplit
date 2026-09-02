import { useState } from "react";
import { ChevronDown, ChevronUp, Sparkles } from "lucide-react";

export interface ContextSuggestionData {
  workspaceId: string;
  workspaceName: string;
  confidence: number;
  reasons?: string[];
}

interface ContextSuggestionProps {
  itemId: string;
  suggestion: ContextSuggestionData;
  onAccept: (workspaceId: string) => void | Promise<void>;
  onDismiss: () => void | Promise<void>;
}

export function ContextSuggestion({ suggestion, onAccept, onDismiss }: ContextSuggestionProps) {
  const [showReasons, setShowReasons] = useState(false);
  const [isActing, setIsActing] = useState(false);

  const handleAccept = async () => {
    setIsActing(true);
    try {
      await onAccept(suggestion.workspaceId);
    } finally {
      setIsActing(false);
    }
  };

  const handleDismiss = async () => {
    setIsActing(true);
    try {
      await onDismiss();
    } finally {
      setIsActing(false);
    }
  };

  return (
    <div className="bg-gradient-to-r from-blue-50/70 via-indigo-50/60 to-purple-50/40 border-t border-indigo-100/70 dark:border-indigo-900/30 p-2.5 text-xs flex flex-col nodrag transition-all">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 min-w-0">
          <Sparkles size={13} className="text-indigo-600 shrink-0" />
          <span className="text-slate-800 font-medium text-[12px] truncate">
            Related to <strong className="text-indigo-950 font-semibold">{suggestion.workspaceName}</strong>
          </span>
        </div>
        <span className="text-indigo-600 font-semibold text-[11px] bg-indigo-100/60 px-1.5 py-0.5 rounded-md shrink-0">
          {suggestion.confidence}% confidence
        </span>
      </div>

      {suggestion.reasons && suggestion.reasons.length > 0 && (
        <div className="mt-1">
          <button
            type="button"
            onClick={() => setShowReasons(!showReasons)}
            className="text-slate-400 hover:text-indigo-600 font-normal flex items-center gap-1 text-[10px] transition-colors cursor-pointer"
          >
            Why? {showReasons ? <ChevronUp size={11} /> : <ChevronDown size={11} />}
          </button>
          {showReasons && (
            <ul className="text-[11px] text-slate-600 mt-1.5 mb-1 pl-4 list-disc space-y-0.5">
              {suggestion.reasons.map((r, i) => (
                <li key={i}>{r}</li>
              ))}
            </ul>
          )}
        </div>
      )}

      <div className="flex items-center gap-2 mt-2 pt-1.5 border-t border-indigo-100/50">
        <button
          type="button"
          disabled={isActing}
          onClick={handleAccept}
          className="flex-1 bg-indigo-600 hover:bg-indigo-700 active:bg-indigo-800 text-white font-medium py-1 px-2.5 rounded-md transition-colors text-center text-[11px] shadow-xs truncate cursor-pointer disabled:opacity-50"
        >
          Add to {suggestion.workspaceName}
        </button>
        <button
          type="button"
          disabled={isActing}
          onClick={handleDismiss}
          className="bg-white hover:bg-slate-100 active:bg-slate-200 text-slate-600 border border-slate-200/80 font-medium py-1 px-2.5 rounded-md transition-colors text-center text-[11px] shadow-xs shrink-0 cursor-pointer disabled:opacity-50"
        >
          Dismiss
        </button>
      </div>
    </div>
  );
}
