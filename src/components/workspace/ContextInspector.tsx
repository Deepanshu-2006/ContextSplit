import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { X, Server, Clock, AppWindow, MousePointerClick, Globe, CheckCircle, Network, Search, BrainCircuit, AlertTriangle, Sparkles, HelpCircle } from "lucide-react";
import { db, ContextSignal, Item, WorkspaceScore, ItemRelationship, ItemSemanticStatus, ItemEmbeddingMetadata, UnifiedContextInterpretation, ContextReasoningResult } from "../../services/db";

interface ContextInspectorProps {
  itemId: string;
  onClose: () => void;
}

export function ContextInspector({ itemId, onClose }: ContextInspectorProps) {
  const [item, setItem] = useState<Item | null>(null);
  const [signals, setSignals] = useState<ContextSignal[]>([]);
  const [scores, setScores] = useState<WorkspaceScore[]>([]);
  const [relationships, setRelationships] = useState<ItemRelationship[]>([]);
  const [semanticStatus, setSemanticStatus] = useState<ItemSemanticStatus | null>(null);
  const [embeddingMeta, setEmbeddingMeta] = useState<ItemEmbeddingMetadata | null>(null);
  const [interpretation, setInterpretation] = useState<UnifiedContextInterpretation | null>(null);
  const [aiResult, setAiResult] = useState<ContextReasoningResult | null>(null);
  const [aiLoading, setAiLoading] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function fetchData() {
      try {
        setLoading(true);
        const [fetchedItem, fetchedSignals, fetchedScores, fetchedRels, fetchedSemStatus, fetchedEmbMeta, fetchedInterpretation] = await Promise.all([
          db.getItem(itemId),
          db.listContextSignals(null, itemId),
          db.scoreItem(itemId),
          db.getStrongestRelationships(itemId, 10),
          db.getItemSemanticStatus(itemId),
          db.getItemEmbeddingMetadata(itemId),
          db.evaluateItemContext(itemId).catch(() => null),
        ]);
        setItem(fetchedItem);
        setSignals(fetchedSignals);
        setScores(fetchedScores);
        setRelationships(fetchedRels);
        setSemanticStatus(fetchedSemStatus);
        setEmbeddingMeta(fetchedEmbMeta);
        setInterpretation(fetchedInterpretation);
      } catch (err) {
        console.error("Failed to fetch context data", err);
      } finally {
        setLoading(false);
      }
    }
    fetchData();
  }, [itemId]);

  const handleRunAiReasoning = async () => {
    setAiLoading(true);
    try {
      const result = await db.analyzeItemContextAI(itemId);
      setAiResult(result);
    } catch (err) {
      console.error("Failed to run AI reasoning", err);
    } finally {
      setAiLoading(false);
    }
  };

  const getSignalIcon = (type: string) => {
    switch (type) {
      case 'active_app': return <AppWindow size={14} className="text-blue-500" />;
      case 'user_action': return <MousePointerClick size={14} className="text-purple-500" />;
      case 'url_metadata': return <Globe size={14} className="text-green-500" />;
      case 'suggestion_accepted':
      case 'suggestion_dismissed':
      case 'suggestion_changed': return <CheckCircle size={14} className="text-orange-500" />;
      default: return <Server size={14} className="text-gray-500" />;
    }
  };

  const getSignalTitle = (type: string) => {
    switch (type) {
      case 'active_app': return 'Source Application';
      case 'user_action': return 'User Action';
      case 'url_metadata': return 'URL Metadata';
      case 'suggestion_accepted': return 'Suggestion Accepted';
      case 'suggestion_dismissed': return 'Suggestion Dismissed';
      case 'suggestion_changed': return 'Destination Changed';
      default: return type;
    }
  };

  if (loading) {
    return createPortal(
      <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/20 backdrop-blur-sm p-4">
        <div className="bg-white rounded-xl shadow-2xl w-full max-w-4xl p-10 text-center text-gray-500">
          Loading Context Inspector...
        </div>
      </div>,
      document.body
    );
  }

  const feedbackSignals = signals.filter(s => s.signal_type.startsWith('suggestion_'));
  const captureSignals = signals.filter(s => !s.signal_type.startsWith('suggestion_'));

  return createPortal(
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/20 backdrop-blur-sm p-4">
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-4xl max-h-[90vh] flex flex-col overflow-hidden border border-gray-200" onClick={(e) => e.stopPropagation()}>
        
        <div className="flex items-center justify-between px-5 py-4 border-b border-gray-100 bg-gray-50/50">
          <div>
            <h2 className="text-lg font-semibold text-gray-900 tracking-tight flex items-center gap-2">
              <Search size={18} className="text-blue-500" />
              Context Inspector
            </h2>
            <p className="text-xs text-gray-500 font-mono mt-1">{itemId}</p>
          </div>
          <button 
            onClick={onClose}
            className="p-2 rounded-lg hover:bg-gray-200/50 text-gray-500 transition-colors"
          >
            <X size={18} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-6 bg-gray-50 cursor-default grid grid-cols-1 md:grid-cols-2 gap-6">
          {/* LEFT COLUMN: OBSERVED FACTS */}
          <div className="space-y-6">
            
            {/* Section 1: Basic Info */}
            <div className="bg-white rounded-lg border border-gray-200 shadow-sm overflow-hidden">
              <div className="bg-gray-100 px-4 py-2 border-b border-gray-200 flex items-center justify-between">
                <h3 className="text-sm font-semibold text-gray-800">1. Basic Item Information</h3>
                <span className="text-[10px] uppercase font-bold tracking-wider text-green-700 bg-green-100 px-2 py-0.5 rounded">Observed</span>
              </div>
              <div className="p-4 space-y-2 text-sm">
                <div className="flex justify-between"><span className="text-gray-500">Type</span><span className="font-mono text-gray-900">{item?.item_type}</span></div>
                <div className="flex justify-between"><span className="text-gray-500">Workspace</span><span className="font-mono text-gray-900">{item?.workspace_id || 'Inbox'}</span></div>
                <div className="flex justify-between"><span className="text-gray-500">Created</span><span className="font-mono text-gray-900">{new Date(item?.created_at || '').toLocaleString()}</span></div>
              </div>
            </div>

            {/* Section 2: Semantic Processing */}
            <div className="bg-white rounded-lg border border-gray-200 shadow-sm overflow-hidden">
              <div className="bg-gray-100 px-4 py-2 border-b border-gray-200 flex items-center justify-between">
                <h3 className="text-sm font-semibold text-gray-800">2. Semantic Status</h3>
                <span className="text-[10px] uppercase font-bold tracking-wider text-green-700 bg-green-100 px-2 py-0.5 rounded">Observed</span>
              </div>
              <div className="p-4 space-y-2 text-sm">
                <div className="flex justify-between">
                  <span className="text-gray-500">Status</span>
                  <span className={`font-mono font-medium ${
                    semanticStatus?.status === 'ready' ? 'text-green-600' :
                    semanticStatus?.status === 'failed' ? 'text-red-600' :
                    'text-amber-600'
                  }`}>
                    {semanticStatus?.status || 'unknown'}
                  </span>
                </div>
                {semanticStatus?.status === 'failed' && (
                  <div className="flex justify-between">
                    <span className="text-gray-500">Retries</span>
                    <span className="font-mono text-red-600">{semanticStatus.retries}</span>
                  </div>
                )}
                {embeddingMeta && (
                  <>
                    <div className="flex justify-between"><span className="text-gray-500">Provider</span><span className="font-mono text-gray-900">{embeddingMeta.provider}</span></div>
                    <div className="flex justify-between"><span className="text-gray-500">Model</span><span className="font-mono text-gray-900">{embeddingMeta.model}</span></div>
                    <div className="flex justify-between"><span className="text-gray-500">Dimensions</span><span className="font-mono text-gray-900">{embeddingMeta.dimensions}</span></div>
                  </>
                )}
                {!embeddingMeta && semanticStatus?.status === 'ready' && (
                  <div className="text-xs text-gray-400 italic text-right mt-1">Embedding data missing</div>
                )}
              </div>
            </div>

            {/* Section 3: Capture Signals */}
            <div className="bg-white rounded-lg border border-gray-200 shadow-sm overflow-hidden">
              <div className="bg-gray-100 px-4 py-2 border-b border-gray-200 flex items-center justify-between">
                <h3 className="text-sm font-semibold text-gray-800">3. Capture Signals</h3>
                <span className="text-[10px] uppercase font-bold tracking-wider text-green-700 bg-green-100 px-2 py-0.5 rounded">Observed</span>
              </div>
              <div className="p-4 space-y-3">
                {captureSignals.length === 0 ? (
                  <p className="text-xs text-gray-500 italic">No capture signals recorded.</p>
                ) : (
                  captureSignals.map((signal) => {
                    let payloadObj = null;
                    try { payloadObj = JSON.parse(signal.payload); } catch {}
                    
                    return (
                      <div key={signal.id} className="text-sm border border-gray-100 rounded bg-gray-50 p-2">
                        <div className="flex items-center gap-2 mb-1">
                          {getSignalIcon(signal.signal_type)}
                          <span className="font-medium text-gray-700">{getSignalTitle(signal.signal_type)}</span>
                        </div>
                        {payloadObj ? (
                          <div className="text-xs font-mono text-gray-600 bg-white p-2 rounded border border-gray-200">
                            {Object.entries(payloadObj).map(([k, v]) => (
                              <div key={k} className="flex">
                                <span className="w-24 text-gray-400">{k}:</span>
                                <span className="truncate">{String(v)}</span>
                              </div>
                            ))}
                          </div>
                        ) : (
                          <div className="text-xs font-mono text-gray-600 bg-white p-2 rounded border border-gray-200 truncate">
                            {signal.payload}
                          </div>
                        )}
                      </div>
                    )
                  })
                )}
              </div>
            </div>

            {/* Section 4: User Feedback History */}
            <div className="bg-white rounded-lg border border-gray-200 shadow-sm overflow-hidden">
              <div className="bg-gray-100 px-4 py-2 border-b border-gray-200 flex items-center justify-between">
                <h3 className="text-sm font-semibold text-gray-800">4. User Feedback</h3>
                <span className="text-[10px] uppercase font-bold tracking-wider text-green-700 bg-green-100 px-2 py-0.5 rounded">Observed</span>
              </div>
              <div className="p-4 space-y-2">
                {feedbackSignals.length === 0 ? (
                  <p className="text-xs text-gray-500 italic">No feedback recorded.</p>
                ) : (
                  feedbackSignals.map(sig => (
                    <div key={sig.id} className="text-xs flex items-center justify-between p-2 bg-orange-50 text-orange-800 rounded border border-orange-100">
                      <div className="flex items-center gap-2">
                        <CheckCircle size={12} />
                        <span>{getSignalTitle(sig.signal_type)}</span>
                      </div>
                      <span className="font-mono">{new Date(sig.created_at).toLocaleTimeString()}</span>
                    </div>
                  ))
                )}
              </div>
            </div>

          </div>

          {/* RIGHT COLUMN: INFERRED ENGINE DATA */}
          <div className="space-y-6">
            
            {/* Section 5: Context Session */}
            <div className="bg-white rounded-lg border border-gray-200 shadow-sm overflow-hidden">
              <div className="bg-gray-100 px-4 py-2 border-b border-gray-200 flex items-center justify-between">
                <h3 className="text-sm font-semibold text-gray-800">5. Current Session</h3>
                <span className="text-[10px] uppercase font-bold tracking-wider text-purple-700 bg-purple-100 px-2 py-0.5 rounded">Inferred</span>
              </div>
              <div className="p-4">
                {item?.session_id ? (
                  <div className="text-sm flex items-center gap-2">
                    <Clock size={14} className="text-purple-500" />
                    <span className="font-mono text-gray-800 bg-purple-50 px-2 py-1 rounded border border-purple-100">{item.session_id}</span>
                  </div>
                ) : (
                  <p className="text-xs text-gray-500 italic">No session assigned.</p>
                )}
              </div>
            </div>

            {/* Section 6: Related Items */}
            <div className="bg-white rounded-lg border border-gray-200 shadow-sm overflow-hidden">
              <div className="bg-gray-100 px-4 py-2 border-b border-gray-200 flex items-center justify-between">
                <h3 className="text-sm font-semibold text-gray-800">6. Context Graph Edges</h3>
                <span className="text-[10px] uppercase font-bold tracking-wider text-purple-700 bg-purple-100 px-2 py-0.5 rounded">Inferred</span>
              </div>
              <div className="p-4 space-y-2">
                {relationships.length === 0 ? (
                  <p className="text-xs text-gray-500 italic">No strong relationships detected.</p>
                ) : (
                  relationships.map(rel => {
                    const otherId = rel.item_a_id === itemId ? rel.item_b_id : rel.item_a_id;
                    const percent = Math.round(rel.strength * 100);
                    const isSemantic = rel.relationship_type === 'semantic_similarity';
                    return (
                      <div key={rel.id} className={`text-xs p-2 border rounded flex flex-col gap-1.5 ${isSemantic ? 'bg-cyan-50 border-cyan-100' : 'bg-gray-50 border-gray-100'}`}>
                        <div className="flex justify-between items-center">
                          <div className="flex items-center gap-1.5 text-gray-700 font-mono">
                            {isSemantic ? <BrainCircuit size={12} className="text-cyan-500" /> : <Network size={12} className="text-blue-500" />}
                            {otherId}
                          </div>
                          <span className={`font-semibold ${isSemantic ? 'text-cyan-600' : 'text-blue-600'}`}>{percent}% match</span>
                        </div>
                        <div className="w-full bg-gray-200 rounded-full h-1.5">
                          <div className={`h-1.5 rounded-full ${isSemantic ? 'bg-cyan-500' : 'bg-blue-500'}`} style={{ width: `${percent}%` }}></div>
                        </div>
                        <span className="text-gray-400 capitalize">Signal: {rel.relationship_type.replace(/_/g, ' ')}</span>
                      </div>
                    )
                  })
                )}
              </div>
            </div>

            {/* Section 7 & 8: Workspace Scores and Explanation */}
            <div className="bg-white rounded-lg border border-gray-200 shadow-sm overflow-hidden">
              <div className="bg-gray-100 px-4 py-2 border-b border-gray-200 flex items-center justify-between">
                <h3 className="text-sm font-semibold text-gray-800">7 & 8. Workspace Scoring</h3>
                <span className="text-[10px] uppercase font-bold tracking-wider text-purple-700 bg-purple-100 px-2 py-0.5 rounded">Inferred</span>
              </div>
              <div className="p-4 space-y-3">
                {/* Confidence Assessment Banner */}
                {interpretation && (
                  <div className={`p-3 rounded-lg border text-xs flex flex-col gap-1 ${
                    interpretation.primary_interpretation === "ambiguous"
                      ? "bg-amber-50 border-amber-200 text-amber-900"
                      : interpretation.primary_interpretation === "unassigned"
                      ? "bg-slate-50 border-slate-200 text-slate-700"
                      : (interpretation.existing_workspaces[0]?.confidence || 0) >= 0.70
                      ? "bg-emerald-50 border-emerald-200 text-emerald-900"
                      : "bg-blue-50 border-blue-200 text-blue-900"
                  }`}>
                    <div className="flex items-center gap-1.5 font-semibold">
                      {interpretation.primary_interpretation === "ambiguous" ? (
                        <>
                          <AlertTriangle size={14} className="text-amber-600 shrink-0" />
                          <span>Uncertain Context (Ambiguous)</span>
                        </>
                      ) : interpretation.primary_interpretation === "unassigned" ? (
                        <>
                          <HelpCircle size={14} className="text-slate-500 shrink-0" />
                          <span>No Meaningful Context (Weak Signals)</span>
                        </>
                      ) : (interpretation.existing_workspaces[0]?.confidence || 0) >= 0.70 ? (
                        <>
                          <Sparkles size={14} className="text-emerald-600 shrink-0" />
                          <span>High Confidence Match</span>
                        </>
                      ) : (
                        <>
                          <Sparkles size={14} className="text-blue-600 shrink-0" />
                          <span>Moderate Confidence Match</span>
                        </>
                      )}
                    </div>
                    <p className="text-[11px] leading-relaxed opacity-90">{interpretation.summary}</p>
                    {interpretation.primary_interpretation === "ambiguous" && (
                      <p className="text-[10px] font-medium text-amber-700 mt-0.5">
                        Item is NOT forced into any workspace. Remains safely in Inbox.
                      </p>
                    )}
                  </div>
                )}

                {scores.length === 0 ? (
                  <p className="text-xs text-gray-500 italic">No scores calculated.</p>
                ) : (
                  scores.sort((a,b) => b.confidence - a.confidence).map(score => {
                    const conf = Math.round(score.confidence * 100);
                    return (
                      <div key={score.workspace_id} className="border border-indigo-100 bg-indigo-50/30 rounded p-3">
                        <div className="flex justify-between items-center mb-2">
                          <span className="font-semibold text-indigo-900 text-sm">{score.workspace_id}</span>
                          <span className={`font-bold text-sm ${conf >= 30 ? 'text-indigo-600' : 'text-gray-400'}`}>{conf}%</span>
                        </div>
                        {score.reasons.length > 0 ? (
                          <ul className="text-xs space-y-1 mt-2 text-gray-600">
                            {score.reasons.map((reason, idx) => {
                              const isSemanticReason = reason.includes("semantic") || reason.includes("Semantic");
                              return (
                                <li key={idx} className="flex gap-1.5 items-start">
                                  {isSemanticReason ? (
                                    <BrainCircuit size={12} className="text-cyan-500 mt-0.5 shrink-0" />
                                  ) : (
                                    <span className="text-indigo-400 shrink-0">•</span>
                                  )}
                                  <span className={isSemanticReason ? "text-cyan-800" : ""}>{reason}</span>
                                </li>
                              );
                            })}
                          </ul>
                        ) : (
                          <p className="text-xs text-gray-400 italic">No specific reasons provided.</p>
                        )}
                      </div>
                    )
                  })
                )}
              </div>
            </div>

            {/* Section 9: AI Context Reasoning */}
            <div className="bg-white rounded-lg border border-gray-200 shadow-sm overflow-hidden">
              <div className="bg-gray-100 px-4 py-2 border-b border-gray-200 flex items-center justify-between">
                <h3 className="text-sm font-semibold text-gray-800 flex items-center gap-1.5">
                  <Sparkles size={14} className="text-purple-600" />
                  9. AI Context Reasoning
                </h3>
                <span className="text-[10px] uppercase font-bold tracking-wider text-purple-700 bg-purple-100 px-2 py-0.5 rounded">
                  Privacy Preserved
                </span>
              </div>
              <div className="p-4 space-y-3">
                {!aiResult ? (
                  <div className="text-center py-4 space-y-2">
                    <p className="text-xs text-gray-500">
                      Query the isolated AI Reasoner with minimal EvidencePackage to analyze context and explanations.
                    </p>
                    <button
                      onClick={handleRunAiReasoning}
                      disabled={aiLoading}
                      className="inline-flex items-center gap-1.5 px-3 py-1.5 bg-purple-600 hover:bg-purple-700 text-white rounded text-xs font-medium transition disabled:opacity-50"
                    >
                      <Sparkles size={12} />
                      {aiLoading ? "Analyzing Evidence..." : "Run AI Context Reasoning"}
                    </button>
                  </div>
                ) : (
                  <div className="space-y-3">
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-medium text-gray-500">Decision:</span>
                        <span className={`text-xs px-2 py-0.5 rounded font-semibold capitalize ${
                          aiResult.decision === "existing_context"
                            ? "bg-emerald-100 text-emerald-800 border border-emerald-200"
                            : aiResult.decision === "new_context"
                            ? "bg-purple-100 text-purple-800 border border-purple-200"
                            : aiResult.decision === "uncertain"
                            ? "bg-amber-100 text-amber-800 border border-amber-200"
                            : "bg-slate-100 text-slate-700 border border-slate-200"
                        }`}>
                          {aiResult.decision.replace(/_/g, " ")}
                        </span>
                      </div>
                      <span className="text-xs font-bold text-gray-700">
                        {Math.round(aiResult.confidence * 100)}% confidence
                      </span>
                    </div>

                    {aiResult.suggested_name && (
                      <div className="text-xs flex items-center justify-between bg-gray-50 p-2 rounded border border-gray-100">
                        <span className="text-gray-500">Suggested Context:</span>
                        <span className="font-semibold text-gray-800">{aiResult.suggested_name}</span>
                      </div>
                    )}

                    {aiResult.uncertainty_reason && (
                      <div className="text-xs bg-amber-50 border border-amber-200 text-amber-900 p-2.5 rounded flex items-start gap-1.5">
                        <AlertTriangle size={13} className="text-amber-600 mt-0.5 shrink-0" />
                        <div>
                          <span className="font-semibold block">Uncertainty Notice:</span>
                          <span className="text-[11px]">{aiResult.uncertainty_reason}</span>
                        </div>
                      </div>
                    )}

                    <div className="text-xs bg-gray-50 border border-gray-100 rounded p-2.5 space-y-1">
                      <span className="font-medium text-gray-600 block">Explanation (Evidence Grounded):</span>
                      <p className="text-gray-800 italic leading-relaxed">"{aiResult.explanation}"</p>
                    </div>

                    {aiResult.alternative_interpretations && aiResult.alternative_interpretations.length > 0 && (
                      <div className="text-xs space-y-1">
                        <span className="text-gray-500 font-medium">Alternative Interpretations:</span>
                        <ul className="list-disc list-inside text-gray-600 space-y-1 pl-1">
                          {aiResult.alternative_interpretations.map((alt, idx) => (
                            <li key={idx}>
                              <span className="font-medium text-gray-800">{alt.context_name}</span>{" "}
                              <span className="text-gray-400 font-mono">({Math.round(alt.confidence * 100)}%)</span>:{" "}
                              <span>{alt.rationale}</span>
                            </li>
                          ))}
                        </ul>
                      </div>
                    )}

                    <button
                      onClick={handleRunAiReasoning}
                      disabled={aiLoading}
                      className="text-xs text-purple-600 hover:text-purple-700 font-medium flex items-center gap-1 pt-1"
                    >
                      <Sparkles size={11} /> Re-run AI Analysis
                    </button>
                  </div>
                )}
              </div>
            </div>

          </div>
        </div>
      </div>
    </div>,
    document.body
  );
}
