import { BoxSelect } from "lucide-react";

export function Canvas() {
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
          Welcome to Portfolio Website
        </h2>
        
        <p className="text-[14px] text-gray-500 max-w-sm text-center leading-relaxed">
          This workspace is empty. Drop images, code snippets, colors, or links anywhere on the canvas to add them to this context.
        </p>
      </div>
    </div>
  );
}
