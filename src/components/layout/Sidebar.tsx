import { Plus, LayoutGrid, Search, LayoutTemplate, Layers } from "lucide-react";

const WORKSPACES = [
  { id: "1", name: "Portfolio Website", icon: LayoutTemplate, active: true },
  { id: "2", name: "Spotify Clone", icon: Layers, active: false },
  { id: "3", name: "Hackathon Project", icon: LayoutGrid, active: false },
];

export function Sidebar() {
  return (
    <aside className="w-64 h-full bg-[#FAFAFA] border-r border-black/[0.04] flex flex-col pt-8 pb-4 select-none">
      <div className="px-6 mb-8 flex items-center gap-2">
        <div className="w-5 h-5 rounded-md bg-black shadow-sm" />
        <h1 className="font-semibold text-[15px] tracking-tight text-gray-900">
          ContextSplit
        </h1>
      </div>

      <div className="px-4 mb-2">
        <p className="px-2 text-xs font-medium text-gray-400 tracking-wider uppercase mb-2">
          Workspaces
        </p>
        <nav className="flex flex-col gap-0.5">
          {WORKSPACES.map((workspace) => (
            <button
              key={workspace.id}
              className={`flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-[13px] font-medium transition-colors ${
                workspace.active
                  ? "bg-black/5 text-gray-900"
                  : "text-gray-500 hover:text-gray-900 hover:bg-black/[0.02]"
              }`}
            >
              <workspace.icon
                size={14}
                className={workspace.active ? "text-gray-900" : "text-gray-400"}
              />
              {workspace.name}
            </button>
          ))}
        </nav>
      </div>

      <div className="px-4 mt-auto">
        <button className="w-full flex items-center justify-center gap-2 px-2 py-2 rounded-lg border border-black/5 text-[13px] font-medium text-gray-600 hover:bg-black/5 hover:text-gray-900 transition-colors shadow-sm bg-white">
          <Plus size={14} />
          New Workspace
        </button>
      </div>
    </aside>
  );
}
