import { useEffect, useState } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { TopBar } from "./components/layout/TopBar";
import { Canvas } from "./components/workspace/Canvas";
import { useWorkspaceStore } from "./store/workspaceStore";
import { Toaster } from 'sonner';
import { CaptureTarget } from "./components/workspace/CaptureTarget";
import { CaptureOrb } from "./components/workspace/CaptureOrb";
import { useDragAndDrop } from "./hooks/useDragAndDrop";
import { getCurrentWindow } from '@tauri-apps/api/window';
import "./App.css";

function App() {
  const { isLoading, loadWorkspaces, listenForClipboard } = useWorkspaceStore();
  const [isNotchTarget, setIsNotchTarget] = useState(false);
  const [isOrbTarget, setIsOrbTarget] = useState(false);
  
  // Initialize global drag and drop listeners
  useDragAndDrop();

  useEffect(() => {
    // Check if we are rendering in the secondary macOS overlay window
    if (getCurrentWindow().label === "macos-notch-target") {
      setIsNotchTarget(true);
      return; // Skip loading workspaces and clipboard for the notch window
    }

    if (getCurrentWindow().label === "windows-orb-target") {
      setIsOrbTarget(true);
      return;
    }

    loadWorkspaces();
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
  }, [loadWorkspaces, listenForClipboard]);

  if (isNotchTarget) {
    return (
      <div className="w-screen h-screen flex justify-center bg-transparent">
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
      <CaptureTarget />
      <Sidebar />
      <div className="flex-1 flex flex-col min-w-0">
        <TopBar />
        <Canvas />
      </div>
      <Toaster position="bottom-right" />
    </div>
  );
}

export default App;
