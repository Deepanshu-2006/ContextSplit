import { useState } from "react";
import { Search, Sparkles, Settings, Clock, Layers } from "lucide-react";
import { SettingsModal } from "../settings/SettingsModal";
import { useDiscoveryStore } from "../../store/discoveryStore";
import { useWorkspaceStore } from "../../store/workspaceStore";

export function TopBar() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const { candidates, runDiscoveryScan, isLoading } = useDiscoveryStore();
  const { activeWorkspaceId, workspaces, openTimeline, openOverview } = useWorkspaceStore();

  const currentWorkspace = workspaces.find((w) => w.id === activeWorkspaceId);
  const detectedCount = candidates.filter((c) => c.status === "detected").length;

  return (
    <header 
      // data-tauri-drag-region allows the user to drag the window from the top bar if decorations are disabled
      data-tauri-drag-region 
      className="h-14 w-full flex items-center justify-between px-6 bg-white/50 backdrop-blur-md border-b border-black/4 select-none sticky top-0 z-10"
    >
      <div className="flex items-center gap-4 flex-1">
        <div className="relative group w-64">
          <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
            <Search size={14} className="text-gray-400 group-hover:text-gray-600 transition-colors" />
          </div>
          <input
            type="text"
            placeholder="Search anything..."
            className="w-full bg-gray-50/50 hover:bg-gray-100/50 border border-transparent rounded-md py-1.5 pl-9 pr-3 text-[13px] placeholder-gray-400 focus:outline-none transition-all shadow-sm"
          />
        </div>
      </div>

      <div className="flex items-center gap-2.5">
        {currentWorkspace && (
          <>
            <button
              onClick={() => openOverview(currentWorkspace.id)}
              title={`View ${currentWorkspace.name} Context Overview`}
              className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium text-violet-700 bg-violet-50/70 hover:bg-violet-100/70 rounded-lg border border-violet-200/60 transition-colors cursor-pointer"
            >
              <Layers size={13} className="text-violet-600" />
              <span>Overview</span>
            </button>

            <button
              onClick={() => openTimeline(currentWorkspace.id)}
              title={`View ${currentWorkspace.name} Context Timeline`}
              className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium text-indigo-700 bg-indigo-50/70 hover:bg-indigo-100/70 rounded-lg border border-indigo-200/60 transition-colors cursor-pointer"
            >
              <Clock size={13} className="text-indigo-600" />
              <span>Timeline</span>
            </button>
          </>
        )}

        <button
          onClick={runDiscoveryScan}
          disabled={isLoading}
          title="Run Context Discovery scan"
          className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium text-gray-600 hover:text-gray-900 hover:bg-gray-100/80 rounded-lg border border-gray-200/60 transition-colors cursor-pointer"
        >
          <Sparkles size={13} className={`text-amber-500 ${isLoading ? "animate-spin" : ""}`} />
          <span>Discover</span>
          {detectedCount > 0 && (
            <span className="w-1.5 h-1.5 rounded-full bg-amber-500 animate-pulse" />
          )}
        </button>

        <button 
          className="p-1.5 text-gray-400 hover:text-gray-900 transition-colors rounded-lg hover:bg-gray-100/80 cursor-pointer"
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

