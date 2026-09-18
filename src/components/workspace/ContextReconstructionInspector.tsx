import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  RefreshCw,
  AlertTriangle,
  ArrowRightLeft,
  Cpu,
  Snowflake,
  History,
} from 'lucide-react';
import {
  contextReconstructionService,
  ContextReconstruction,
  ReconstructionSectionStatus,
} from '../../services/contextReconstruction';
import { toast } from 'sonner';

interface ContextReconstructionInspectorProps {
  workspaceId: string;
  onClose: () => void;
}

export function ContextReconstructionInspector({
  workspaceId,
  onClose,
}: ContextReconstructionInspectorProps) {
  const [currentReconstruction, setCurrentReconstruction] = useState<ContextReconstruction | null>(null);
  const [previousReconstruction, setPreviousReconstruction] = useState<ContextReconstruction | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isColdLoading, setIsColdLoading] = useState(false);
  const [isCompareMode, setIsCompareMode] = useState(false);
  const [activeTab, setActiveTab] = useState<'overview' | 'workset' | 'quality' | 'diff' | 'raw'>('overview');
  const [error, setError] = useState<string | null>(null);

  const fetchReconstruction = async (cold: boolean = false) => {
    if (cold) {
      setIsColdLoading(true);
    } else {
      setIsLoading(true);
    }
    setError(null);

    try {
      const result = cold
        ? await contextReconstructionService.reconstructCold(workspaceId)
        : await contextReconstructionService.getContextReconstruction(workspaceId);

      setCurrentReconstruction((prev) => {
        if (prev) {
          setPreviousReconstruction(prev);
        }
        return result;
      });

      if (cold) {
        toast.success('Cold reconstruction completed (bypassed all in-memory caches)');
      }
    } catch (err: any) {
      setError(err?.toString() || 'Failed to reconstruct context');
    } finally {
      setIsLoading(false);
      setIsColdLoading(false);
    }
  };

  useEffect(() => {
    if (workspaceId) {
      fetchReconstruction(false);
    }
  }, [workspaceId]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  return createPortal(
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-slate-950/80 backdrop-blur-md p-4 animate-in fade-in duration-150">
      <div className="fixed inset-0" onClick={onClose} />

      <div
        className="relative w-full max-w-6xl bg-slate-900 text-slate-100 rounded-2xl shadow-2xl border border-slate-800 flex flex-col max-h-[92vh] overflow-hidden z-10 font-sans"
        onClick={(e) => e.stopPropagation()}
      >
        {/* DEV Header */}
        <div className="px-6 py-4 border-b border-slate-800 bg-slate-950/70 flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-lg bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center text-emerald-400 flex-shrink-0">
              <Cpu size={18} />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h2 className="text-base font-bold text-white tracking-tight">
                  Context Reconstruction Inspector
                </h2>
                <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-amber-500/20 text-amber-300 border border-amber-500/30">
                  DEV ONLY
                </span>
              </div>
              <p className="text-xs text-slate-400 font-mono">
                Workspace ID: {workspaceId} · Reconstructed at:{' '}
                {currentReconstruction?.reconstructed_at
                  ? new Date(currentReconstruction.reconstructed_at).toLocaleTimeString()
                  : '—'}
              </p>
            </div>
          </div>

          {/* Action Toolbar */}
          <div className="flex items-center gap-2">
            <button
              onClick={() => fetchReconstruction(false)}
              disabled={isLoading || isColdLoading}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 transition-colors cursor-pointer disabled:opacity-50"
              title="Run cached deterministic reconstruction"
            >
              <RefreshCw size={13} className={isLoading ? 'animate-spin text-emerald-400' : ''} />
              <span>Reconstruct</span>
            </button>

            <button
              onClick={() => fetchReconstruction(true)}
              disabled={isLoading || isColdLoading}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg bg-emerald-950/60 hover:bg-emerald-900/60 text-emerald-300 border border-emerald-800/60 transition-colors cursor-pointer disabled:opacity-50"
              title="Bypass all caches and reconstruct cold from persisted SQLite store"
            >
              <Snowflake size={13} className={isColdLoading ? 'animate-spin text-emerald-400' : ''} />
              <span>Cold Reconstruct</span>
            </button>

            {previousReconstruction && (
              <button
                onClick={() => {
                  setIsCompareMode(!isCompareMode);
                  if (!isCompareMode) setActiveTab('diff');
                }}
                className={`inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border transition-colors cursor-pointer ${
                  isCompareMode
                    ? 'bg-indigo-600 text-white border-indigo-500'
                    : 'bg-slate-800 hover:bg-slate-700 text-slate-300 border-slate-700'
                }`}
                title="Compare with previous reconstruction in this session"
              >
                <ArrowRightLeft size={13} />
                <span>Compare</span>
              </button>
            )}

            <button
              onClick={onClose}
              className="p-1.5 text-slate-400 hover:text-slate-100 hover:bg-slate-800 rounded-lg transition-colors cursor-pointer ml-1"
              title="Close (Esc)"
            >
              <X size={16} />
            </button>
          </div>
        </div>

        {/* Navigation Tabs */}
        <div className="flex border-b border-slate-800 bg-slate-950/40 px-6 text-xs font-medium">
          <button
            onClick={() => setActiveTab('overview')}
            className={`py-2.5 px-3 border-b-2 transition-colors cursor-pointer ${
              activeTab === 'overview'
                ? 'border-emerald-400 text-emerald-300'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            Reconstruction Overview
          </button>
          <button
            onClick={() => setActiveTab('workset')}
            className={`py-2.5 px-3 border-b-2 transition-colors cursor-pointer ${
              activeTab === 'workset'
                ? 'border-emerald-400 text-emerald-300'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            Working Set & Memories ({currentReconstruction?.workset.total_count || 0})
          </button>
          <button
            onClick={() => setActiveTab('quality')}
            className={`py-2.5 px-3 border-b-2 transition-colors cursor-pointer ${
              activeTab === 'quality'
                ? 'border-emerald-400 text-emerald-300'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            Quality & Contradictions ({currentReconstruction?.quality.factors.contradiction_count || 0})
          </button>
          {previousReconstruction && (
            <button
              onClick={() => setActiveTab('diff')}
              className={`py-2.5 px-3 border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
                activeTab === 'diff'
                  ? 'border-indigo-400 text-indigo-300'
                  : 'border-transparent text-slate-400 hover:text-slate-200'
              }`}
            >
              <History size={12} />
              <span>Diff Comparison</span>
            </button>
          )}
          <button
            onClick={() => setActiveTab('raw')}
            className={`py-2.5 px-3 border-b-2 transition-colors cursor-pointer ${
              activeTab === 'raw'
                ? 'border-emerald-400 text-emerald-300'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            Raw JSON
          </button>
        </div>

        {/* Content Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6 bg-slate-900/90 text-slate-200">
          {isLoading && !currentReconstruction ? (
            <div className="h-72 flex flex-col items-center justify-center text-slate-400 gap-2">
              <RefreshCw size={24} className="animate-spin text-emerald-400" />
              <p className="text-xs font-mono">Running deterministic 12-step reconstruction pipeline...</p>
            </div>
          ) : error ? (
            <div className="p-4 bg-red-950/60 border border-red-800 rounded-xl text-red-200 text-sm flex items-start gap-3">
              <AlertTriangle size={18} className="flex-shrink-0 text-red-400 mt-0.5" />
              <div>
                <div className="font-semibold">Reconstruction Error</div>
                <div className="text-xs font-mono text-red-300 mt-1">{error}</div>
              </div>
            </div>
          ) : currentReconstruction ? (
            activeTab === 'overview' ? (
              <InspectorOverviewTab reconstruction={currentReconstruction} />
            ) : activeTab === 'workset' ? (
              <InspectorWorksetTab reconstruction={currentReconstruction} />
            ) : activeTab === 'quality' ? (
              <InspectorQualityTab reconstruction={currentReconstruction} />
            ) : activeTab === 'diff' ? (
              <InspectorDiffTab
                current={currentReconstruction}
                previous={previousReconstruction}
              />
            ) : (
              <pre className="p-4 bg-slate-950 rounded-xl border border-slate-800 text-xs font-mono text-emerald-300 overflow-x-auto max-h-[60vh] leading-relaxed">
                <code>{JSON.stringify(currentReconstruction, null, 2)}</code>
              </pre>
            )
          ) : null}
        </div>

        {/* Epistemic Disclaimer Footer */}
        <div className="px-6 py-2.5 border-t border-slate-800 bg-slate-950 text-[11px] font-mono text-slate-500 flex items-center justify-between">
          <div className="truncate mr-4">
            <span className="text-slate-400 font-semibold">Disclaimer: </span>
            {currentReconstruction?.disclaimer}
          </div>
          <div className="flex-shrink-0 text-slate-400">
            Rating: <strong className="text-emerald-400">{currentReconstruction?.quality.rating}</strong>
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
}

// ---------------------------------------------------------------------------
// Epistemic Badges
// ---------------------------------------------------------------------------

function StatusBadge({ status }: { status: ReconstructionSectionStatus | string }) {
  switch (status) {
    case 'available':
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">
          AVAILABLE
        </span>
      );
    case 'partial':
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-amber-500/20 text-amber-300 border border-amber-500/30">
          PARTIAL
        </span>
      );
    case 'uncertain':
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-rose-500/20 text-rose-300 border border-rose-500/30">
          UNCERTAIN
        </span>
      );
    case 'unavailable':
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-slate-700/50 text-slate-400 border border-slate-600/50">
          UNAVAILABLE
        </span>
      );
    default:
      return <span className="text-slate-400">{status}</span>;
  }
}

