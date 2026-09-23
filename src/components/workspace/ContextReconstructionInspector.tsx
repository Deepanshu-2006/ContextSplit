import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  RefreshCw,
  AlertTriangle,
  ArrowRightLeft,
  Snowflake,
  History,
  CheckCircle2,
  AlertCircle,
  HelpCircle,
  Layers,
  FileText,
  Bookmark,
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
        toast.success('Cold reconstruction completed (bypassed in-memory caches)');
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

  const contextName = currentReconstruction?.context?.name || 'Workspace';

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-gray-900/40 backdrop-blur-xs p-4 animate-in fade-in duration-150 font-sans">
      <div className="fixed inset-0" onClick={onClose} />

      <div
        className="relative w-full max-w-4xl bg-white text-gray-900 rounded-2xl shadow-2xl border border-gray-100 flex flex-col max-h-[88vh] overflow-hidden z-10"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Top Header */}
        <div className="px-6 py-4 border-b border-gray-100 flex items-center justify-between bg-white">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-lg bg-gray-50 border border-gray-200/80 flex items-center justify-center text-gray-700">
              <Layers size={16} />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <span className="text-sm font-semibold text-gray-900">{contextName}</span>
                <span className="text-gray-300">/</span>
                <span className="text-xs text-gray-500 font-medium">Reconstruction Diagnostics</span>
                <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-gray-100 text-gray-600">
                  DEV
                </span>
              </div>
              <p className="text-[11px] text-gray-400 font-mono mt-0.5">
                ID: {workspaceId.slice(0, 16)}... {currentReconstruction?.reconstructed_at && `· ${new Date(currentReconstruction.reconstructed_at).toLocaleTimeString()}`}
              </p>
            </div>
          </div>

          {/* Action Toolbar */}
          <div className="flex items-center gap-1.5">
            <button
              onClick={() => fetchReconstruction(false)}
              disabled={isLoading || isColdLoading}
              className="inline-flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium rounded-lg text-gray-600 hover:text-gray-900 hover:bg-gray-100 transition-colors cursor-pointer disabled:opacity-50"
              title="Refresh cached reconstruction"
            >
              <RefreshCw size={13} className={isLoading ? 'animate-spin text-gray-900' : 'text-gray-500'} />
              <span>Refresh</span>
            </button>

            <button
              onClick={() => fetchReconstruction(true)}
              disabled={isLoading || isColdLoading}
              className="inline-flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium rounded-lg text-emerald-700 bg-emerald-50 hover:bg-emerald-100/80 border border-emerald-200/60 transition-colors cursor-pointer disabled:opacity-50"
              title="Bypass all caches and reconstruct cold from SQLite"
            >
              <Snowflake size={13} className={isColdLoading ? 'animate-spin' : ''} />
              <span>Cold Test</span>
            </button>

            {previousReconstruction && (
              <button
                onClick={() => {
                  setIsCompareMode(!isCompareMode);
                  if (!isCompareMode) setActiveTab('diff');
                }}
                className={`inline-flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium rounded-lg transition-colors cursor-pointer ${
                  isCompareMode
                    ? 'bg-indigo-600 text-white shadow-2xs'
                    : 'text-gray-600 hover:text-gray-900 hover:bg-gray-100'
                }`}
              >
                <ArrowRightLeft size={13} />
                <span>Compare</span>
              </button>
            )}

            <div className="w-px h-4 bg-gray-200 mx-1" />

            <button
              onClick={onClose}
              className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors cursor-pointer"
              title="Close (Esc)"
            >
              <X size={16} />
            </button>
          </div>
        </div>

        {/* Tab Navigation */}
        <div className="flex border-b border-gray-100 px-6 bg-white text-xs gap-6">
          <button
            onClick={() => setActiveTab('overview')}
            className={`py-3 font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
              activeTab === 'overview'
                ? 'border-gray-900 text-gray-900 font-semibold'
                : 'border-transparent text-gray-400 hover:text-gray-700'
            }`}
          >
            Overview
          </button>
          <button
            onClick={() => setActiveTab('workset')}
            className={`py-3 font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
              activeTab === 'workset'
                ? 'border-gray-900 text-gray-900 font-semibold'
                : 'border-transparent text-gray-400 hover:text-gray-700'
            }`}
          >
            Working Set & Memories
            <span className="px-1.5 py-0.2 rounded-full text-[10px] bg-gray-100 text-gray-600 font-medium">
              {currentReconstruction?.workset?.total_count ?? 0}
            </span>
          </button>
          <button
            onClick={() => setActiveTab('quality')}
            className={`py-3 font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
              activeTab === 'quality'
                ? 'border-gray-900 text-gray-900 font-semibold'
                : 'border-transparent text-gray-400 hover:text-gray-700'
            }`}
          >
            Quality & Signals
          </button>
          {previousReconstruction && (
            <button
              onClick={() => setActiveTab('diff')}
              className={`py-3 font-medium border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 ${
                activeTab === 'diff'
                  ? 'border-indigo-600 text-indigo-600 font-semibold'
                  : 'border-transparent text-gray-400 hover:text-gray-700'
              }`}
            >
              <History size={13} />
              <span>Diff</span>
            </button>
          )}
          <button
            onClick={() => setActiveTab('raw')}
            className={`py-3 font-medium border-b-2 transition-colors cursor-pointer ${
              activeTab === 'raw'
                ? 'border-gray-900 text-gray-900 font-semibold'
                : 'border-transparent text-gray-400 hover:text-gray-700'
            }`}
          >
            Raw JSON
          </button>
        </div>

        {/* Content Body */}
        <div className="flex-1 overflow-y-auto p-6 bg-gray-50/50">
          {isLoading && !currentReconstruction ? (
            <div className="h-64 flex flex-col items-center justify-center text-gray-400 gap-2">
              <RefreshCw size={20} className="animate-spin text-gray-500" />
              <p className="text-xs text-gray-500 font-medium">Evaluating context reconstruction...</p>
            </div>
          ) : error ? (
            <div className="p-4 bg-red-50 text-red-700 rounded-xl border border-red-200/80 text-sm flex items-start gap-3">
              <AlertTriangle size={18} className="flex-shrink-0 text-red-500 mt-0.5" />
              <div>
                <div className="font-semibold text-red-900">Reconstruction Error</div>
                <div className="text-xs font-mono text-red-700 mt-1">{error}</div>
              </div>
            </div>
          ) : currentReconstruction ? (
            activeTab === 'overview' ? (
              <MinimalOverviewTab reconstruction={currentReconstruction} />
            ) : activeTab === 'workset' ? (
              <MinimalWorksetTab reconstruction={currentReconstruction} />
            ) : activeTab === 'quality' ? (
              <MinimalQualityTab reconstruction={currentReconstruction} />
            ) : activeTab === 'diff' ? (
              <MinimalDiffTab
                current={currentReconstruction}
                previous={previousReconstruction}
              />
            ) : (
              <pre className="p-4 bg-gray-900 text-emerald-400 rounded-xl text-xs font-mono overflow-x-auto max-h-[55vh] leading-relaxed">
                <code>{JSON.stringify(currentReconstruction, null, 2)}</code>
              </pre>
            )
          ) : null}
        </div>

        {/* Minimal Footer */}
        <div className="px-6 py-2.5 border-t border-gray-100 bg-white text-[11px] text-gray-400 flex items-center justify-between">
          <span className="truncate pr-4">
            {currentReconstruction?.epistemic_disclaimer || currentReconstruction?.disclaimer || 'Deterministic evaluation based on recorded workspace activity signals.'}
          </span>
          <span className="flex-shrink-0 font-medium text-gray-600">
            Engine Rating: <strong className="text-emerald-700 uppercase font-semibold">{currentReconstruction?.quality?.rating || '—'}</strong>
          </span>
        </div>
      </div>
    </div>,
    document.body
  );
}

