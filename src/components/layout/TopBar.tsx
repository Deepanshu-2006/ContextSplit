import { useState } from "react";
import { Search, Bell, Settings } from "lucide-react";
import { SettingsModal } from "../settings/SettingsModal";

export function TopBar() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  return (
    <header 
      // data-tauri-drag-region allows the user to drag the window from the top bar if decorations are disabled
      data-tauri-drag-region 
      className="h-14 w-full flex items-center justify-between px-6 bg-white/50 backdrop-blur-md border-b border-black/4 select-none sticky top-0 z-10"
    >
      <div className="flex items-center gap-4 flex-1">
        <div className="relative group w-64">
          <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
            <Search size={14} className="text-gray-400 group-hover:text-gray-600 transition-colors" />
          </div>
          <input
            type="text"
            placeholder="Search anything..."
            className="w-full bg-gray-50/50 hover:bg-gray-100/50 border border-transparent rounded-md py-1.5 pl-9 pr-3 text-[13px] placeholder-gray-400 focus:outline-none transition-all shadow-sm"
          />
        </div>
      </div>

      <div className="flex items-center gap-3">
        <button className="text-gray-400 hover:text-gray-900 transition-colors">
          <Bell size={16} />
        </button>
        <button 
          className="text-gray-400 hover:text-gray-900 transition-colors"
          onClick={() => setIsSettingsOpen(true)}
        >
          <Settings size={16} />
        </button>
      </div>
      
      {isSettingsOpen && (
        <SettingsModal onClose={() => setIsSettingsOpen(false)} />
      )}
    </header>
  );
}
