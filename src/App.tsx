import { useEffect } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { TopBar } from "./components/layout/TopBar";
import { Canvas } from "./components/workspace/Canvas";
import { useWorkspaceStore } from "./store/workspaceStore";
import "./App.css";

function App() {
  const { isLoading, loadWorkspaces, listenForClipboard } = useWorkspaceStore();

  useEffect(() => {
    loadWorkspaces();
    listenForClipboard();
  }, [loadWorkspaces, listenForClipboard]);

  if (isLoading) {
    return <div className="w-screen h-screen bg-white flex items-center justify-center">Loading...</div>;
  }

  return (
    <div className="w-screen h-screen flex overflow-hidden bg-white font-sans text-gray-900">
      <Sidebar />
      <div className="flex-1 flex flex-col min-w-0">
        <TopBar />
        <Canvas />
      </div>
    </div>
  );
}

export default App;
