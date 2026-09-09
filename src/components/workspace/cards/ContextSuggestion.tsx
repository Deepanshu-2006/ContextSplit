import { useState } from "react";
import { ChevronDown, ChevronUp, Sparkles, HelpCircle, Inbox } from "lucide-react";

export type ConfidenceTier = "high" | "moderate" | "uncertain" | "no_context";

export interface CandidateChoice {
  id: string;
  name: string;
  confidence: number;
}

export interface ContextSuggestionData {
  tier: ConfidenceTier;
  confidence: number; // 0 to 100 percentage
  workspaceId?: string | null;
  workspaceName?: string | null;
  competingCandidates?: CandidateChoice[];
  reasons?: string[];
  explanation?: string;
  uncertaintyReason?: string | null;
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

  const handleAccept = async (wsId?: string | null) => {
    const targetId = wsId || suggestion.workspaceId;
    if (!targetId) return;
    setIsActing(true);
    try {
      await onAccept(targetId);
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

  // 1. TIER: HIGH CONFIDENCE (>= 70%)
  if (suggestion.tier === "high") {
    return (
      <div className="bg-gradient-to-r from-emerald-50/90 via-teal-50/70 to-emerald-50/50 border-t border-emerald-200/80 dark:border-emerald-900/30 p-2.5 text-xs flex flex-col nodrag transition-all">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 min-w-0">
            <Sparkles size={13} className="text-emerald-600 shrink-0" />
            <span className="text-slate-800 font-medium text-[12px] truncate">
              Related to <strong className="text-emerald-950 font-semibold">{suggestion.workspaceName}</strong>
            </span>
          </div>
          <span className="text-emerald-700 font-semibold text-[11px] bg-emerald-100/80 border border-emerald-200/70 px-1.5 py-0.5 rounded-md shrink-0 shadow-2xs">
            High Confidence ({suggestion.confidence}%)
          </span>
        </div>

        {suggestion.reasons && suggestion.reasons.length > 0 && (
          <div className="mt-1">
            <button
              type="button"
              onClick={() => setShowReasons(!showReasons)}
              className="text-slate-400 hover:text-emerald-700 font-normal flex items-center gap-1 text-[10px] transition-colors cursor-pointer"
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

        <div className="flex items-center gap-2 mt-2 pt-1.5 border-t border-emerald-200/60">
          <button
            type="button"
            disabled={isActing}
            onClick={() => handleAccept()}
            className="flex-1 bg-emerald-600 hover:bg-emerald-700 active:bg-emerald-800 text-white font-medium py-1 px-2.5 rounded-md transition-colors text-center text-[11px] shadow-xs truncate cursor-pointer disabled:opacity-50"
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

  // 2. TIER: MODERATE CONFIDENCE (40% - 69%)
  if (suggestion.tier === "moderate") {
    return (
      <div className="bg-gradient-to-r from-blue-50/80 via-indigo-50/70 to-blue-50/50 border-t border-blue-200/70 dark:border-blue-900/30 p-2.5 text-xs flex flex-col nodrag transition-all">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 min-w-0">
            <Sparkles size={13} className="text-blue-600 shrink-0" />
            <span className="text-slate-800 font-medium text-[12px] truncate">
              Suggested for <strong className="text-blue-950 font-semibold">{suggestion.workspaceName}</strong>
            </span>
          </div>
          <span className="text-blue-700 font-semibold text-[11px] bg-blue-100/70 border border-blue-200/60 px-1.5 py-0.5 rounded-md shrink-0 shadow-2xs">
            Moderate ({suggestion.confidence}%)
          </span>
        </div>

        {suggestion.reasons && suggestion.reasons.length > 0 && (
          <div className="mt-1">
            <button
              type="button"
              onClick={() => setShowReasons(!showReasons)}
              className="text-slate-400 hover:text-blue-600 font-normal flex items-center gap-1 text-[10px] transition-colors cursor-pointer"
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

        <div className="flex items-center gap-2 mt-2 pt-1.5 border-t border-blue-200/50">
          <button
            type="button"
            disabled={isActing}
            onClick={() => handleAccept()}
            className="flex-1 bg-blue-600 hover:bg-blue-700 active:bg-blue-800 text-white font-medium py-1 px-2.5 rounded-md transition-colors text-center text-[11px] shadow-xs truncate cursor-pointer disabled:opacity-50"
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

  // 3. TIER: UNCERTAIN (competing contexts / conflicting signals)
  if (suggestion.tier === "uncertain") {
    return (
      <div className="bg-gradient-to-r from-amber-50/95 via-orange-50/80 to-amber-50/60 border-t border-amber-200/90 dark:border-amber-900/40 p-2.5 text-xs flex flex-col nodrag transition-all">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 min-w-0">
            <HelpCircle size={13} className="text-amber-600 shrink-0" />
            <span className="text-amber-950 font-semibold text-[12px] truncate">
              Uncertain Context
            </span>
          </div>
          <span className="text-amber-800 font-semibold text-[11px] bg-amber-100/90 border border-amber-300/80 px-1.5 py-0.5 rounded-md shrink-0 shadow-2xs">
            Competing Signals
          </span>
        </div>

        <p className="text-amber-900/90 text-[11px] mt-1 leading-snug">
          {suggestion.explanation || "Signals show competing affinity across multiple workspaces. Not forced into any workspace."}
        </p>

        {suggestion.uncertaintyReason && (
          <p className="text-amber-800/80 text-[10px] mt-0.5 italic">
            {suggestion.uncertaintyReason}
          </p>
        )}

        {suggestion.reasons && suggestion.reasons.length > 0 && (
          <div className="mt-1">
            <button
              type="button"
              onClick={() => setShowReasons(!showReasons)}
              className="text-amber-700/70 hover:text-amber-900 font-normal flex items-center gap-1 text-[10px] transition-colors cursor-pointer"
            >
              Signal breakdown {showReasons ? <ChevronUp size={11} /> : <ChevronDown size={11} />}
            </button>
            {showReasons && (
              <ul className="text-[11px] text-amber-950/80 mt-1 pl-4 list-disc space-y-0.5">
                {suggestion.reasons.map((r, i) => (
                  <li key={i}>{r}</li>
                ))}
              </ul>
            )}
          </div>
        )}

        {/* Unforced Options: User picks between competing candidates or keeps in inbox */}
        <div className="flex flex-wrap items-center gap-1.5 mt-2 pt-1.5 border-t border-amber-200/70">
          <span className="text-[10px] text-amber-900 font-medium mr-0.5">Select:</span>
          {suggestion.competingCandidates && suggestion.competingCandidates.length > 0 ? (
            suggestion.competingCandidates.map((cand) => (
              <button
                key={cand.id}
                type="button"
                disabled={isActing}
                onClick={() => handleAccept(cand.id)}
                className="bg-amber-600 hover:bg-amber-700 active:bg-amber-800 text-white font-medium py-0.5 px-2 rounded-md transition-colors text-[10px] shadow-xs cursor-pointer disabled:opacity-50"
              >
                {cand.name} ({cand.confidence}%)
              </button>
            ))
          ) : (
            <span className="text-[10px] text-amber-700 italic">No single workspace chosen</span>
          )}
          <button
            type="button"
            disabled={isActing}
            onClick={handleDismiss}
            className="bg-white hover:bg-amber-50 active:bg-amber-100 text-amber-800 border border-amber-200 font-medium py-0.5 px-2 rounded-md transition-colors text-[10px] shadow-xs shrink-0 cursor-pointer disabled:opacity-50 ml-auto"
          >
            Keep in Inbox
          </button>
        </div>
      </div>
    );
  }

  // 4. TIER: NO MEANINGFUL CONTEXT (insufficient evidence / weak signals)
  return (
    <div className="bg-gradient-to-r from-slate-50/95 via-gray-50/85 to-slate-50/70 border-t border-slate-200/80 dark:border-slate-800/40 p-2.5 text-xs flex flex-col nodrag transition-all">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 min-w-0">
          <Inbox size={13} className="text-slate-500 shrink-0" />
          <span className="text-slate-700 font-medium text-[12px] truncate">
            No Strong Context Yet
          </span>
        </div>
        <span className="text-slate-600 font-semibold text-[10px] bg-slate-200/70 border border-slate-300/70 px-1.5 py-0.5 rounded-md shrink-0 shadow-2xs">
          Inbox
        </span>
      </div>

      <p className="text-slate-600 text-[11px] mt-1 leading-snug">
        {suggestion.explanation || "Available signals are too weak or inconclusive to suggest a workspace. Item remains safely in Inbox."}
      </p>

      {suggestion.uncertaintyReason && (
        <p className="text-slate-500 text-[10px] mt-0.5 italic">
          {suggestion.uncertaintyReason}
        </p>
      )}

      <div className="flex items-center justify-between gap-2 mt-2 pt-1.5 border-t border-slate-200/60">
        <span className="text-[10px] text-slate-400 truncate">
          Deterministic signals are below actionable threshold
        </span>
        <button
          type="button"
          disabled={isActing}
          onClick={handleDismiss}
          className="bg-white hover:bg-slate-100 active:bg-slate-200 text-slate-600 border border-slate-200 font-medium py-0.5 px-2 rounded-md transition-colors text-[10px] shadow-xs shrink-0 cursor-pointer disabled:opacity-50"
        >
          Dismiss notice
        </button>
      </div>
    </div>
  );
}