function ProvenanceBadge({ provenance }: { provenance?: string | null }) {
  if (!provenance) return null;
  const p = provenance.toLowerCase();
  if (p === 'observed') {
    return (
      <span className="inline-flex items-center px-1.5 py-0.2 rounded text-[9px] font-mono font-semibold bg-emerald-950 text-emerald-400 border border-emerald-800">
        OBSERVED
      </span>
    );
  }
  if (p === 'confirmed') {
    return (
      <span className="inline-flex items-center px-1.5 py-0.2 rounded text-[9px] font-mono font-semibold bg-blue-950 text-blue-400 border border-blue-800">
        CONFIRMED
      </span>
    );
  }
  return (
    <span className="inline-flex items-center px-1.5 py-0.2 rounded text-[9px] font-mono font-semibold bg-purple-950 text-purple-400 border border-purple-800">
      INFERRED
    </span>
  );
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

function InspectorOverviewTab({ reconstruction }: { reconstruction: ContextReconstruction }) {
  const { context, current_task, current_state, last_meaningful_activity, confidence, resume_readiness } = reconstruction;

  return (
    <div className="space-y-6">
      {/* 1. Context & Task Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Context Entity */}
        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">Context Entity</span>
            <div className="flex items-center gap-2">
              <ProvenanceBadge provenance={context.provenance} />
              <StatusBadge status={context.status} />
            </div>
          </div>
          <div className="text-lg font-bold text-white tracking-tight">{context.name}</div>
          <div className="grid grid-cols-2 gap-2 text-xs font-mono text-slate-400 pt-2 border-t border-slate-800/80">
            <div>ID: <span className="text-slate-200">{context.id || 'None'}</span></div>
            <div>Lifecycle: <span className="text-slate-200">{context.lifecycle_state || 'None'}</span></div>
            <div>Confidence: <span className="text-emerald-400">{Math.round(context.confidence * 100)}%</span></div>
          </div>
        </div>

        {/* Current Task */}
        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">Current Task</span>
            <div className="flex items-center gap-2">
              {current_task.provenance && <ProvenanceBadge provenance={current_task.provenance} />}
              <StatusBadge status={current_task.status} />
            </div>
          </div>
          <div className="text-lg font-bold text-white tracking-tight">
            {current_task.title || <span className="text-slate-500 italic">No Task Identified</span>}
          </div>
          <div className="grid grid-cols-2 gap-2 text-xs font-mono text-slate-400 pt-2 border-t border-slate-800/80">
            <div>Verb: <span className="text-slate-200">{current_task.action_verb || '—'}</span></div>
            <div>Target: <span className="text-slate-200">{current_task.target_subject || '—'}</span></div>
            <div>Confidence: <span className="text-emerald-400">{current_task.confidence != null ? `${Math.round(current_task.confidence * 100)}%` : '—'}</span></div>
          </div>
          {current_task.reasons && current_task.reasons.length > 0 && (
            <div className="text-xs text-slate-400 pt-1">
              <span className="text-slate-500 font-semibold">Reason: </span>
              {current_task.reasons[0]}
            </div>
          )}
          {current_task.conflict_reason && (
            <div className="p-2 rounded bg-rose-950/40 border border-rose-800/50 text-xs text-rose-300">
              <strong>Contradiction: </strong> {current_task.conflict_reason}
            </div>
          )}
        </div>
      </div>

      {/* 2. Operational State & Last Meaningful Activity */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Operational State */}
        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">Operational State</span>
            <StatusBadge status={current_state.status} />
          </div>
          <div className="flex items-center gap-3">
            <span className="text-base font-bold text-white uppercase tracking-wider font-mono">
              {current_state.state_value || 'None'}
            </span>
            {current_state.freshness && (
              <span className="px-2 py-0.5 rounded text-[10px] font-mono bg-indigo-500/20 text-indigo-300 border border-indigo-500/30">
                {current_state.freshness.toUpperCase()}
              </span>
            )}
          </div>
          <div className="text-xs text-slate-400 font-mono space-y-1 pt-2 border-t border-slate-800/80">
            <div>Display: <span className="text-slate-200">{current_state.freshness_display || '—'}</span></div>
            <div>Last Active: <span className="text-slate-200">{current_state.last_active_at ? new Date(current_state.last_active_at).toLocaleString() : '—'}</span></div>
            {current_state.explanation && <div className="text-slate-400 italic">{current_state.explanation}</div>}
          </div>
        </div>

        {/* Meaningful Activity */}
        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">Last Meaningful Activity</span>
            <div className="flex items-center gap-2">
              {last_meaningful_activity.provenance && <ProvenanceBadge provenance={last_meaningful_activity.provenance} />}
              <StatusBadge status={last_meaningful_activity.status} />
            </div>
          </div>
          <div className="text-sm font-semibold text-slate-100">
            {last_meaningful_activity.description || <span className="text-slate-500 italic">No meaningful event</span>}
          </div>
          <div className="grid grid-cols-2 gap-2 text-xs font-mono text-slate-400 pt-2 border-t border-slate-800/80">
            <div>Timestamp: <span className="text-slate-200">{last_meaningful_activity.timestamp ? new Date(last_meaningful_activity.timestamp).toLocaleTimeString() : '—'}</span></div>
            <div>Confidence: <span className="text-emerald-400">{last_meaningful_activity.confidence != null ? `${Math.round(last_meaningful_activity.confidence * 100)}%` : '—'}</span></div>
          </div>
        </div>
      </div>

      {/* 3. Composite Confidence & Resume Readiness */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Composite Confidence */}
        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">Reconstruction Confidence</span>
            <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">
              {confidence.band.toUpperCase()}
            </span>
          </div>
          <div className="text-2xl font-black font-mono text-emerald-400">
            {Math.round(confidence.score * 100)}%
          </div>
          <div className="space-y-1 pt-2 border-t border-slate-800/80">
            <span className="text-[11px] font-bold text-slate-400 uppercase tracking-wider">Rationale:</span>
            <ul className="text-xs text-slate-300 space-y-1">
              {confidence.reasons.map((r, i) => (
                <li key={i} className="flex items-start gap-1.5">
                  <span className="text-emerald-500 select-none">•</span>
                  <span>{r}</span>
                </li>
              ))}
            </ul>
          </div>
        </div>

        {/* Resume Readiness */}
        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">Resume Readiness</span>
            <StatusBadge status={resume_readiness.status} />
          </div>
          <div className="text-base font-bold font-mono text-white uppercase tracking-wider">
            {resume_readiness.readiness}
          </div>
          <div className="space-y-1 text-xs text-slate-300 pt-2 border-t border-slate-800/80">
            <div><strong className="text-slate-400">Action: </strong>{resume_readiness.suggested_action}</div>
            {resume_readiness.blockers.length > 0 && (
              <div className="text-rose-400">
                <strong>Blockers: </strong>{resume_readiness.blockers.join(', ')}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function InspectorWorksetTab({ reconstruction }: { reconstruction: ContextReconstruction }) {
  const { workset, important_memories, latest_snapshot } = reconstruction;

  return (
    <div className="space-y-6">
      {/* Workset Items Table */}
      <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-3">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-sm font-bold text-white uppercase tracking-wider">Working Set Assets</h3>
            <p className="text-xs text-slate-400 font-mono">
              Total: {workset.total_count} · Primary: {workset.primary_count} · Supporting: {workset.supporting_count} · Quality: {Math.round(workset.quality_score * 100)}%
            </p>
          </div>
          <StatusBadge status={workset.status} />
        </div>

        {workset.items.length > 0 ? (
          <div className="overflow-x-auto">
            <table className="w-full text-xs font-mono text-left border-collapse">
              <thead>
                <tr className="border-b border-slate-800 text-slate-400 uppercase text-[10px]">
                  <th className="py-2 px-2">ID</th>
                  <th className="py-2 px-2">Type</th>
                  <th className="py-2 px-2">Title / Content</th>
                  <th className="py-2 px-2">App / Domain</th>
                  <th className="py-2 px-2">Relevance</th>
                  <th className="py-2 px-2">Score</th>
                  <th className="py-2 px-2">Provenance</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/60 text-slate-300">
                {workset.items.map((item) => (
                  <tr key={item.id} className="hover:bg-slate-800/40 transition-colors">
                    <td className="py-2 px-2 text-slate-400">{item.id.slice(0, 8)}...</td>
                    <td className="py-2 px-2 uppercase text-[10px] text-slate-400">{item.item_type}</td>
                    <td className="py-2 px-2 font-sans font-medium text-white max-w-xs truncate">
                      {item.title || item.content_snippet}
                    </td>
                    <td className="py-2 px-2 text-slate-400">{item.app_name || item.domain || '—'}</td>
                    <td className="py-2 px-2">
                      <span className={`px-1.5 py-0.5 rounded text-[10px] uppercase font-bold ${
                        item.relevance === 'primary' ? 'bg-emerald-500/20 text-emerald-300' : 'bg-slate-700/50 text-slate-300'
                      }`}>
                        {item.relevance}
                      </span>
                    </td>
                    <td className="py-2 px-2 text-emerald-400 font-bold">{Math.round(item.relevance_score * 100)}%</td>
                    <td className="py-2 px-2"><ProvenanceBadge provenance={item.provenance} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="text-xs text-slate-500 italic py-4 text-center">No working set items identified.</div>
        )}
      </div>

      {/* Important Memories Table */}
      <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-3">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-sm font-bold text-white uppercase tracking-wider">Important Contextual Memories</h3>
            <p className="text-xs text-slate-400 font-mono">
              Total: {important_memories.total_count} · Pinned: {important_memories.pinned_count}
            </p>
          </div>
          <StatusBadge status={important_memories.status} />
        </div>

        {important_memories.memories.length > 0 ? (
          <div className="overflow-x-auto">
            <table className="w-full text-xs font-mono text-left border-collapse">
              <thead>
                <tr className="border-b border-slate-800 text-slate-400 uppercase text-[10px]">
                  <th className="py-2 px-2">Type</th>
                  <th className="py-2 px-2">Title</th>
                  <th className="py-2 px-2">Importance</th>
                  <th className="py-2 px-2">Score</th>
                  <th className="py-2 px-2">Provenance</th>
                  <th className="py-2 px-2">Explanation</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/60 text-slate-300">
                {important_memories.memories.map((mem) => (
                  <tr key={mem.id} className="hover:bg-slate-800/40 transition-colors">
                    <td className="py-2 px-2 uppercase text-[10px] text-slate-400">{mem.memory_type}</td>
                    <td className="py-2 px-2 font-sans font-medium text-white max-w-xs truncate">{mem.title}</td>
                    <td className="py-2 px-2 uppercase text-[10px] text-amber-400 font-bold">{mem.importance_band}</td>
                    <td className="py-2 px-2 text-emerald-400 font-bold">{Math.round(mem.importance_score * 100)}%</td>
                    <td className="py-2 px-2"><ProvenanceBadge provenance={mem.provenance} /></td>
                    <td className="py-2 px-2 font-sans text-slate-400 max-w-sm truncate">{mem.explanation}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="text-xs text-slate-500 italic py-4 text-center">No important memories recorded.</div>
        )}
      </div>

      {/* Snapshot Details */}
      <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-2">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-bold text-white uppercase tracking-wider">Latest Departure Snapshot</h3>
          <StatusBadge status={latest_snapshot.status} />
        </div>
        {latest_snapshot.snapshot ? (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-xs font-mono text-slate-300 pt-2">
            <div>Snapshot ID: <span className="text-slate-400">{latest_snapshot.snapshot.id.slice(0, 8)}...</span></div>
            <div>Captured: <span className="text-slate-400">{new Date(latest_snapshot.snapshot.created_at).toLocaleString()}</span></div>
            <div>Rating: <span className="text-emerald-400 uppercase font-bold">{latest_snapshot.snapshot.quality_rating}</span></div>
            <div>Workset Count: <span className="text-slate-400">{latest_snapshot.snapshot.workset_count}</span></div>
          </div>
        ) : (
          <div className="text-xs text-slate-500 italic py-2">No departure snapshot available.</div>
        )}
      </div>
    </div>
  );
}

function InspectorQualityTab({ reconstruction }: { reconstruction: ContextReconstruction }) {
  const { quality } = reconstruction;
  const { factors } = quality;

  return (
    <div className="space-y-6">
      {/* Quality Summary Header */}
      <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">Quality Classification</span>
          <span className="px-2.5 py-1 rounded text-xs font-mono font-bold bg-emerald-500/20 text-emerald-300 border border-emerald-500/30 uppercase">
            {quality.rating}
          </span>
        </div>
        <div className="text-lg font-bold text-white">{quality.display_label}</div>
        <p className="text-xs text-slate-300 font-sans leading-relaxed">{quality.explanation}</p>
        <div className="text-xs font-mono text-slate-400 pt-2 border-t border-slate-800">
          Can Confidently Resume: <strong className={quality.can_confidently_resume ? 'text-emerald-400' : 'text-rose-400'}>{quality.can_confidently_resume ? 'TRUE' : 'FALSE'}</strong>
        </div>
      </div>

      {/* 9 Quality Dimensions Breakdown */}
      <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-3">
        <h3 className="text-xs font-bold text-slate-300 uppercase tracking-wider">Evaluated Quality Dimensions</h3>
        <div className="grid grid-cols-2 md:grid-cols-3 gap-3 text-xs font-mono">
          <div className="p-2.5 bg-slate-900 rounded-lg border border-slate-800">
            <span className="text-slate-500 uppercase text-[10px]">Context Confidence</span>
            <div className="text-emerald-400 font-bold mt-1 uppercase">{factors.context_confidence}</div>
          </div>
          <div className="p-2.5 bg-slate-900 rounded-lg border border-slate-800">
            <span className="text-slate-500 uppercase text-[10px]">Task Confidence</span>
            <div className="text-emerald-400 font-bold mt-1 uppercase">{factors.task_confidence}</div>
          </div>
          <div className="p-2.5 bg-slate-900 rounded-lg border border-slate-800">
            <span className="text-slate-500 uppercase text-[10px]">State Confidence</span>
            <div className="text-emerald-400 font-bold mt-1 uppercase">{factors.state_confidence}</div>
          </div>
          <div className="p-2.5 bg-slate-900 rounded-lg border border-slate-800">
            <span className="text-slate-500 uppercase text-[10px]">Workset Quality</span>
            <div className="text-indigo-400 font-bold mt-1 uppercase">{factors.workset_quality}</div>
          </div>
          <div className="p-2.5 bg-slate-900 rounded-lg border border-slate-800">
            <span className="text-slate-500 uppercase text-[10px]">Memory Quality</span>
            <div className="text-indigo-400 font-bold mt-1 uppercase">{factors.memory_quality}</div>
          </div>
          <div className="p-2.5 bg-slate-900 rounded-lg border border-slate-800">
            <span className="text-slate-500 uppercase text-[10px]">Snapshot Freshness</span>
            <div className="text-indigo-400 font-bold mt-1 uppercase">{factors.snapshot_freshness}</div>
          </div>
          <div className="p-2.5 bg-slate-900 rounded-lg border border-slate-800">
            <span className="text-slate-500 uppercase text-[10px]">Evidence Consistency</span>
            <div className="text-amber-400 font-bold mt-1 uppercase">{factors.evidence_consistency}</div>
          </div>
          <div className="p-2.5 bg-slate-900 rounded-lg border border-slate-800">
            <span className="text-slate-500 uppercase text-[10px]">Data Freshness</span>
            <div className="text-amber-400 font-bold mt-1 uppercase">{factors.data_freshness}</div>
          </div>
          <div className="p-2.5 bg-slate-900 rounded-lg border border-slate-800">
            <span className="text-slate-500 uppercase text-[10px]">Contradictions Count</span>
            <div className="text-rose-400 font-bold mt-1">{factors.contradiction_count}</div>
          </div>
        </div>
      </div>

      {/* Contradictions & Gaps */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-2">
          <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">Contradictions Detected</span>
          {factors.contradictions.length > 0 ? (
            <ul className="text-xs text-rose-300 space-y-1 font-mono">
              {factors.contradictions.map((c, idx) => (
                <li key={idx} className="flex items-start gap-1.5">
                  <span className="text-rose-500 select-none">•</span>
                  <span>{c}</span>
                </li>
              ))}
            </ul>
          ) : (
            <div className="text-xs text-emerald-400 font-mono">No contradictions detected. Clean consistency.</div>
          )}
        </div>

        <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 space-y-2">
          <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">Gaps & Missing Evidence</span>
          {factors.gaps.length > 0 ? (
            <ul className="text-xs text-amber-300 space-y-1 font-mono">
              {factors.gaps.map((g, idx) => (
                <li key={idx} className="flex items-start gap-1.5">
                  <span className="text-amber-500 select-none">•</span>
                  <span>{g}</span>
                </li>
              ))}
            </ul>
          ) : (
            <div className="text-xs text-emerald-400 font-mono">No evidence gaps. Fully specified.</div>
          )}
        </div>
      </div>
    </div>
  );
}

function InspectorDiffTab({
  current,
  previous,
}: {
  current: ContextReconstruction;
  previous: ContextReconstruction | null;
}) {
  if (!previous) {
    return (
      <div className="p-8 text-center text-xs text-slate-400 font-mono">
        No previous reconstruction state recorded yet. Click &ldquo;Reconstruct&rdquo; or &ldquo;Cold Reconstruct&rdquo; to produce a second state to compare.
      </div>
    );
  }

  const taskChanged = current.current_task.title !== previous.current_task.title;
  const stateChanged = current.current_state.state_value !== previous.current_state.state_value;
  const confidenceDiff = current.confidence.score - previous.confidence.score;
  const worksetCountDiff = current.workset.total_count - previous.workset.total_count;

  return (
    <div className="space-y-6">
      {/* Diff Metrics Grid */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs font-mono">
        <div className="p-3 bg-slate-950/70 rounded-lg border border-slate-800">
          <span className="text-slate-500 uppercase text-[10px]">Task Delta</span>
          <div className={`font-bold mt-1 ${taskChanged ? 'text-amber-400' : 'text-slate-300'}`}>
            {taskChanged ? 'CHANGED' : 'IDENTICAL'}
          </div>
        </div>
        <div className="p-3 bg-slate-950/70 rounded-lg border border-slate-800">
          <span className="text-slate-500 uppercase text-[10px]">State Delta</span>
          <div className={`font-bold mt-1 ${stateChanged ? 'text-amber-400' : 'text-slate-300'}`}>
            {stateChanged ? 'TRANSITIONED' : 'UNCHANGED'}
          </div>
        </div>
        <div className="p-3 bg-slate-950/70 rounded-lg border border-slate-800">
          <span className="text-slate-500 uppercase text-[10px]">Confidence Delta</span>
          <div className={`font-bold mt-1 ${confidenceDiff !== 0 ? 'text-emerald-400' : 'text-slate-300'}`}>
            {confidenceDiff > 0 ? `+${Math.round(confidenceDiff * 100)}%` : `${Math.round(confidenceDiff * 100)}%`}
          </div>
        </div>
        <div className="p-3 bg-slate-950/70 rounded-lg border border-slate-800">
          <span className="text-slate-500 uppercase text-[10px]">Workset Item Delta</span>
          <div className="font-bold text-indigo-300 mt-1">
            {worksetCountDiff > 0 ? `+${worksetCountDiff} items` : `${worksetCountDiff} items`}
          </div>
        </div>
      </div>

      {/* Side-by-Side Comparison Table */}
      <div className="p-4 bg-slate-950/60 rounded-xl border border-slate-800 overflow-x-auto">
        <table className="w-full text-xs font-mono text-left border-collapse">
          <thead>
            <tr className="border-b border-slate-800 text-slate-400 uppercase text-[10px]">
              <th className="py-2 px-3">Field</th>
              <th className="py-2 px-3 text-slate-400">Previous Reconstruction</th>
              <th className="py-2 px-3 text-emerald-400">Current Reconstruction</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/60 text-slate-300">
            <tr>
              <td className="py-2.5 px-3 font-semibold text-slate-400">Context Name</td>
              <td className="py-2.5 px-3 text-slate-400">{previous.context.name}</td>
              <td className="py-2.5 px-3 text-emerald-300 font-bold">{current.context.name}</td>
            </tr>
            <tr>
              <td className="py-2.5 px-3 font-semibold text-slate-400">Task Title</td>
              <td className="py-2.5 px-3 text-slate-400">{previous.current_task.title || 'None'}</td>
              <td className="py-2.5 px-3 text-emerald-300 font-bold">{current.current_task.title || 'None'}</td>
            </tr>
            <tr>
              <td className="py-2.5 px-3 font-semibold text-slate-400">State Value</td>
              <td className="py-2.5 px-3 text-slate-400">{previous.current_state.state_value || 'None'}</td>
              <td className="py-2.5 px-3 text-emerald-300 font-bold">{current.current_state.state_value || 'None'}</td>
            </tr>
            <tr>
              <td className="py-2.5 px-3 font-semibold text-slate-400">Workset Items</td>
              <td className="py-2.5 px-3 text-slate-400">{previous.workset.total_count} items</td>
              <td className="py-2.5 px-3 text-emerald-300 font-bold">{current.workset.total_count} items</td>
            </tr>
            <tr>
              <td className="py-2.5 px-3 font-semibold text-slate-400">Confidence</td>
              <td className="py-2.5 px-3 text-slate-400">{Math.round(previous.confidence.score * 100)}%</td>
              <td className="py-2.5 px-3 text-emerald-300 font-bold">{Math.round(current.confidence.score * 100)}%</td>
            </tr>
            <tr>
              <td className="py-2.5 px-3 font-semibold text-slate-400">Quality Rating</td>
              <td className="py-2.5 px-3 text-slate-400 uppercase">{previous.quality.rating}</td>
              <td className="py-2.5 px-3 text-emerald-300 font-bold uppercase">{current.quality.rating}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  );
}
