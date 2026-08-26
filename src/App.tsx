import { useEffect } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { TopBar } from "./components/layout/TopBar";
import { Canvas } from "./components/workspace/Canvas";
import { db } from "./services/db";
import "./App.css";

function App() {
  useEffect(() => {
    async function testDb() {
      try {
        console.log("Testing DB connection...");
        
        // 1. Create
        const newWorkspace = await db.createWorkspace("Test Workspace");
        console.log("Created:", newWorkspace);
        
        // 2. Read (Get)
        const fetched = await db.getWorkspace(newWorkspace.id);
        console.log("Fetched:", fetched);
        
        // 3. Update
        const updated = await db.updateWorkspace(fetched.id, "Updated Workspace");
        console.log("Updated:", updated);
        
        // 4. List
        const allWorkspaces = await db.listWorkspaces();
        console.log("All Workspaces:", allWorkspaces);
        
        // 5. Delete
        await db.deleteWorkspace(updated.id);
        console.log("Deleted. Final list:", await db.listWorkspaces());

      } catch (err) {
        console.error("DB Test failed:", err);
      }
    }
    
    testDb();
  }, []);

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
