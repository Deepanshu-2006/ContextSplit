import { BoxSelect, LayoutTemplate } from "lucide-react";
import { Workspace } from "../../services/db";

interface CanvasProps {
  activeWorkspace?: Workspace;
}

export function Canvas({ activeWorkspace }: CanvasProps) {
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
    <div 
      className="flex-1 w-full h-full relative bg-[#FDFDFD] overflow-hidden flex flex-col"
      style={{
        backgroundImage: "radial-gradient(circle, #E5E7EB 1px, transparent 1px)",
        backgroundSize: "24px 24px"
      }}
    >
      <div className="flex-1 flex flex-col items-center justify-center p-8">
        <div className="w-16 h-16 rounded-2xl bg-gray-50 border border-gray-100 flex items-center justify-center mb-6 shadow-sm">
          <BoxSelect className="text-gray-300" size={32} strokeWidth={1.5} />
        </div>
        
        <h2 className="text-xl font-medium text-gray-900 mb-2 tracking-tight">
          Welcome to {activeWorkspace.name}
        </h2>
        
        <p className="text-[14px] text-gray-500 max-w-sm text-center leading-relaxed">
          This workspace is empty. Drop images, code snippets, colors, or links anywhere on the canvas to add them to this context.
        </p>
      </div>
    </div>
  );
}
