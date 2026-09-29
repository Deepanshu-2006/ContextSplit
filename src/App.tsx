import { useEffect } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { TopBar } from "./components/layout/TopBar";
import { Canvas } from "./components/workspace/Canvas";
import { useWorkspaceStore } from "./store/workspaceStore";
import { useDiscoveryStore } from "./store/discoveryStore";
import { DiscoveryNotificationCard } from "./components/discovery/DiscoveryNotificationCard";
import { CandidateReviewModal } from "./components/discovery/CandidateReviewModal";
import { ContextTimelineDrawer } from "./components/timeline/ContextTimelineDrawer";
import { ContextOverviewPanel } from "./components/workspace/ContextOverviewPanel";
import { ContextReconstructionInspector } from "./components/workspace/ContextReconstructionInspector";
import { ContextDiagnosticsScreen } from "./components/diagnostics/ContextDiagnosticsScreen";
import { Toaster } from 'sonner';
import { CaptureTarget } from "./components/workspace/CaptureTarget";
import { CaptureOrb } from "./components/workspace/CaptureOrb";
import { useDragAndDrop } from "./hooks/useDragAndDrop";
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import { useLauncherStore } from "./store/launcherStore";
import { ContextLauncher } from "./components/launcher/ContextLauncher";
import { Terminal } from "lucide-react";
import "./App.css";

function App() {
  const {
    isLoading,
    loadWorkspaces,
    listenForClipboard,
    activeWorkspaceId,
    isReconstructionInspectorOpen,
    reconstructionInspectorWorkspaceId,
    closeReconstructionInspector,
    isDiagnosticsOpen,
    toggleDiagnostics,
    closeDiagnostics,
  } = useWorkspaceStore();
  const { checkForCandidates } = useDiscoveryStore();
  const { toggleLauncher } = useLauncherStore();
  
  const windowLabel = typeof window !== 'undefined' ? getCurrentWindow().label : '';
  const isNotchTarget = windowLabel === "macos-notch-target";
  const isOrbTarget = windowLabel === "windows-orb-target";
  
  // Initialize global drag and drop listeners
  useDragAndDrop();

  useEffect(() => {
    // Global keyboard shortcuts
    const handleKeyDown = (e: KeyboardEvent) => {
      // Cmd+Shift+D or Ctrl+Shift+D: Dev Diagnostics
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && (e.key === 'D' || e.key === 'd')) {
        e.preventDefault();
        toggleDiagnostics();
      }
      // Cmd+K or Ctrl+K or Alt+Space: Context Launcher
      if (((e.metaKey || e.ctrlKey) && (e.key === 'k' || e.key === 'K')) || (e.altKey && (e.code === 'Space' || e.key === ' '))) {
        e.preventDefault();
        toggleLauncher();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [toggleDiagnostics, toggleLauncher]);

  useEffect(() => {
    // Only main window loads workspaces and clipboard monitor
    if (isNotchTarget || isOrbTarget) {
      return;
    }

    // Ensure system notification permissions are granted
    const initNotifications = async () => {
      try {
        let granted = await isPermissionGranted();
        if (!granted) {
          const permission = await requestPermission();
          granted = permission === 'granted';
        }
      } catch (err) {
        console.warn("Could not request notification permissions:", err);
      }
    };
    initNotifications();

    loadWorkspaces();
    checkForCandidates();

    let isMounted = true;
    let unlisten: (() => void) | undefined;
    
    listenForClipboard().then((unlistenFn) => {
      if (isMounted) {
        unlisten = unlistenFn;
      } else {
        unlistenFn();
      }
    });

    return () => {
      isMounted = false;
      if (unlisten) {
        unlisten();
      }
    };
  }, [loadWorkspaces, listenForClipboard, checkForCandidates, isNotchTarget, isOrbTarget]);

  if (isNotchTarget) {
    return (
      <div className="w-screen h-screen flex flex-col items-center justify-start bg-transparent overflow-hidden">
        <CaptureTarget />
      </div>
    );
  }

  if (isOrbTarget) {
    return (
      <div className="w-screen h-screen flex justify-center bg-transparent">
        <CaptureOrb />
      </div>
    );
  }

  if (isLoading) {
    return <div className="w-screen h-screen bg-white flex items-center justify-center">Loading...</div>;
  }

  return (
    <div className="w-screen h-screen flex overflow-hidden bg-white font-sans text-stone-900">
      <Sidebar />
      <div className="flex-1 flex flex-col min-w-0">
        {activeWorkspaceId !== "inbox" && <TopBar />}
        <Canvas />
      </div>
      <DiscoveryNotificationCard />
      <CandidateReviewModal />
      <ContextTimelineDrawer />
      <ContextOverviewPanel />
      <ContextLauncher />
      {import.meta.env.DEV && isReconstructionInspectorOpen && reconstructionInspectorWorkspaceId && (
        <ContextReconstructionInspector
          workspaceId={reconstructionInspectorWorkspaceId}
          onClose={closeReconstructionInspector}
        />
      )}
      {import.meta.env.DEV && isDiagnosticsOpen && (
        <ContextDiagnosticsScreen
          onClose={closeDiagnostics}
        />
      )}
      {import.meta.env.DEV && !isDiagnosticsOpen && (
        <button
          onClick={toggleDiagnostics}
          title="Open Developer Diagnostics (Cmd+Shift+D)"
          className="fixed bottom-3 left-3 z-40 flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-slate-900/90 hover:bg-slate-900 text-slate-300 hover:text-emerald-400 border border-slate-700/60 shadow-lg backdrop-blur text-[11px] font-mono transition-all group cursor-pointer"
        >
          <Terminal className="w-3.5 h-3.5 text-emerald-400" />
          <span className="font-semibold text-white">Dev Diagnostics</span>
          <span className="text-[10px] text-slate-400 opacity-60 group-hover:opacity-100">⌘⇧D</span>
        </button>
      )}
      <Toaster
        position="bottom-right"
        toastOptions={{
          className: "!bg-gray-950/95 !backdrop-blur-xl !text-white !border !border-white/10 !rounded-2xl !shadow-2xl !text-[13px] !font-medium",
          actionButtonStyle: {
            background: "rgba(255, 255, 255, 0.12)",
            color: "#ffffff",
            borderRadius: "9999px",
            fontSize: "12px",
            fontWeight: "500",
            padding: "4px 12px",
            border: "1px solid rgba(255, 255, 255, 0.18)",
            cursor: "pointer",
          },
        }}
      />
    </div>
  );
}

export default App;

