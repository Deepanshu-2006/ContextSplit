import { Sidebar } from "./components/layout/Sidebar";
import { TopBar } from "./components/layout/TopBar";
import { Canvas } from "./components/workspace/Canvas";
import "./App.css";

function App() {
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
