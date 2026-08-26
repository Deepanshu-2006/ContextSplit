import { useEffect, useState } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { TopBar } from "./components/layout/TopBar";
import { Canvas } from "./components/workspace/Canvas";
import { db, Workspace } from "./services/db";
import { confirm } from "@tauri-apps/plugin-dialog";
import "./App.css";

function App() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    async function loadWorkspaces() {
      try {
        const loaded = await db.listWorkspaces();
        setWorkspaces(loaded);
        if (loaded.length > 0) {
          setActiveWorkspaceId(loaded[0].id);
        }
      } catch (err) {
        console.error("Failed to load workspaces:", err);
      } finally {
        setIsLoading(false);
      }
    }
    loadWorkspaces();
  }, []);

  const handleCreate = async () => {
    try {
      const newWs = await db.createWorkspace("New Workspace");
      setWorkspaces((prev) => [newWs, ...prev]);
      setActiveWorkspaceId(newWs.id);
    } catch (err) {
      console.error("Failed to create workspace:", err);
    }
  };

  const handleSelect = (id: string) => {
    setActiveWorkspaceId(id);
  };

  const handleRename = async (id: string, newName: string) => {
    try {
      const updated = await db.updateWorkspace(id, newName);
      setWorkspaces((prev) => prev.map((ws) => (ws.id === id ? updated : ws)));
    } catch (err) {
      console.error("Failed to rename workspace:", err);
    }
  };

  const handleDelete = async (id: string) => {
    const isConfirmed = await confirm(
      "Are you sure you want to delete this workspace? This action cannot be undone.",
      { title: "Delete Workspace", kind: "warning" }
    );
    if (!isConfirmed) return;
    
    try {
      await db.deleteWorkspace(id);
      setWorkspaces((prev) => {
        const filtered = prev.filter((ws) => ws.id !== id);
        // If we deleted the active workspace, select the first available one
        if (activeWorkspaceId === id) {
          setActiveWorkspaceId(filtered.length > 0 ? filtered[0].id : null);
        }
        return filtered;
      });
    } catch (err) {
      console.error("Failed to delete workspace:", err);
    }
  };

  const activeWorkspace = workspaces.find((ws) => ws.id === activeWorkspaceId);

  if (isLoading) {
    return <div className="w-screen h-screen bg-white flex items-center justify-center">Loading...</div>;
  }

  return (
    <div className="w-screen h-screen flex overflow-hidden bg-white font-sans text-gray-900">
      <Sidebar 
        workspaces={workspaces}
        activeWorkspaceId={activeWorkspaceId}
        onSelect={handleSelect}
        onCreate={handleCreate}
        onRename={handleRename}
        onDelete={handleDelete}
      />
      <div className="flex-1 flex flex-col min-w-0">
        <TopBar />
        <Canvas activeWorkspace={activeWorkspace} />
      </div>
    </div>
  );
}

export default App;
