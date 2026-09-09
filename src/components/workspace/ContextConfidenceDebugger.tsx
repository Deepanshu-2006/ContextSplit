import { useState } from "react";
import {
  BrainCircuit,
  AlertTriangle,
  Sparkles,
  ShieldCheck,
  Scale,
  Layers,
  Database,
  BarChart2,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Terminal,
} from "lucide-react";
import {
  ContextConfidenceDebugTrace,
  CandidateConfidenceDebugItem,
} from "../../services/db";

interface ContextConfidenceDebuggerProps {
  trace: ContextConfidenceDebugTrace | null;
  loading?: boolean;
}

export function ContextConfidenceDebugger({ trace, loading }: ContextConfidenceDebuggerProps) {
  const [activeStage, setActiveStage] = useState<"summary" | "raw" | "normalized" | "contributions" | "diversity" | "decision">("summary");
  const [selectedCandidateIndex, setSelectedCandidateIndex] = useState<number>(0);
  const [expandedRawItems, setExpandedRawItems] = useState<Record<string, boolean>>({});

  if (loading) {
    return (
      <div className="p-8 text-center text-slate-500 space-y-3">
        <div className="animate-spin inline-block w-6 h-6 border-2 border-indigo-600 border-t-transparent rounded-full" />
        <p className="text-xs font-mono">Running Context Confidence Calibration & Diagnostics...</p>
      </div>
    );
  }

  if (!trace) {
    return (
      <div className="p-8 text-center text-slate-400 text-xs">
        No context confidence debug trace available for this item.
      </div>
    );
  }

  const selectedCandidate: CandidateConfidenceDebugItem | undefined =
    trace.candidates[selectedCandidateIndex] || trace.candidates[0];

  const getBandBadgeClass = (band: string) => {
    switch (band.toLowerCase()) {
      case "very_high":
        return "bg-emerald-100 text-emerald-800 border-emerald-300";
      case "high":
        return "bg-teal-100 text-teal-800 border-teal-300";
      case "medium":
        return "bg-blue-100 text-blue-800 border-blue-300";
      case "low":
        return "bg-amber-100 text-amber-800 border-amber-300";
      case "very_low":
      default:
        return "bg-slate-100 text-slate-700 border-slate-300";
    }
  };

  const getBandDisplayName = (band: string) => {
    switch (band.toLowerCase()) {
      case "very_high": return "Very High";
      case "high": return "High";
      case "medium": return "Medium";
      case "low": return "Low";
      case "very_low": return "Very Low";
      default: return band;
    }
  };

  const toggleRawItem = (id: string) => {
    setExpandedRawItems(prev => ({ ...prev, [id]: !prev[id] }));
  };

  return (
    <div className="space-y-6 text-slate-800">
      {/* Dev Header Badge & Stage Switcher */}
      <div className="bg-slate-900 text-slate-100 p-4 rounded-xl shadow-sm border border-slate-800 space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Terminal size={16} className="text-emerald-400" />
            <h3 className="text-xs font-mono font-semibold tracking-wider text-slate-200 uppercase">
              Context Confidence Debugger
            </h3>
            <span className="text-[10px] font-mono bg-emerald-950 text-emerald-400 border border-emerald-800/80 px-2 py-0.5 rounded">
              DEV / DIAGNOSTIC
            </span>
          </div>
          <div className="text-[11px] font-mono text-slate-400">
            Target Item: <span className="text-slate-200">{trace.item_id}</span> ({trace.item_type})
          </div>
        </div>

        {/* Pipeline Stage Navigator */}
        <div className="flex flex-wrap gap-1.5 pt-1 border-t border-slate-800 text-xs">
          {[
            { key: "summary", label: "Overview & Candidates", icon: <Scale size={13} /> },
            { key: "raw", label: "1. Raw Evidence", icon: <Database size={13} /> },
            { key: "normalized", label: "2. Normalized Evidence", icon: <Layers size={13} /> },
            { key: "contributions", label: "3. Score Breakdown", icon: <BarChart2 size={13} /> },
            { key: "diversity", label: "4. Diversity & Competition", icon: <BrainCircuit size={13} /> },
            { key: "decision", label: "5. Final Decision", icon: <CheckCircle2 size={13} /> },
          ].map(stage => (
            <button
              key={stage.key}
              onClick={() => setActiveStage(stage.key as any)}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg font-medium transition-colors text-xs ${
                activeStage === stage.key
                  ? "bg-indigo-600 text-white shadow-sm"
                  : "bg-slate-800/70 hover:bg-slate-800 text-slate-300"
              }`}
            >
              {stage.icon}
              {stage.label}
            </button>
          ))}
        </div>
      </div>

      {/* ========================================================================= */}
      {/* STAGE 0: OVERVIEW & CANDIDATE BREAKDOWN (Exact requested format)           */}
      {/* ========================================================================= */}
      {activeStage === "summary" && (
        <div className="space-y-6">
          {/* Top Level Summary Cards Grid */}
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            
            {/* Final Result Card */}
            <div className="bg-white rounded-xl border border-indigo-100 shadow-sm p-4 flex flex-col justify-between">
              <div>
                <span className="text-[10px] uppercase font-bold tracking-wider text-indigo-600">Final Result</span>
                <h4 className="text-sm font-semibold text-slate-900 mt-1 flex items-center gap-1.5">
                  <Sparkles size={15} className="text-indigo-500 shrink-0" />
                  {trace.final_result.title}
                </h4>
                <p className="text-xs text-slate-600 mt-2 leading-relaxed">
                  {trace.final_result.summary}
                </p>
              </div>
              <div className="mt-3 pt-2 border-t border-slate-100 flex items-center justify-between text-xs font-mono">
                <span className="text-slate-400">Decision:</span>
                <span className="font-semibold text-slate-700 bg-slate-100 px-2 py-0.5 rounded">
                  {trace.final_result.decision}
                </span>
              </div>
            </div>

            {/* Competition Card */}
            <div className="bg-white rounded-xl border border-slate-200 shadow-sm p-4 flex flex-col justify-between">
              <div>
                <span className="text-[10px] uppercase font-bold tracking-wider text-purple-600">Competition</span>
                {trace.competition ? (
                  <div className="mt-2 space-y-1.5 font-mono text-xs">
                    {trace.competition.top_context_name && (
                      <div className="flex justify-between items-center py-0.5">
                        <span className="text-slate-800 font-semibold">{trace.competition.top_context_name}:</span>
                        <span className="font-bold text-slate-900">{trace.competition.top_score}</span>
                      </div>
                    )}
                    {trace.competition.runner_up_context_name && (
                      <div className="flex justify-between items-center py-0.5 text-slate-600">
                        <span>{trace.competition.runner_up_context_name}:</span>
                        <span className="font-semibold">{trace.competition.runner_up_score}</span>
                      </div>
                    )}
                    <div className="flex justify-between items-center pt-1 border-t border-slate-100 font-semibold text-purple-700">
                      <span>Margin:</span>
                      <span>{trace.competition.score_margin}</span>
                    </div>
                  </div>
                ) : (
                  <p className="text-xs text-slate-400 italic mt-2">Single candidate / No competition</p>
                )}
              </div>
              <div className="mt-3 pt-2 border-t border-slate-100 flex items-center justify-between text-xs">
                <span className="text-slate-400">Ambiguity State:</span>
                <span className={`font-mono text-[11px] font-semibold px-2 py-0.5 rounded ${
                  trace.competition?.ambiguity_state === "clear"
                    ? "bg-emerald-100 text-emerald-800"
                    : trace.competition?.ambiguity_state === "probable"
                    ? "bg-blue-100 text-blue-800"
                    : "bg-amber-100 text-amber-800"
                }`}>
                  {trace.competition?.ambiguity_state || "none"}
                </span>
              </div>
            </div>

            {/* Evidence Diversity Card */}
            <div className="bg-white rounded-xl border border-slate-200 shadow-sm p-4 flex flex-col justify-between">
              <div>
                <span className="text-[10px] uppercase font-bold tracking-wider text-teal-600">Evidence Diversity</span>
                <h4 className="text-sm font-semibold text-slate-900 mt-1">
                  {trace.evidence_diversity.independent_groups_count} independent {trace.evidence_diversity.independent_groups_count === 1 ? "category" : "categories"}
                </h4>
                <p className="text-xs text-slate-500 mt-1 leading-relaxed">
                  {trace.evidence_diversity.explanation}
                </p>
              </div>
              <div className="mt-3 pt-2 border-t border-slate-100 flex flex-wrap gap-1">
                {trace.evidence_diversity.independent_groups.map((grp, idx) => (
                  <span key={idx} className="text-[10px] font-mono bg-teal-50 text-teal-800 border border-teal-200 px-1.5 py-0.5 rounded">
                    {grp}
                  </span>
                ))}
              </div>
            </div>

          </div>

          {/* Context Candidates List */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h4 className="text-sm font-bold text-slate-900 uppercase tracking-wider flex items-center gap-1.5">
                <Scale size={15} className="text-indigo-600" />
                Context Candidates ({trace.candidates.length})
              </h4>
              <span className="text-xs text-slate-400">Sorted by score</span>
            </div>

            {trace.candidates.length === 0 ? (
              <div className="bg-white rounded-xl border border-dashed border-slate-300 p-8 text-center text-slate-500 text-xs">
                No candidate workspaces scored above threshold.
              </div>
            ) : (
              <div className="space-y-4">
                {trace.candidates.map((cand, idx) => (
                  <div
                    key={cand.context_id}
                    className={`bg-white rounded-xl border p-5 shadow-sm space-y-4 transition-all ${
                      idx === 0 ? "border-indigo-200 ring-1 ring-indigo-50" : "border-slate-200"
                    }`}
                  >
                    {/* Candidate Header */}
                    <div className="flex flex-wrap items-center justify-between gap-2 border-b border-slate-100 pb-3">
                      <div>
                        <div className="flex items-center gap-2">
                          <h5 className="text-base font-bold text-slate-900">{cand.context_name}</h5>
                          {idx === 0 && (
                            <span className="text-[10px] bg-indigo-100 text-indigo-800 font-bold px-2 py-0.5 rounded-full">
                              Top Context
                            </span>
                          )}
                        </div>
                        <span className="text-xs font-mono text-slate-400">{cand.context_id}</span>
                      </div>

                      {/* Score & Calibrated Confidence */}
                      <div className="flex items-center gap-4">
                        <div className="text-right">
                          <span className="text-[10px] font-mono uppercase text-slate-400 block">Score</span>
                          <span className="text-lg font-bold font-mono text-slate-900">{cand.score_formatted}</span>
                        </div>
                        <div className="text-right">
                          <span className="text-[10px] font-mono uppercase text-slate-400 block">Confidence</span>
                          <span className={`inline-block text-xs font-semibold px-2.5 py-1 rounded-md border ${getBandBadgeClass(cand.confidence_band)}`}>
                            {getBandDisplayName(cand.confidence_band)} ({cand.confidence_percentage})
                          </span>
                        </div>
                      </div>
                    </div>

                    {/* Evidence & Contradictions Grid */}
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs">
                      
                      {/* Positive Supporting Evidence */}
                      <div className="space-y-2 bg-slate-50/70 p-3 rounded-lg border border-slate-100">
                        <span className="font-bold text-slate-700 uppercase tracking-wider text-[10px] flex items-center gap-1">
                          <ShieldCheck size={12} className="text-emerald-600" />
                          Evidence ({cand.evidence_bullets.length})
                        </span>
                        {cand.evidence_bullets.length === 0 ? (
                          <p className="text-slate-400 italic">No specific positive evidence registered.</p>
                        ) : (
                          <ul className="space-y-1.5 font-mono text-[11px] text-emerald-800">
                            {cand.evidence_bullets.map((bullet, bIdx) => (
                              <li key={bIdx} className="flex items-start gap-1">
                                <span className="font-bold text-emerald-600 select-none">+</span>
                                <span>{bullet.replace(/^\+\s*/, '')}</span>
                              </li>
                            ))}
                          </ul>
                        )}
                      </div>

                      {/* Contradictions & Penalties */}
                      <div className="space-y-2 bg-slate-50/70 p-3 rounded-lg border border-slate-100">
                        <span className="font-bold text-slate-700 uppercase tracking-wider text-[10px] flex items-center gap-1">
                          <AlertTriangle size={12} className="text-amber-600" />
                          Contradictions ({cand.contradiction_bullets.length})
                        </span>
                        {cand.contradiction_bullets.length === 0 ? (
                          <p className="text-slate-400 italic">No contradictory signals detected.</p>
                        ) : (
                          <ul className="space-y-1.5 font-mono text-[11px] text-amber-800">
                            {cand.contradiction_bullets.map((bullet, bIdx) => (
                              <li key={bIdx} className="flex items-start gap-1">
                                <span className="font-bold text-amber-600 select-none">-</span>
                                <span>{bullet.replace(/^-\s*/, '')}</span>
                              </li>
                            ))}
                          </ul>
                        )}
                      </div>

                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {/* ========================================================================= */}
      {/* STAGE 1: RAW EVIDENCE                                                     */}
      {/* ========================================================================= */}
      {activeStage === "raw" && (
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h4 className="text-sm font-bold text-slate-900 uppercase tracking-wider flex items-center gap-1.5">
              <Database size={15} className="text-blue-600" />
              Raw Context Evidence ({trace.raw_evidence.length} items)
            </h4>
            <span className="text-xs text-slate-400">Captured facts before weights/decay</span>
          </div>

          <div className="space-y-2.5">
            {trace.raw_evidence.map(ev => {
              const isExpanded = !!expandedRawItems[ev.id];
              return (
                <div key={ev.id} className="bg-white rounded-lg border border-slate-200 p-3 shadow-sm text-xs space-y-2">
                  <div className="flex items-center justify-between cursor-pointer" onClick={() => toggleRawItem(ev.id)}>
                    <div className="flex items-center gap-2">
                      <span className={`text-[10px] font-mono uppercase px-2 py-0.5 rounded font-bold ${
                        ev.provenance === "observed" ? "bg-green-100 text-green-800" : "bg-purple-100 text-purple-800"
                      }`}>
                        {ev.provenance}
                      </span>
                      <span className="font-mono font-semibold text-slate-800">{ev.category}</span>
                      <span className="text-slate-500 font-medium">— {ev.title}</span>
                    </div>
                    <div className="flex items-center gap-3">
                      <span className="font-mono font-bold text-slate-700">Raw Strength: {ev.raw_strength.toFixed(2)}</span>
                      {isExpanded ? <ChevronDown size={14} className="text-slate-400" /> : <ChevronRight size={14} className="text-slate-400" />}
                    </div>
                  </div>

                  <p className="text-slate-600 leading-relaxed text-[11px] pl-1">
                    {ev.description}
                  </p>

                  {isExpanded && Object.keys(ev.attributes).length > 0 && (
                    <div className="mt-2 p-2.5 bg-slate-50 rounded border border-slate-200 font-mono text-[11px] text-slate-700 space-y-1">
                      <span className="text-[10px] uppercase font-bold text-slate-400 block mb-1">Attributes</span>
                      {Object.entries(ev.attributes).map(([k, v]) => (
                        <div key={k} className="flex">
                          <span className="w-32 text-slate-400 shrink-0">{k}:</span>
                          <span className="text-slate-800 truncate">{JSON.stringify(v)}</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* ========================================================================= */}
      {/* STAGE 2: NORMALIZED EVIDENCE                                              */}
      {/* ========================================================================= */}
      {activeStage === "normalized" && (
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h4 className="text-sm font-bold text-slate-900 uppercase tracking-wider flex items-center gap-1.5">
              <Layers size={15} className="text-purple-600" />
              Normalized Evidence ({trace.normalized_evidence.length} items)
            </h4>
            <span className="text-xs text-slate-400">After temporal decay & anti-dominance caps</span>
          </div>

          <div className="space-y-2.5">
            {trace.normalized_evidence.map((norm, idx) => (
              <div key={idx} className="bg-white rounded-lg border border-slate-200 p-3 shadow-sm text-xs space-y-1.5">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <span className="font-mono font-semibold text-slate-800">{norm.category}</span>
                    {norm.correlation_group && (
                      <span className="text-[10px] font-mono bg-slate-100 text-slate-700 px-1.5 py-0.5 rounded">
                        Group: {norm.correlation_group}
                      </span>
                    )}
                    {norm.is_capped && (
                      <span className="text-[10px] font-mono bg-amber-100 text-amber-800 px-1.5 py-0.5 rounded font-bold">
                        Capped
                      </span>
                    )}
                    {norm.is_conflicting && (
                      <span className="text-[10px] font-mono bg-red-100 text-red-800 px-1.5 py-0.5 rounded font-bold">
                        Conflicting
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-4 font-mono text-[11px]">
                    <span className="text-slate-500">Raw: <b className="text-slate-700">{norm.raw_strength.toFixed(2)}</b></span>
                    <span className="text-slate-500">Norm: <b className="text-slate-700">{norm.normalized_strength.toFixed(2)}</b></span>
                    <span className="text-purple-700 font-bold">Eff. Weight: {norm.effective_weight.toFixed(2)}</span>
                  </div>
                </div>
                {norm.normalization_notes.length > 0 && (
                  <p className="text-slate-500 text-[11px]">{norm.normalization_notes.join("; ")}</p>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* ========================================================================= */}
      {/* STAGE 3: SCORE CONTRIBUTION BREAKDOWN                                     */}
      {/* ========================================================================= */}
      {activeStage === "contributions" && (
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h4 className="text-sm font-bold text-slate-900 uppercase tracking-wider flex items-center gap-1.5">
              <BarChart2 size={15} className="text-indigo-600" />
              Score Contributions per Candidate
            </h4>
          </div>

          {/* Candidate selector pills */}
          <div className="flex flex-wrap gap-2">
            {trace.candidates.map((cand, idx) => (
              <button
                key={cand.context_id}
                onClick={() => setSelectedCandidateIndex(idx)}
                className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                  selectedCandidateIndex === idx
                    ? "bg-indigo-600 text-white shadow-sm"
                    : "bg-white border border-slate-200 text-slate-700 hover:bg-slate-50"
                }`}
              >
                {cand.context_name} (Score: {cand.score_formatted})
              </button>
            ))}
          </div>

          {selectedCandidate && (
            <div className="bg-white rounded-xl border border-slate-200 p-5 shadow-sm space-y-4">
              <div className="flex justify-between items-center border-b border-slate-100 pb-3">
                <h5 className="font-bold text-sm text-slate-900">{selectedCandidate.context_name} Contribution Table</h5>
                <span className="text-xs font-mono font-bold text-indigo-700">Total: {selectedCandidate.score_formatted}</span>
              </div>

              {selectedCandidate.score_contributions.length === 0 ? (
                <p className="text-xs text-slate-400 italic">No score contributions registered.</p>
              ) : (
                <div className="space-y-3">
                  {selectedCandidate.score_contributions.map((contrib, idx) => (
                    <div key={idx} className="space-y-1 text-xs">
                      <div className="flex justify-between items-center font-mono">
                        <span className="text-slate-800 font-semibold">{contrib.description}</span>
                        <div className="flex items-center gap-3">
                          <span className="text-slate-500">Weight: {contrib.weighted_score.toFixed(3)}</span>
                          <span className="text-indigo-600 font-bold">{contrib.percentage_of_score.toFixed(1)}%</span>
                        </div>
                      </div>
                      <div className="w-full bg-slate-100 rounded-full h-2 overflow-hidden">
                        <div
                          className="bg-indigo-600 h-2 rounded-full transition-all"
                          style={{ width: `${Math.min(contrib.percentage_of_score, 100)}%` }}
                        />
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* ========================================================================= */}
      {/* STAGE 4: DIVERSITY & COMPETITION ANALYSIS                                 */}
      {/* ========================================================================= */}
      {activeStage === "diversity" && (
        <div className="space-y-6">
          {/* Diversity Detail */}
          <div className="bg-white rounded-xl border border-slate-200 p-5 shadow-sm space-y-3">
            <h4 className="text-sm font-bold text-slate-900 flex items-center gap-2">
              <BrainCircuit size={16} className="text-teal-600" />
              Evidence Diversity Calculation
            </h4>
            <p className="text-xs text-slate-600 leading-relaxed">
              {trace.evidence_diversity.explanation}
            </p>
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 pt-2 text-xs font-mono">
              <div className="bg-slate-50 p-2.5 rounded border border-slate-200">
                <span className="text-slate-400 block text-[10px]">TOTAL ITEMS</span>
                <span className="font-bold text-slate-800">{trace.evidence_diversity.evidence_quantity}</span>
              </div>
              <div className="bg-slate-50 p-2.5 rounded border border-slate-200">
                <span className="text-slate-400 block text-[10px]">CATEGORIES</span>
                <span className="font-bold text-slate-800">{trace.evidence_diversity.evidence_categories_count}</span>
              </div>
              <div className="bg-slate-50 p-2.5 rounded border border-slate-200">
                <span className="text-slate-400 block text-[10px]">INDEPENDENT GROUPS</span>
                <span className="font-bold text-teal-700">{trace.evidence_diversity.independent_groups_count}</span>
              </div>
              <div className="bg-slate-50 p-2.5 rounded border border-slate-200">
                <span className="text-slate-400 block text-[10px]">MULTIPLIER</span>
                <span className="font-bold text-slate-800">{trace.evidence_diversity.diversity_multiplier.toFixed(2)}x</span>
              </div>
            </div>
          </div>

          {/* Competition Detail */}
          <div className="bg-white rounded-xl border border-slate-200 p-5 shadow-sm space-y-3">
            <h4 className="text-sm font-bold text-slate-900 flex items-center gap-2">
              <Scale size={16} className="text-purple-600" />
              Competition & Ambiguity Analysis
            </h4>
            {trace.competition ? (
              <div className="space-y-3 text-xs">
                <p className="text-slate-600 leading-relaxed">{trace.competition.explanation}</p>
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs font-mono">
                  <div className="bg-slate-50 p-2.5 rounded border border-slate-200">
                    <span className="text-slate-400 block text-[10px]">TOP SCORE</span>
                    <span className="font-bold text-slate-800">{trace.competition.top_score}</span>
                  </div>
                  <div className="bg-slate-50 p-2.5 rounded border border-slate-200">
                    <span className="text-slate-400 block text-[10px]">RUNNER-UP SCORE</span>
                    <span className="font-bold text-slate-800">{trace.competition.runner_up_score}</span>
                  </div>
                  <div className="bg-purple-50 p-2.5 rounded border border-purple-200">
                    <span className="text-purple-600 block text-[10px] font-bold">SCORE MARGIN</span>
                    <span className="font-bold text-purple-900">{trace.competition.score_margin}</span>
                  </div>
                  <div className="bg-slate-50 p-2.5 rounded border border-slate-200">
                    <span className="text-slate-400 block text-[10px]">RELATIVE CONF.</span>
                    <span className="font-bold text-slate-800">{(trace.competition.relative_confidence * 100).toFixed(0)}%</span>
                  </div>
                </div>
              </div>
            ) : (
              <p className="text-xs text-slate-400 italic">No competing contexts to analyze.</p>
            )}
          </div>
        </div>
      )}

      {/* ========================================================================= */}
      {/* STAGE 5: FINAL DECISION                                                   */}
      {/* ========================================================================= */}
      {activeStage === "decision" && (
        <div className="bg-white rounded-xl border border-slate-200 p-6 shadow-sm space-y-4">
          <div className="flex items-center justify-between border-b border-slate-100 pb-3">
            <div className="flex items-center gap-2">
              <CheckCircle2 size={18} className="text-indigo-600" />
              <h4 className="text-sm font-bold text-slate-900">Final Engine Decision</h4>
            </div>
            <span className={`text-xs font-semibold px-2.5 py-1 rounded-md border ${getBandBadgeClass(trace.final_result.confidence_band)}`}>
              {getBandDisplayName(trace.final_result.confidence_band)} ({trace.final_result.confidence_percentage})
            </span>
          </div>

          <div className="space-y-2">
            <h5 className="text-base font-bold text-slate-900">{trace.final_result.title}</h5>
            <p className="text-xs text-slate-600 leading-relaxed">{trace.final_result.summary}</p>
          </div>

          {trace.final_result.no_context_reason && (
            <div className="p-3 bg-slate-50 border border-slate-200 rounded-lg text-xs space-y-1">
              <span className="font-semibold text-slate-700 block">Diagnosed Reason:</span>
              <p className="font-mono text-slate-600">{trace.final_result.no_context_reason}</p>
              {trace.final_result.no_context_explanation && (
                <p className="text-slate-500 text-[11px]">{trace.final_result.no_context_explanation}</p>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