// ---------------------------------------------------------------------------
// Helpers & Subcomponents
// ---------------------------------------------------------------------------

function formatExplanationLines(text?: string | null): string[] {
  if (!text) return [];
  // Strip "Why ContextSplit thinks this::" or similar prefixes
  let cleaned = text.replace(/^(why contextsplit thinks this\s*::\s*)/i, '');
  return cleaned
    .split(';')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

function SectionStatusPill({ status }: { status?: ReconstructionSectionStatus | string | null }) {
  const s = (status || 'unavailable').toLowerCase();
  if (s === 'available') {
    return (
      <span className="inline-flex items-center gap-1 text-[11px] font-medium text-emerald-700">
        <CheckCircle2 size={12} className="text-emerald-600" />
        Available
      </span>
    );
  }
  if (s === 'partial') {
    return (
      <span className="inline-flex items-center gap-1 text-[11px] font-medium text-amber-700">
        <AlertCircle size={12} className="text-amber-600" />
        Partial
      </span>
    );
  }
  if (s === 'uncertain') {
    return (
      <span className="inline-flex items-center gap-1 text-[11px] font-medium text-rose-700">
        <AlertCircle size={12} className="text-rose-600" />
        Uncertain
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 text-[11px] font-medium text-gray-400">
      <HelpCircle size={12} />
      Unavailable
    </span>
  );
}

// ---------------------------------------------------------------------------
// Minimal Overview Tab
// ---------------------------------------------------------------------------

function MinimalOverviewTab({ reconstruction }: { reconstruction: ContextReconstruction }) {
  const {
    context,
    current_task,
    current_state,
    last_meaningful_activity,
    confidence,
    resume_readiness,
    quality,
  } = reconstruction;

  const scorePct = Math.round((confidence?.score ?? 0) * 100);
  const explanationBullets = formatExplanationLines(current_state?.explanation);

  return (
    <div className="space-y-4">
      {/* 1. Hero Summary Card */}
      <div className="p-5 bg-white rounded-xl border border-gray-200/70 shadow-2xs">
        <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-4">
          <div className="space-y-1.5 max-w-xl">
            <div className="flex items-center gap-2">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-400">Active Task</span>
              <SectionStatusPill status={current_task?.status} />
            </div>
            <h3 className="text-lg font-bold text-gray-900 tracking-tight">
              {current_task?.title || <span className="text-gray-400 font-normal italic">No active task identified</span>}
            </h3>
            <div className="flex items-center gap-3 text-xs text-gray-500 pt-0.5">
              <span className="inline-flex items-center gap-1">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
                {context?.name} ({context?.lifecycle_state || 'active'})
              </span>
              <span className="text-gray-300">•</span>
              <span>{current_state?.freshness_display || 'Active recently'}</span>
            </div>
          </div>

          {/* Confidence Score Pill */}
          <div className="flex sm:flex-col items-baseline sm:items-end justify-between sm:justify-start gap-1 p-3 sm:p-0 bg-gray-50 sm:bg-transparent rounded-lg">
            <div className="flex items-center gap-2">
              <span className="text-2xl font-bold text-gray-900 tracking-tight">{scorePct}%</span>
              <span className="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-50 text-emerald-700 border border-emerald-200/60 uppercase">
                {quality?.rating || 'COHERENT'}
              </span>
            </div>
            <span className="text-[11px] text-gray-400">Confidence Score</span>
          </div>
        </div>
      </div>

      {/* 2. Structured Two-Column Breakdown */}
      <div className="grid grid-cols-1 md:grid-cols-12 gap-4">
        {/* Left Column (Primary Story & Activity - 7 cols) */}
        <div className="md:col-span-7 space-y-4">
          {/* Last Activity Card */}
          <div className="p-4 bg-white rounded-xl border border-gray-200/70 shadow-2xs space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-400">
                Last Verified Activity
              </span>
              <SectionStatusPill status={last_meaningful_activity?.status} />
            </div>
            <div className="text-sm font-medium text-gray-900">
              {last_meaningful_activity?.description || <span className="text-gray-400 italic font-normal">No activity recorded</span>}
            </div>
            {last_meaningful_activity?.timestamp && (
              <div className="text-xs text-gray-400 font-mono">
                {new Date(last_meaningful_activity.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}
              </div>
            )}
          </div>

          {/* Engine Rationale Card */}
          <div className="p-4 bg-white rounded-xl border border-gray-200/70 shadow-2xs space-y-2.5">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-400">
              Evidence & Rationale
            </span>
            {explanationBullets.length > 0 ? (
              <ul className="text-xs text-gray-600 space-y-1.5">
                {explanationBullets.map((bullet, idx) => (
                  <li key={idx} className="flex items-start gap-2">
                    <span className="text-emerald-600 font-bold select-none">•</span>
                    <span>{bullet}</span>
                  </li>
                ))}
              </ul>
            ) : confidence?.reasons && confidence.reasons.length > 0 ? (
              <ul className="text-xs text-gray-600 space-y-1.5">
                {confidence.reasons.map((r, idx) => (
                  <li key={idx} className="flex items-start gap-2">
                    <span className="text-emerald-600 font-bold select-none">•</span>
                    <span>{r}</span>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-xs text-gray-400 italic">No explicit rationale recorded.</p>
            )}
          </div>
        </div>

        {/* Right Column (Vitals & Resumability - 5 cols) */}
        <div className="md:col-span-5 space-y-4">
          {/* Context Vitals */}
          <div className="p-4 bg-white rounded-xl border border-gray-200/70 shadow-2xs space-y-3">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-400">
              Workspace Vitals
            </span>
            <div className="space-y-2.5 text-xs divide-y divide-gray-100">
              <div className="flex items-center justify-between pt-1">
                <span className="text-gray-500">Working Set</span>
                <span className="font-medium text-gray-800">{reconstruction.workset?.total_count ?? 0} assets</span>
              </div>
              <div className="flex items-center justify-between pt-2">
                <span className="text-gray-500">Memories</span>
                <span className="font-medium text-gray-800">{reconstruction.important_memories?.total_count ?? 0} saved</span>
              </div>
              <div className="flex items-center justify-between pt-2">
                <span className="text-gray-500">State Freshness</span>
                <span className="font-medium text-gray-800 capitalize">{current_state?.freshness || 'Active'}</span>
              </div>
              <div className="flex items-center justify-between pt-2">
                <span className="text-gray-500">Contradictions</span>
                <span className="font-medium text-emerald-700">
                  {quality?.factors?.contradiction_count ?? 0} detected
                </span>
              </div>
            </div>
          </div>

          {/* Resumability Status */}
          <div className="p-4 bg-white rounded-xl border border-gray-200/70 shadow-2xs space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-400">
                Resumability
              </span>
              <span className="text-[11px] font-semibold text-gray-700 capitalize">
                {resume_readiness?.readiness || 'Ready'}
              </span>
            </div>
            <p className="text-xs text-gray-600 leading-relaxed">
              {resume_readiness?.suggested_action || quality?.explanation || 'Context is coherent and ready to resume.'}
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Minimal Working Set Tab
// ---------------------------------------------------------------------------

function MinimalWorksetTab({ reconstruction }: { reconstruction: ContextReconstruction }) {
  const { workset, important_memories } = reconstruction;

  return (
    <div className="space-y-4">
      {/* Workset Items */}
      <div className="bg-white rounded-xl border border-gray-200/70 shadow-2xs overflow-hidden">
        <div className="px-4 py-3 border-b border-gray-100 flex items-center justify-between">
          <div>
            <h4 className="text-xs font-semibold text-gray-800">Working Set Assets</h4>
            <p className="text-[11px] text-gray-400">
              {workset?.total_count ?? 0} items ({workset?.primary_count ?? 0} primary, {workset?.supporting_count ?? 0} supporting)
            </p>
          </div>
          <SectionStatusPill status={workset?.status} />
        </div>

        {workset?.items && workset.items.length > 0 ? (
          <div className="divide-y divide-gray-100 text-xs">
            {workset.items.map((item) => (
              <div key={item.id} className="px-4 py-3 hover:bg-gray-50/70 transition-colors flex items-center justify-between gap-4">
                <div className="flex items-center gap-3 min-w-0">
                  <div className="w-6 h-6 rounded bg-gray-100 flex items-center justify-center text-gray-500 flex-shrink-0">
                    <FileText size={12} />
                  </div>
                  <div className="min-w-0">
                    <div className="font-medium text-gray-900 truncate">
                      {item.title || item.content_snippet || 'Untitled item'}
                    </div>
                    <div className="text-[11px] text-gray-400 truncate">
                      {item.app_name || item.domain || item.item_type}
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-3 flex-shrink-0">
                  <span className={`px-2 py-0.5 rounded text-[10px] font-semibold uppercase ${
                    item.relevance === 'primary' ? 'bg-emerald-50 text-emerald-700' : 'bg-gray-100 text-gray-600'
                  }`}>
                    {item.relevance}
                  </span>
                  <span className="text-xs font-semibold text-gray-600 w-8 text-right">
                    {Math.round(item.relevance_score * 100)}%
                  </span>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <div className="p-8 text-center text-xs text-gray-400 italic">No assets in current working set.</div>
        )}
      </div>

      {/* Important Memories */}
      <div className="bg-white rounded-xl border border-gray-200/70 shadow-2xs overflow-hidden">
        <div className="px-4 py-3 border-b border-gray-100 flex items-center justify-between">
          <div>
            <h4 className="text-xs font-semibold text-gray-800">Contextual Memories</h4>
            <p className="text-[11px] text-gray-400">{important_memories?.total_count ?? 0} recorded memories</p>
          </div>
          <SectionStatusPill status={important_memories?.status} />
        </div>

        {important_memories?.memories && important_memories.memories.length > 0 ? (
          <div className="divide-y divide-gray-100 text-xs">
            {important_memories.memories.map((mem) => (
              <div key={mem.id} className="px-4 py-3 hover:bg-gray-50/70 transition-colors flex items-center justify-between gap-4">
                <div className="flex items-center gap-3 min-w-0">
                  <Bookmark size={13} className="text-amber-500 flex-shrink-0" />
                  <div className="min-w-0">
                    <div className="font-medium text-gray-900 truncate">{mem.title}</div>
                    <div className="text-[11px] text-gray-400 truncate">{mem.explanation}</div>
                  </div>
                </div>
                <span className="text-[10px] font-semibold uppercase text-amber-700 bg-amber-50 px-2 py-0.5 rounded flex-shrink-0">
                  {mem.importance_band}
                </span>
              </div>
            ))}
          </div>
        ) : (
          <div className="p-8 text-center text-xs text-gray-400 italic">No long-term memories recorded.</div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Minimal Quality Tab
// ---------------------------------------------------------------------------

function MinimalQualityTab({ reconstruction }: { reconstruction: ContextReconstruction }) {
  const { quality } = reconstruction;
  const factors = quality?.factors;

  const dims = [
    { label: 'Context Confidence', value: factors?.context_confidence || 'strong' },
    { label: 'Task Confidence', value: factors?.task_confidence || 'strong' },
    { label: 'State Confidence', value: factors?.state_confidence || 'strong' },
    { label: 'Workset Quality', value: factors?.workset_quality || 'moderate' },
    { label: 'Memory Quality', value: factors?.memory_quality || 'moderate' },
    { label: 'Snapshot Freshness', value: factors?.snapshot_freshness || 'fresh' },
    { label: 'Evidence Consistency', value: factors?.evidence_consistency || 'consistent' },
    { label: 'Data Freshness', value: factors?.data_freshness || 'recent' },
  ];

  return (
    <div className="space-y-4">
      {/* Quality Summary Banner */}
      <div className="p-4 bg-white rounded-xl border border-gray-200/70 shadow-2xs space-y-1.5">
        <div className="flex items-center justify-between">
          <span className="text-xs font-semibold text-gray-900">{quality?.display_label || 'Coherent Reconstruction'}</span>
          <span className="px-2 py-0.5 rounded text-[10px] font-semibold bg-emerald-50 text-emerald-700 uppercase">
            {quality?.rating || 'COHERENT'}
          </span>
        </div>
        <p className="text-xs text-gray-600 leading-relaxed">{quality?.explanation}</p>
      </div>

      {/* Dimensions Grid */}
      <div className="bg-white rounded-xl border border-gray-200/70 shadow-2xs p-4 space-y-3">
        <h4 className="text-xs font-semibold text-gray-800">Quality Dimensions (9 Factors)</h4>
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs">
          {dims.map((d, i) => (
            <div key={i} className="p-2.5 bg-gray-50 rounded-lg space-y-1">
              <span className="text-[10px] text-gray-400 font-medium block truncate">{d.label}</span>
              <span className="font-semibold text-gray-800 uppercase text-[11px]">{d.value}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Contradictions & Gaps */}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        <div className="p-4 bg-white rounded-xl border border-gray-200/70 shadow-2xs space-y-2">
          <h4 className="text-xs font-semibold text-gray-800">Contradictions Detected</h4>
          {factors?.contradictions && factors.contradictions.length > 0 ? (
            <ul className="text-xs text-rose-700 space-y-1">
              {factors.contradictions.map((c, idx) => (
                <li key={idx} className="flex items-start gap-1.5">
                  <span className="text-rose-500 font-bold">•</span>
                  <span>{c}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-xs text-emerald-700 font-medium">0 detected · Clean consistency</p>
          )}
        </div>

        <div className="p-4 bg-white rounded-xl border border-gray-200/70 shadow-2xs space-y-2">
          <h4 className="text-xs font-semibold text-gray-800">Evidence Gaps</h4>
          {factors?.gaps && factors.gaps.length > 0 ? (
            <ul className="text-xs text-amber-700 space-y-1">
              {factors.gaps.map((g, idx) => (
                <li key={idx} className="flex items-start gap-1.5">
                  <span className="text-amber-500 font-bold">•</span>
                  <span>{g}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-xs text-emerald-700 font-medium">0 gaps · Fully specified evidence</p>
          )}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Minimal Diff Tab
// ---------------------------------------------------------------------------

function MinimalDiffTab({
  current,
  previous,
}: {
  current: ContextReconstruction;
  previous: ContextReconstruction | null;
}) {
  if (!previous) {
    return (
      <div className="p-8 text-center text-xs text-gray-400">
        No previous state snapshot in this session. Trigger &ldquo;Refresh&rdquo; or &ldquo;Cold Test&rdquo; to compare states.
      </div>
    );
  }

  const confidenceDelta = Math.round(((current.confidence?.score ?? 0) - (previous.confidence?.score ?? 0)) * 100);

  return (
    <div className="bg-white rounded-xl border border-gray-200/70 shadow-2xs overflow-hidden">
      <div className="px-4 py-3 border-b border-gray-100 flex items-center justify-between">
        <h4 className="text-xs font-semibold text-gray-800">Session State Comparison</h4>
        <span className="text-[11px] text-gray-400">
          Confidence Delta: <strong className={confidenceDelta >= 0 ? 'text-emerald-700' : 'text-rose-700'}>{confidenceDelta >= 0 ? `+${confidenceDelta}%` : `${confidenceDelta}%`}</strong>
        </span>
      </div>
      <table className="w-full text-xs text-left border-collapse">
        <thead>
          <tr className="border-b border-gray-100 bg-gray-50 text-gray-400 uppercase text-[10px] font-semibold">
            <th className="py-2.5 px-4">Field</th>
            <th className="py-2.5 px-4">Previous State</th>
            <th className="py-2.5 px-4 text-emerald-700">Current State</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-100 text-gray-700">
          <tr>
            <td className="py-2.5 px-4 font-medium text-gray-500">Active Task</td>
            <td className="py-2.5 px-4 text-gray-500">{previous.current_task?.title || 'None'}</td>
            <td className="py-2.5 px-4 font-semibold text-gray-900">{current.current_task?.title || 'None'}</td>
          </tr>
          <tr>
            <td className="py-2.5 px-4 font-medium text-gray-500">Operational State</td>
            <td className="py-2.5 px-4 text-gray-500 uppercase">{previous.current_state?.state_value || 'None'}</td>
            <td className="py-2.5 px-4 font-semibold text-gray-900 uppercase">{current.current_state?.state_value || 'None'}</td>
          </tr>
          <tr>
            <td className="py-2.5 px-4 font-medium text-gray-500">Working Set Count</td>
            <td className="py-2.5 px-4 text-gray-500">{previous.workset?.total_count ?? 0} assets</td>
            <td className="py-2.5 px-4 font-semibold text-gray-900">{current.workset?.total_count ?? 0} assets</td>
          </tr>
          <tr>
            <td className="py-2.5 px-4 font-medium text-gray-500">Confidence Score</td>
            <td className="py-2.5 px-4 text-gray-500">{Math.round((previous.confidence?.score ?? 0) * 100)}%</td>
            <td className="py-2.5 px-4 font-semibold text-emerald-700">{Math.round((current.confidence?.score ?? 0) * 100)}%</td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}
