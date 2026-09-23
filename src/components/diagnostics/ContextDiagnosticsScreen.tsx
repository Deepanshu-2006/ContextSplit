import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  RefreshCw,
  Trash2,
  Activity,
  GitBranch,
  ShieldCheck,
  Zap,
  Clock,
  Layers,
  FileText,
  Brain,
  MessageSquare,
  Camera,
  CheckCircle2,
  Cpu,
  Search,
  Sliders,
} from 'lucide-react';
import {
  diagnosticsService,
  DiagnosticsSafetyStatus,
  ContextDecisionTrace,
  ContextDecisionExplanation,
  ContextLifecycleTransitionExplanation,
  ContextMemoryProvenanceExplanation,
  ContextFeedbackEffectExplanation,
  ContextHealthReport,
  CrossSystemConsistencyReport,
  PerformanceDiagnosticsReport,
  PerformanceSpan,
  TaskCandidate,
} from '../../services/diagnosticsService';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { contextReconstructionService, ReconstructionSnapshotRef } from '../../services/contextReconstruction';
import { toast } from 'sonner';

interface ContextDiagnosticsScreenProps {
  onClose: () => void;
}

type TabType =
  | 'contexts'
  | 'timeline'
  | 'decisions'
  | 'evidence'
  | 'lifecycle'
  | 'tasks'
  | 'memories'
  | 'feedback'
  | 'snapshots'
  | 'health'
  | 'performance';

