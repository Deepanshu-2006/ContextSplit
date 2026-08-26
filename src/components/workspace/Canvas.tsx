import { BoxSelect, LayoutTemplate } from "lucide-react";
import { ReactFlow, Background, BackgroundVariant } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useWorkspaceStore } from "../../store/workspaceStore";

export function Canvas() {
  const { workspaces, activeWorkspaceId } = useWorkspaceStore();
  const activeWorkspace = workspaces.find((ws) => ws.id === activeWorkspaceId);

  if (!activeWorkspace) {
    return (
      <div 
        className="flex-1 w-full h-full relative bg-[#FDFDFD] flex items-center justify-center"
      >
        <div className="flex flex-col items-center text-center p-8 max-w-sm">
          <div className="w-16 h-16 rounded-2xl bg-gray-50 border border-gray-100 flex items-center justify-center mb-6 shadow-sm">
            <LayoutTemplate className="text-gray-300" size={32} strokeWidth={1.5} />
          </div>
          <h2 className="text-xl font-medium text-gray-900 mb-2 tracking-tight">
            No Workspace Selected
          </h2>
          <p className="text-[14px] text-gray-500 leading-relaxed">
            Create a new workspace from the sidebar to start collecting your context.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 w-full h-full relative bg-[#FDFDFD] overflow-hidden flex flex-col">
      <ReactFlow
        nodes={[]}
        edges={[]}
        minZoom={0.1}
        maxZoom={4}
        defaultViewport={{ x: 0, y: 0, zoom: 1 }}
        proOptions={{ hideAttribution: true }}
        fitView
      >
        <Background 
          color="#9CA3AF" 
          variant={BackgroundVariant.Dots} 
          gap={24} 
          size={2} 
        />
        
        {/* Empty State Overlay */}
        <div className="absolute inset-0 pointer-events-none flex flex-col items-center justify-center p-8 z-10">
          <div className="w-16 h-16 rounded-2xl bg-gray-50 border border-gray-100 flex items-center justify-center mb-6 shadow-sm">
            <BoxSelect className="text-gray-300" size={32} strokeWidth={1.5} />
          </div>
          
          <h2 className="text-xl font-medium text-gray-900 mb-2 tracking-tight">
            {activeWorkspace.name}
          </h2>
          
          <p className="text-[14px] text-gray-500 max-w-sm text-center leading-relaxed">
            This workspace is empty. Drop images, code snippets, colors, or links anywhere on the canvas to add them to this context.
          </p>
        </div>
      </ReactFlow>
    </div>
  );
}
