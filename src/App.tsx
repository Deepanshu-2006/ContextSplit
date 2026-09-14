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
import { Toaster } from 'sonner';
import { CaptureTarget } from "./components/workspace/CaptureTarget";
import { CaptureOrb } from "./components/workspace/CaptureOrb";
import { useDragAndDrop } from "./hooks/useDragAndDrop";
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import "./App.css";

function App() {
  const { isLoading, loadWorkspaces, listenForClipboard, activeWorkspaceId } = useWorkspaceStore();
  const { checkForCandidates } = useDiscoveryStore();
  
  const windowLabel = typeof window !== 'undefined' ? getCurrentWindow().label : '';
  const isNotchTarget = windowLabel === "macos-notch-target";
  const isOrbTarget = windowLabel === "windows-orb-target";
  
  // Initialize global drag and drop listeners
  useDragAndDrop();

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
    <div className="w-screen h-screen flex overflow-hidden bg-white font-sans text-gray-900">
      <Sidebar />
      <div className="flex-1 flex flex-col min-w-0">
        {activeWorkspaceId !== "inbox" && <TopBar />}
        <Canvas />
      </div>
      <DiscoveryNotificationCard />
      <CandidateReviewModal />
      <ContextTimelineDrawer />
      <ContextOverviewPanel />
      <Toaster position="bottom-right" />
    </div>
  );
}

export default App;