export function ContextDiagnosticsScreen({ onClose }: ContextDiagnosticsScreenProps) {
  const { workspaces, activeWorkspaceId } = useWorkspaceStore();

  const [activeTab, setActiveTab] = useState<TabType>('decisions');
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string>(
    activeWorkspaceId && activeWorkspaceId !== 'inbox' ? activeWorkspaceId : workspaces[0]?.id || ''
  );

  const [safetyStatus, setSafetyStatus] = useState<DiagnosticsSafetyStatus | null>(null);
  const [traces, setTraces] = useState<ContextDecisionTrace[]>([]);
  const [selectedTrace, setSelectedTrace] = useState<ContextDecisionTrace | null>(null);
  const [selectedTraceExplanation, setSelectedTraceExplanation] = useState<ContextDecisionExplanation | null>(null);
  const [traceFilter, setTraceFilter] = useState<string>('all');
  const [searchQuery, setSearchQuery] = useState<string>('');

  const [lifecycleExp, setLifecycleExp] = useState<ContextLifecycleTransitionExplanation | null>(null);
  const [memoriesExp, setMemoriesExp] = useState<ContextMemoryProvenanceExplanation[]>([]);
  const [feedbackExp, setFeedbackExp] = useState<ContextFeedbackEffectExplanation | null>(null);
  const [tasks, setTasks] = useState<TaskCandidate[]>([]);
  const [snapshot, setSnapshot] = useState<ReconstructionSnapshotRef | null>(null);

  const [healthReport, setHealthReport] = useState<ContextHealthReport | null>(null);
  const [consistencyReport, setConsistencyReport] = useState<CrossSystemConsistencyReport | null>(null);
  const [perfReport, setPerfReport] = useState<PerformanceDiagnosticsReport | null>(null);
  const [recentSpans, setRecentSpans] = useState<PerformanceSpan[]>([]);

  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [isHealthRunning, setIsHealthRunning] = useState<boolean>(false);

  // Fetch telemetry and active tab data
  const loadDiagnostics = async () => {
    setIsLoading(true);
    try {
      const status = await diagnosticsService.getSafetyStatus();
      setSafetyStatus(status);

      const recentTraces = await diagnosticsService.getRecentDecisionTraces(100);
      setTraces(recentTraces);

      if (selectedWorkspaceId) {
        try {
          const lExp = await diagnosticsService.explainWorkspaceLifecycle(selectedWorkspaceId);
          setLifecycleExp(lExp);
        } catch (_) {}

        try {
          const mExp = await diagnosticsService.explainWorkspaceMemories(selectedWorkspaceId);
          setMemoriesExp(mExp);
        } catch (_) {}

        try {
          const fb = await diagnosticsService.explainWorkspaceFeedback(selectedWorkspaceId);
          setFeedbackExp(fb);
        } catch (_) {}

        try {
          const tList = await diagnosticsService.listAllTaskCandidates(selectedWorkspaceId);
          setTasks(tList);
        } catch (_) {}

        try {
          const recon = await contextReconstructionService.getContextReconstruction(selectedWorkspaceId);
          setSnapshot(recon.latest_snapshot?.snapshot || null);
        } catch (_) {}
      }

      const pReport = await diagnosticsService.getPerformanceTracesSummary();
      setPerfReport(pReport);

      const spans = await diagnosticsService.getRecentPerformanceSpans(100);
      setRecentSpans(spans);
    } catch (err: any) {
      console.error('Failed to load diagnostics:', err);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadDiagnostics();
  }, [selectedWorkspaceId]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  const handleSelectTrace = async (trace: ContextDecisionTrace) => {
    setSelectedTrace(trace);
    try {
      const exp = await diagnosticsService.explainDecisionTrace(trace.decision_id);
      setSelectedTraceExplanation(exp);
    } catch (_) {
      setSelectedTraceExplanation(null);
    }
  };

  const handlePurge = async () => {
    try {
      await diagnosticsService.purgeAllDiagnostics();
      toast.success('In-memory diagnostics buffers wiped');
      loadDiagnostics();
      setSelectedTrace(null);
      setSelectedTraceExplanation(null);
    } catch (err) {
      toast.error('Failed to purge diagnostics');
    }
  };

  const handleRunHealthCheck = async () => {
    setIsHealthRunning(true);
    try {
      const [hRep, cRep] = await Promise.all([
        diagnosticsService.runContextHealthChecks(),
        diagnosticsService.runCrossSystemConsistencyChecks(),
      ]);
      setHealthReport(hRep);
      setConsistencyReport(cRep);
      toast.success('Health and consistency checks executed');
    } catch (err: any) {
      toast.error('Failed running health checks: ' + err);
    } finally {
      setIsHealthRunning(false);
    }
  };

  const filteredTraces = traces.filter((t) => {
    const matchesType = traceFilter === 'all' || t.decision_type === traceFilter;
    const matchesSearch =
      !searchQuery ||
      t.decision_id.toLowerCase().includes(searchQuery.toLowerCase()) ||
      t.reason.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (t.final_decision.primary_context_name || '').toLowerCase().includes(searchQuery.toLowerCase());
    const matchesWs =
      !selectedWorkspaceId ||
      t.final_decision.primary_context_id === selectedWorkspaceId ||
      t.source_entity.entity_id.includes(selectedWorkspaceId);
    return matchesType && matchesSearch && (traceFilter !== 'all' || matchesWs);
  });

  const selectedWorkspace = workspaces.find((w) => w.id === selectedWorkspaceId);

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-gray-950/80 backdrop-blur-md p-3 sm:p-6 font-mono text-xs animate-in fade-in duration-150">
      <div className="fixed inset-0" onClick={onClose} />

      <div
        className="relative w-full max-w-7xl h-[92vh] bg-gray-900 text-gray-200 rounded-2xl shadow-2xl border border-gray-800 flex flex-col overflow-hidden z-10"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Top Header */}
        <div className="px-5 py-3.5 border-b border-gray-800 flex items-center justify-between bg-gray-950/80">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-indigo-500/10 border border-indigo-500/20 text-indigo-400">
              <Cpu className="w-5 h-5" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <span className="font-semibold text-sm text-gray-100 font-sans tracking-tight">
                  ContextSplit Developer Diagnostics
                </span>
                <span className="px-2 py-0.5 rounded-full text-[10px] font-medium bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                  DEV-ONLY
                </span>
                <span className="px-2 py-0.5 rounded-full text-[10px] font-medium bg-gray-800 text-gray-300">
                  {safetyStatus?.mode === 'local_dev' ? 'LocalDev Active' : 'Production Safe'}
                </span>
              </div>
              <p className="text-[11px] text-gray-400 font-sans">
                Real-time contextual engine observability, decision traces, and grounded explainability
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {/* Workspace selector */}
            <div className="flex items-center gap-1.5 bg-gray-800/80 border border-gray-700/60 rounded-lg px-2.5 py-1">
              <span className="text-gray-400 text-[11px]">Context:</span>
              <select
                value={selectedWorkspaceId}
                onChange={(e) => setSelectedWorkspaceId(e.target.value)}
                className="bg-transparent text-gray-200 text-[11px] font-medium focus:outline-hidden cursor-pointer"
              >
                {workspaces.map((w) => (
                  <option key={w.id} value={w.id} className="bg-gray-900 text-gray-200">
                    {w.name} {w.id === activeWorkspaceId ? '★ (Active)' : ''}
                  </option>
                ))}
              </select>
            </div>

            <button
              onClick={handlePurge}
              title="Purge all in-memory diagnostic traces"
              className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-rose-500/10 text-rose-300 hover:bg-rose-500/20 border border-rose-500/20 transition-colors"
            >
              <Trash2 className="w-3.5 h-3.5" />
              <span>Purge</span>
            </button>

            <button
              onClick={loadDiagnostics}
              title="Refresh telemetry"
              className="p-1.5 rounded-lg bg-gray-800 hover:bg-gray-700 text-gray-300 border border-gray-700 transition-colors"
            >
              <RefreshCw className={`w-4 h-4 ${isLoading ? 'animate-spin' : ''}`} />
            </button>

            <button
              onClick={onClose}
              className="p-1.5 rounded-lg bg-gray-800 hover:bg-gray-700 text-gray-300 border border-gray-700 transition-colors"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* Tab Navigation */}
        <div className="flex items-center px-4 border-b border-gray-800 bg-gray-950/40 overflow-x-auto gap-1 py-1.5">
          {(
            [
              { id: 'decisions', label: 'Decision Trace', icon: GitBranch },
              { id: 'contexts', label: 'Contexts', icon: Layers },
              { id: 'timeline', label: 'Context Timeline', icon: Clock },
              { id: 'evidence', label: 'Evidence', icon: Activity },
              { id: 'lifecycle', label: 'Lifecycle', icon: Zap },
              { id: 'tasks', label: 'Tasks', icon: FileText },
              { id: 'memories', label: 'Memories', icon: Brain },
              { id: 'feedback', label: 'Feedback', icon: MessageSquare },
              { id: 'snapshots', label: 'Snapshots', icon: Camera },
              { id: 'health', label: 'Health Checks', icon: ShieldCheck },
              { id: 'performance', label: 'Performance', icon: Sliders },
            ] as const
          ).map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md font-sans text-xs font-medium whitespace-nowrap transition-colors ${
                  isActive
                    ? 'bg-indigo-600 text-white shadow-xs'
                    : 'text-gray-400 hover:text-gray-200 hover:bg-gray-800/60'
                }`}
              >
                <Icon className="w-3.5 h-3.5" />
                <span>{tab.label}</span>
              </button>
            );
          })}
        </div>

        {/* Main Diagnostic Workspace */}
        <div className="flex-1 overflow-y-auto p-5 bg-gray-900">
          {/* TAB 1: DECISION TRACE */}
          {activeTab === 'decisions' && (
            <div className="grid grid-cols-1 lg:grid-cols-12 gap-5 h-full">
              {/* Left Column: Trace List & Filter */}
              <div className="lg:col-span-5 flex flex-col gap-3">
                <div className="flex items-center gap-2">
                  <div className="relative flex-1">
                    <Search className="w-3.5 h-3.5 absolute left-2.5 top-2.5 text-gray-500" />
                    <input
                      type="text"
                      placeholder="Search traces by ID, context, reason..."
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                      className="w-full pl-8 pr-3 py-1.5 bg-gray-950 border border-gray-800 rounded-lg text-xs text-gray-200 focus:outline-hidden focus:border-indigo-500 font-sans"
                    />
                  </div>
                  <select
                    value={traceFilter}
                    onChange={(e) => setTraceFilter(e.target.value)}
                    className="bg-gray-950 border border-gray-800 rounded-lg px-2.5 py-1.5 text-xs text-gray-300 font-sans focus:outline-hidden cursor-pointer"
                  >
                    <option value="all">All Types</option>
                    <option value="context_assignment">Assignment</option>
                    <option value="context_rejection">Rejection</option>
                    <option value="context_transition">Transition</option>
                    <option value="task_assignment">Task</option>
                    <option value="memory_creation">Memory</option>
                    <option value="feedback_adaptation">Feedback</option>
                    <option value="lifecycle_transition">Lifecycle</option>
                    <option value="snapshot_creation">Snapshot</option>
                    <option value="resume_readiness">Resume</option>
                  </select>
                </div>

                <div className="flex-1 overflow-y-auto space-y-2 pr-1 max-h-[68vh]">
                  {filteredTraces.length === 0 ? (
                    <div className="p-8 text-center border border-dashed border-gray-800 rounded-xl text-gray-500 font-sans">
                      No decision traces match active filters.
                    </div>
                  ) : (
                    filteredTraces.map((trace) => {
                      const isSelected = selectedTrace?.decision_id === trace.decision_id;
                      return (
                        <div
                          key={trace.decision_id}
                          onClick={() => handleSelectTrace(trace)}
                          className={`p-3 rounded-xl border transition-all cursor-pointer ${
                            isSelected
                              ? 'bg-indigo-950/40 border-indigo-500/50 shadow-sm'
                              : 'bg-gray-950/60 border-gray-800/80 hover:border-gray-700 hover:bg-gray-950'
                          }`}
                        >
                          <div className="flex items-center justify-between gap-2 mb-1.5">
                            <span className="font-semibold text-xs text-indigo-300 font-mono">
                              {trace.decision_type}
                            </span>
                            <span className="text-[10px] text-gray-500">
                              {new Date(trace.timestamp).toLocaleTimeString()}
                            </span>
                          </div>
                          <p className="text-[11px] text-gray-300 line-clamp-2 font-sans mb-2">
                            {trace.reason || trace.final_decision.summary_verdict}
                          </p>
                          <div className="flex items-center justify-between text-[10px] text-gray-500 font-sans pt-1 border-t border-gray-800/50">
                            <span>
                              Context: <strong className="text-gray-300">{trace.final_decision.primary_context_name || 'N/A'}</strong>
                            </span>
                            <span>{trace.evidence_considered.length} signals • {trace.processing_duration_ms.toFixed(1)}ms</span>
                          </div>
                        </div>
                      );
                    })
                  )}
                </div>
              </div>

              {/* Right Column: Trace Detail & Grounded Explanation */}
              <div className="lg:col-span-7 bg-gray-950/90 border border-gray-800 rounded-xl p-5 overflow-y-auto max-h-[75vh]">
                {selectedTrace ? (
                  <div className="space-y-4">
                    {/* Header */}
                    <div className="border-b border-gray-800 pb-3">
                      <div className="flex items-center justify-between">
                        <h3 className="text-sm font-semibold text-gray-100 font-sans">
                          {selectedTraceExplanation?.headline || selectedTrace.decision_type.toUpperCase()}
                        </h3>
                        <span className="px-2 py-0.5 rounded-full text-[10px] bg-indigo-500/10 text-indigo-400 border border-indigo-500/20">
                          {selectedTrace.final_decision.status}
                        </span>
                      </div>
                      <p className="text-[11px] text-gray-400 font-mono mt-1">ID: {selectedTrace.decision_id}</p>
                    </div>

                    {/* Summary Paragraph */}
                    {selectedTraceExplanation && (
                      <div className="p-3 bg-gray-900 rounded-lg border border-gray-800 text-gray-300 text-xs font-sans">
                        {selectedTraceExplanation.summary_paragraph}
                      </div>
                    )}

                    {/* Primary Evidence */}
                    <div>
                      <h4 className="text-xs font-semibold text-gray-300 font-sans mb-1.5 flex items-center gap-1.5">
                        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                        Primary Grounded Evidence
                      </h4>
                      <div className="space-y-1.5">
                        {selectedTrace.evidence_considered.map((ev, i) => (
                          <div key={i} className="p-2.5 bg-gray-900/80 rounded-lg border border-gray-800 text-[11px]">
                            <div className="flex items-center justify-between text-indigo-300 font-medium mb-1">
                              <span>{ev.evidence_name}</span>
                              <span className="text-gray-400 font-mono">
                                weight: {ev.weight.toFixed(2)} | str: {ev.strength.toFixed(2)}
                              </span>
                            </div>
                            <p className="text-gray-300 font-sans">{ev.explanation}</p>
                            {ev.source_file && (
                              <p className="text-[10px] text-gray-500 font-mono mt-1 truncate">File: {ev.source_file}</p>
                            )}
                          </div>
                        ))}
                      </div>
                    </div>

                    {/* Final Verdict */}
                    <div className="p-3 bg-indigo-950/20 border border-indigo-500/20 rounded-lg text-xs font-sans">
                      <strong className="text-indigo-300">Final Verdict:</strong> {selectedTrace.final_decision.summary_verdict}
                    </div>
                  </div>
                ) : (
                  <div className="h-full flex flex-col items-center justify-center text-gray-500 font-sans p-10">
                    <GitBranch className="w-8 h-8 text-gray-700 mb-2" />
                    <p>Select a decision trace on the left to inspect full grounded provenance and diagnostic logs.</p>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* TAB 2: CONTEXTS */}
          {activeTab === 'contexts' && (
            <div className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                {workspaces.map((ws) => (
                  <div
                    key={ws.id}
                    onClick={() => setSelectedWorkspaceId(ws.id)}
                    className={`p-4 rounded-xl border transition-all cursor-pointer ${
                      selectedWorkspaceId === ws.id
                        ? 'bg-indigo-950/30 border-indigo-500/40 shadow-sm'
                        : 'bg-gray-950 border-gray-800 hover:border-gray-700'
                    }`}
                  >
                    <div className="flex items-center justify-between mb-2">
                      <span className="font-semibold text-sm text-gray-100 font-sans">{ws.name}</span>
                      {ws.id === activeWorkspaceId && (
                        <span className="px-2 py-0.5 rounded-full text-[10px] bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                          Active
                        </span>
                      )}
                    </div>
                    <div className="space-y-1 text-[11px] text-gray-400 font-mono">
                      <div>ID: {ws.id}</div>
                      <div>Created: {new Date(ws.created_at).toLocaleDateString()}</div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* TAB 3: CONTEXT TIMELINE */}
          {activeTab === 'timeline' && (
            <div className="space-y-4">
              <div className="p-4 bg-gray-950 border border-gray-800 rounded-xl">
                <h3 className="text-sm font-semibold text-gray-200 font-sans mb-2">Timeline Segments for {selectedWorkspace?.name}</h3>
                <p className="text-xs text-gray-400 font-sans mb-4">
                  Traces continuous focus periods, working sessions, and context returns.
                </p>
                <div className="space-y-2">
                  <div className="p-3 bg-gray-900 border border-gray-800 rounded-lg flex items-center justify-between text-xs font-sans">
                    <div className="flex items-center gap-2">
                      <Clock className="w-4 h-4 text-indigo-400" />
                      <span>Continuous Focus Window: <strong>Active Session</strong></span>
                    </div>
                    <span className="text-gray-400">Duration: ~45 mins</span>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* TAB 4: EVIDENCE */}
          {activeTab === 'evidence' && (
            <div className="space-y-4">
              <div className="p-4 bg-gray-950 border border-gray-800 rounded-xl">
                <h3 className="text-sm font-semibold text-gray-200 font-sans mb-2">Contextual Evidence Breakdown</h3>
                <p className="text-xs text-gray-400 font-sans mb-4">
                  Aggregated evidence signals currently active for context <strong>{selectedWorkspace?.name}</strong>.
                </p>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  <div className="p-3 bg-gray-900 border border-gray-800 rounded-lg">
                    <h4 className="text-xs font-semibold text-indigo-300 font-sans mb-1">Source Repository & File Paths</h4>
                    <p className="text-[11px] text-gray-400 font-sans">High-affinity file grounding matching active repository root.</p>
                  </div>
                  <div className="p-3 bg-gray-900 border border-gray-800 rounded-lg">
                    <h4 className="text-xs font-semibold text-indigo-300 font-sans mb-1">Domain Classification</h4>
                    <p className="text-[11px] text-gray-400 font-sans">Project-relevant technical documentation and APIs.</p>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* TAB 5: LIFECYCLE */}
          {activeTab === 'lifecycle' && (
            <div className="space-y-4">
              {lifecycleExp ? (
                <div className="p-5 bg-gray-950 border border-gray-800 rounded-xl space-y-3 font-sans">
                  <div className="flex items-center justify-between border-b border-gray-800 pb-3">
                    <h3 className="text-sm font-semibold text-gray-100">
                      Lifecycle Diagnostic: {lifecycleExp.context_name}
                    </h3>
                    <div className="flex items-center gap-2">
                      <span className="px-2 py-0.5 rounded text-[10px] bg-gray-800 text-gray-300 font-mono">
                        {lifecycleExp.previous_state} → {lifecycleExp.current_state}
                      </span>
                    </div>
                  </div>
                  <div className="space-y-2 text-xs">
                    <div className="text-indigo-300 font-semibold">Reason:</div>
                    <p className="text-gray-300">{lifecycleExp.transition_reason}</p>
                    <div className="text-indigo-300 font-semibold mt-3">Grounded Indicators:</div>
                    <ul className="list-disc list-inside text-gray-300 space-y-1">
                      {lifecycleExp.why_transitioned.map((w, idx) => (
                        <li key={idx}>{w}</li>
                      ))}
                    </ul>
                  </div>
                </div>
              ) : (
                <div className="p-8 text-center text-gray-500 font-sans">No lifecycle explanation available for selected context.</div>
              )}
            </div>
          )}

          {/* TAB 6: TASKS */}
          {activeTab === 'tasks' && (
            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-gray-200 font-sans">
                Inferred & Active Tasks for {selectedWorkspace?.name} ({tasks.length})
              </h3>
              {tasks.length === 0 ? (
                <div className="p-8 text-center border border-dashed border-gray-800 rounded-xl text-gray-500 font-sans">
                  No task candidates recorded for this context.
                </div>
              ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  {tasks.map((task) => (
                    <div key={task.id} className="p-3.5 bg-gray-950 border border-gray-800 rounded-xl font-sans">
                      <div className="flex items-center justify-between mb-1.5">
                        <span className="font-semibold text-xs text-gray-100">{task.title}</span>
                        <span className="px-2 py-0.5 rounded-full text-[10px] bg-indigo-500/10 text-indigo-400 border border-indigo-500/20 font-mono">
                          {task.status}
                        </span>
                      </div>
                      <div className="flex items-center justify-between text-[10px] text-gray-500 font-mono">
                        <span>Confidence: {(task.confidence * 100).toFixed(0)}%</span>
                        <span>Detected: {new Date(task.first_detected_at).toLocaleDateString()}</span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* TAB 7: MEMORIES */}
          {activeTab === 'memories' && (
            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-gray-200 font-sans">
                Context Memories & Grounded Provenance for {selectedWorkspace?.name} ({memoriesExp.length})
              </h3>
              {memoriesExp.length === 0 ? (
                <div className="p-8 text-center border border-dashed border-gray-800 rounded-xl text-gray-500 font-sans">
                  No important memories stored for this context.
                </div>
              ) : (
                <div className="space-y-3">
                  {memoriesExp.map((mem) => (
                    <div key={mem.memory_id} className="p-4 bg-gray-950 border border-gray-800 rounded-xl font-sans space-y-2">
                      <div className="flex items-center justify-between">
                        <span className="font-semibold text-xs text-indigo-300">{mem.memory_statement}</span>
                        <span className="px-2 py-0.5 rounded-full text-[10px] bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-mono">
                          Source: {mem.source}
                        </span>
                      </div>
                      <div className="text-xs text-gray-400">
                        <span className="font-semibold text-gray-300">Why stored:</span>
                        <ul className="list-disc list-inside text-gray-300 mt-1 space-y-0.5">
                          {mem.why_stored.map((ws, idx) => (
                            <li key={idx}>{ws}</li>
                          ))}
                        </ul>
                      </div>
                      <div className="text-[10px] text-gray-500 font-mono pt-2 border-t border-gray-800/60 flex items-center justify-between">
                        <span>Importance: {mem.importance_band} ({(mem.importance_score * 100).toFixed(0)}%)</span>
                        <span>Confidence: {(mem.confidence * 100).toFixed(0)}%</span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* TAB 8: FEEDBACK */}
          {activeTab === 'feedback' && (
            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-gray-200 font-sans">
                Feedback Adaptation Diagnostics for {selectedWorkspace?.name}
              </h3>
              {feedbackExp ? (
                <div className="p-4 bg-gray-950 border border-gray-800 rounded-xl font-sans space-y-2">
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-xs text-gray-100">Target: {feedbackExp.target_label}</span>
                    <span className="px-2 py-0.5 rounded-full text-[10px] bg-indigo-500/10 text-indigo-400 font-mono">
                      Tier: {feedbackExp.precedence_tier_name}
                    </span>
                  </div>
                  <div className="text-xs text-gray-300">
                    <div><strong>Effective State:</strong> {feedbackExp.effective_state}</div>
                    <div><strong>Final Decision:</strong> {feedbackExp.final_effective_decision}</div>
                  </div>
                </div>
              ) : (
                <div className="p-8 text-center text-gray-500 font-sans">No feedback events recorded for this target.</div>
              )}
            </div>
          )}

          {/* TAB 9: SNAPSHOTS */}
          {activeTab === 'snapshots' && (
            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-gray-200 font-sans">
                Context Snapshots for {selectedWorkspace?.name}
              </h3>
              {snapshot ? (
                <div className="p-4 bg-gray-950 border border-gray-800 rounded-xl font-sans space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-xs text-gray-100">Snapshot ID: {snapshot.id}</span>
                    <span className="px-2 py-0.5 rounded-full text-[10px] bg-emerald-500/10 text-emerald-400 font-mono">
                      Quality: {snapshot.quality_rating} ({(snapshot.quality_score * 100).toFixed(0)}%)
                    </span>
                  </div>
                  <div className="text-xs text-gray-300">
                    <strong>Statement:</strong> {snapshot.snapshot_statement}
                  </div>
                  <div className="text-xs text-gray-400 grid grid-cols-2 gap-2 pt-2 border-t border-gray-800/60">
                    <div>Task: {snapshot.task_title || 'None active'}</div>
                    <div>Workset Items: {snapshot.workset_count}</div>
                    <div>Can Resume: {snapshot.can_confidently_resume ? 'Yes (Confident)' : 'Needs Verification'}</div>
                    <div>Created At: {new Date(snapshot.created_at).toLocaleString()}</div>
                  </div>
                </div>
              ) : (
                <div className="p-8 text-center text-gray-500 font-sans">No snapshot recorded for this context.</div>
              )}
            </div>
          )}

          {/* TAB 10: HEALTH CHECKS */}
          {activeTab === 'health' && (
            <div className="space-y-4 font-sans">
              <div className="flex items-center justify-between p-4 bg-gray-950 border border-gray-800 rounded-xl">
                <div>
                  <h3 className="text-sm font-semibold text-gray-100">Context Health & Cross-System Consistency</h3>
                  <p className="text-xs text-gray-400 mt-0.5">
                    Non-destructive consistency verification across engine, memberships, sessions, memories, workset, snapshots, tasks.
                  </p>
                </div>
                <button
                  onClick={handleRunHealthCheck}
                  disabled={isHealthRunning}
                  className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium transition-colors"
                >
                  <ShieldCheck className={`w-4 h-4 ${isHealthRunning ? 'animate-spin' : ''}`} />
                  <span>{isHealthRunning ? 'Running Audit...' : 'Run Audit'}</span>
                </button>
              </div>

              {healthReport && (
                <div className="p-4 bg-gray-950 border border-gray-800 rounded-xl space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-semibold text-gray-200">Health Check Status:</span>
                      <span
                        className={`px-2 py-0.5 rounded-full text-[10px] font-bold uppercase font-mono ${
                          healthReport.status === 'pass'
                            ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                            : healthReport.status === 'warning'
                            ? 'bg-amber-500/10 text-amber-400 border border-amber-500/20'
                            : 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                        }`}
                      >
                        {healthReport.status}
                      </span>
                    </div>
                    <span className="text-[11px] text-gray-500 font-mono">
                      Checked in {(healthReport.execution_duration_us / 1000).toFixed(2)}ms
                    </span>
                  </div>

                  {healthReport.issues.length === 0 ? (
                    <p className="text-xs text-emerald-400 flex items-center gap-1.5">
                      <CheckCircle2 className="w-4 h-4" />
                      Zero integrity violations detected. All context entities valid.
                    </p>
                  ) : (
                    <div className="space-y-2">
                      {healthReport.issues.map((iss) => (
                        <div key={iss.issue_id} className="p-3 bg-gray-900 border border-gray-800 rounded-lg text-xs">
                          <div className="flex items-center justify-between text-amber-300 font-semibold mb-1">
                            <span>{iss.title}</span>
                            <span className="text-[10px] font-mono uppercase">{iss.severity}</span>
                          </div>
                          <p className="text-gray-300">{iss.description}</p>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}

              {consistencyReport && (
                <div className="p-4 bg-gray-950 border border-gray-800 rounded-xl space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-semibold text-gray-200">Cross-System Consistency:</span>
                      <span
                        className={`px-2 py-0.5 rounded-full text-[10px] font-bold uppercase font-mono ${
                          consistencyReport.status === 'pass'
                            ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                            : 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                        }`}
                      >
                        {consistencyReport.status}
                      </span>
                    </div>
                    <span className="text-[11px] text-gray-500 font-mono">
                      {consistencyReport.total_comparisons} comparisons
                    </span>
                  </div>
                  {consistencyReport.inconsistency_count === 0 ? (
                    <p className="text-xs text-emerald-400 flex items-center gap-1.5">
                      <CheckCircle2 className="w-4 h-4" />
                      All 11 contextual subsystems agree on context identity.
                    </p>
                  ) : (
                    <div className="space-y-2">
                      {consistencyReport.issues.map((iss) => (
                        <div key={iss.issue_id} className="p-3 bg-gray-900 border border-gray-800 rounded-lg text-xs">
                          <span className="text-rose-400 font-semibold">{iss.description}</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </div>
          )}

          {/* TAB 11: PERFORMANCE */}
          {activeTab === 'performance' && (
            <div className="space-y-4 font-sans">
              <div className="p-4 bg-gray-950 border border-gray-800 rounded-xl">
                <h3 className="text-sm font-semibold text-gray-100 mb-3">Context Engine Latency Breakdown</h3>
                {perfReport && (
                  <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-3">
                    {Object.entries(perfReport.category_summaries).map(([cat, sum]) => (
                      <div key={cat} className="p-3 bg-gray-900 border border-gray-800 rounded-lg">
                        <div className="text-[11px] font-medium text-indigo-300 truncate">{cat}</div>
                        <div className="text-base font-bold text-gray-100 mt-1 font-mono">
                          {(sum.average_duration_us / 1000).toFixed(2)}ms
                        </div>
                        <div className="text-[10px] text-gray-500 font-mono mt-0.5">
                          {sum.operation_count} ops • max {(sum.max_duration_us / 1000).toFixed(1)}ms
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className="p-4 bg-gray-950 border border-gray-800 rounded-xl">
                <h3 className="text-sm font-semibold text-gray-100 mb-2">Recent Execution Spans ({recentSpans.length})</h3>
                <div className="space-y-1.5 max-h-[35vh] overflow-y-auto pr-1">
                  {recentSpans.map((sp) => (
                    <div
                      key={sp.span_id}
                      className="p-2.5 bg-gray-900 border border-gray-800/80 rounded-lg flex items-center justify-between text-xs font-mono"
                    >
                      <div className="flex items-center gap-2 truncate">
                        <span className="px-1.5 py-0.5 rounded bg-gray-800 text-gray-300 text-[10px] font-sans">
                          {sp.category}
                        </span>
                        <span className="text-gray-200 truncate">{sp.name}</span>
                      </div>
                      <div className="flex items-center gap-3 text-gray-400 text-[11px]">
                        <span>{(sp.duration_us / 1000).toFixed(2)}ms</span>
                        <span>{sp.db_queries_count} queries</span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>,
    document.body
  );
}
