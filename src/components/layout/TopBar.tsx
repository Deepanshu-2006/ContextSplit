import { useState } from "react";
import { Search, Sparkles, Settings, Clock, Layers, Cpu } from "lucide-react";
import { SettingsModal } from "../settings/SettingsModal";
import { useDiscoveryStore } from "../../store/discoveryStore";
import { useWorkspaceStore } from "../../store/workspaceStore";

export function TopBar() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const { candidates, runDiscoveryScan, isLoading } = useDiscoveryStore();
  const {
    activeWorkspaceId,
    workspaces,
    openTimeline,
    openOverview,
    openReconstructionInspector,
  } = useWorkspaceStore();

  const currentWorkspace = workspaces.find((w) => w.id === activeWorkspaceId);
  const detectedCount = candidates.filter((c) => c.status === "detected").length;

  return (
    <header 
      className="h-14 w-full flex items-center justify-between px-6 bg-[#FFFBF9]/80 backdrop-blur-md border-b border-orange-200/50 select-none sticky top-0 z-10"
    >
      <div className="flex items-center gap-4 flex-1 min-w-0">
        {currentWorkspace && (
          <div className="flex items-center gap-2 min-w-0 pr-3 border-r border-orange-200/50 shrink-0">
            <span
              className={`w-2 h-2 rounded-full shrink-0 ${
                currentWorkspace.lifecycle_state === "dormant"
                  ? "bg-stone-300"
                  : currentWorkspace.lifecycle_state === "archived"
                  ? "bg-stone-400"
                  : "bg-emerald-500"
              }`}
              title={`Context status: ${currentWorkspace.lifecycle_state || "active"}`}
            />
            <h2 className="text-[13.5px] font-semibold text-stone-900 truncate max-w-[200px]">
              {currentWorkspace.name}
            </h2>
          </div>
        )}
        <div className="relative group w-64">
          <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
            <Search size={14} className="text-stone-400 group-hover:text-stone-600 transition-colors" />
          </div>
          <input
            type="text"
            placeholder="Search anything..."
            className="w-full bg-stone-100/50 hover:bg-stone-200/50 border border-transparent rounded-md py-1.5 pl-9 pr-3 text-[13px] placeholder-stone-400 focus:outline-none transition-all shadow-sm"
          />
        </div>
      </div>

      <div className="flex items-center gap-2.5">
        {currentWorkspace && (
          <>
            <button
              onClick={() => openOverview(currentWorkspace.id)}
              title={`View ${currentWorkspace.name} Context Overview`}
              className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium text-rose-700 bg-rose-50/70 hover:bg-rose-100/70 rounded-lg border border-rose-200/60 transition-colors cursor-pointer"
            >
              <Layers size={13} className="text-rose-600" />
              <span>Overview</span>
            </button>

            <button
              onClick={() => openTimeline(currentWorkspace.id)}
              title={`View ${currentWorkspace.name} Context Timeline`}
              className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium text-orange-700 bg-orange-50/70 hover:bg-orange-100/70 rounded-lg border border-orange-200/60 transition-colors cursor-pointer"
            >
              <Clock size={13} className="text-orange-600" />
              <span>Timeline</span>
            </button>

            {/* DEV-only Context Reconstruction Inspector */}
            {import.meta.env.DEV && (
              <button
                onClick={() => openReconstructionInspector(currentWorkspace.id)}
                title="[DEV ONLY] Open Context Reconstruction Inspector"
                className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium text-amber-700 bg-amber-50/80 hover:bg-amber-100/80 rounded-lg border border-amber-200/60 transition-colors cursor-pointer"
              >
                <Cpu size={13} className="text-amber-600" />
                <span>Inspector</span>
              </button>
            )}
          </>
        )}

        <button
          onClick={runDiscoveryScan}
          disabled={isLoading}
          title="Run Context Discovery scan"
          className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium text-orange-800 bg-orange-50/40 hover:bg-orange-100/60 rounded-lg border border-orange-200/60 transition-colors cursor-pointer"
        >
          <Sparkles size={13} className={`text-orange-500 ${isLoading ? "animate-spin" : ""}`} />
          <span>Discover</span>
          {detectedCount > 0 && (
            <span className="w-1.5 h-1.5 rounded-full bg-orange-500 animate-pulse" />
          )}
        </button>

        <button 
          className="p-1.5 text-stone-400 hover:text-stone-900 transition-colors rounded-lg hover:bg-stone-100/80 cursor-pointer"
          onClick={() => setIsSettingsOpen(true)}
          title="Settings"
        >
          <Settings size={16} />
        </button>
      </div>
      
      {isSettingsOpen && (
        <SettingsModal onClose={() => setIsSettingsOpen(false)} />
      )}
    </header>
  );
}

