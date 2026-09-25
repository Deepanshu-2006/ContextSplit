import { useState, useRef, useEffect } from "react";
import { Plus, LayoutTemplate, Trash2, Pencil, Inbox, Clock, Search, Settings } from "lucide-react";
import { Workspace } from "../../services/db";
import { useWorkspaceStore } from "../../store/workspaceStore";
import { SettingsModal } from "../settings/SettingsModal";

export function Sidebar() {
  const {
    workspaces,
    activeWorkspaceId,
    inboxCount,
    isDraggingCard,
    hoveredWorkspaceId,
    selectWorkspace,
    createWorkspace,
    renameWorkspace,
    deleteWorkspace,
    openTimeline,
  } = useWorkspaceStore();

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editingId && inputRef.current) {
      inputRef.current.focus();
    }
  }, [editingId]);

  const startEditing = (ws: Workspace, e: React.MouseEvent) => {
    e.stopPropagation();
    setEditingId(ws.id);
    setEditValue(ws.name);
  };

  const handleRenameSubmit = () => {
    if (editingId && editValue.trim() !== "") {
      renameWorkspace(editingId, editValue.trim());
    }
    setEditingId(null);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") handleRenameSubmit();
    if (e.key === "Escape") setEditingId(null);
  };

  const handleSearchClick = () => {
    if (activeWorkspaceId !== "inbox") {
      selectWorkspace("inbox");
    }
    setTimeout(() => {
      const searchInput = document.getElementById("inbox-search-input") as HTMLInputElement | null;
      if (searchInput) {
        searchInput.focus();
        searchInput.select();
      }
    }, 50);
  };

  const handleTimelineClick = () => {
    const targetWsId = activeWorkspaceId !== "inbox" && activeWorkspaceId ? activeWorkspaceId : workspaces[0]?.id;
    if (targetWsId) {
      openTimeline(targetWsId);
    }
  };

  return (
    <>
      <aside className="w-60 h-full bg-[#FFFBF9] border-r border-orange-200/50 flex flex-col pt-7 pb-4 select-none shrink-0">
        {/* Brand Header */}
        <div className="px-5 mb-6 flex items-center gap-2.5">
          <div className="w-5 h-5 rounded-md bg-gray-900 shadow-xs flex items-center justify-center">
            <span className="w-2 h-2 rounded-xs bg-white" />
          </div>
          <h1 className="font-semibold text-[14px] tracking-tight text-stone-900">
            ContextSplit
          </h1>
        </div>

        {/* Primary Navigation */}
        <div className="px-3 mb-5 flex flex-col gap-0.5">
          {/* Inbox */}
          <div
            data-workspace-target-id="inbox"
            onClick={() => selectWorkspace("inbox")}
            className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg text-[13px] font-medium transition-all duration-150 cursor-pointer ${
              hoveredWorkspaceId === "inbox"
                ? "bg-rose-50 text-rose-600 ring-2 ring-rose-500/20"
                : activeWorkspaceId === "inbox"
                ? "bg-orange-100/50 text-orange-950 font-semibold"
                : isDraggingCard
                ? "text-stone-400 hover:text-stone-600"
                : "text-stone-600 hover:text-stone-900 hover:bg-black/3"
            }`}
          >
            <div className="flex items-center gap-2.5">
              <Inbox
                size={14}
                className={
                  hoveredWorkspaceId === "inbox"
                    ? "text-rose-500"
                    : activeWorkspaceId === "inbox"
                    ? "text-stone-900"
                    : "text-stone-400"
                }
              />
              <span>Inbox</span>
            </div>
            {inboxCount > 0 && (
              <span
                className={`px-1.5 py-0.2 rounded-md text-[11px] font-semibold ${
                  hoveredWorkspaceId === "inbox"
                    ? "bg-rose-100/50 text-rose-600"
                    : activeWorkspaceId === "inbox"
                    ? "bg-orange-200/50 text-orange-950"
                    : "bg-black/5 text-stone-500"
                }`}
              >
                {inboxCount}
              </span>
            )}
          </div>

          {/* Search */}
          <div
            onClick={handleSearchClick}
            className="flex items-center gap-2.5 px-2.5 py-1.5 rounded-lg text-[13px] font-medium text-stone-600 hover:text-stone-900 hover:bg-black/3 transition-all duration-150 cursor-pointer"
          >
            <Search size={14} className="text-stone-400" />
            <span>Search</span>
          </div>

          {/* Timeline */}
          <div
            onClick={handleTimelineClick}
            className="flex items-center gap-2.5 px-2.5 py-1.5 rounded-lg text-[13px] font-medium text-stone-600 hover:text-stone-900 hover:bg-black/3 transition-all duration-150 cursor-pointer"
          >
            <Clock size={14} className="text-stone-400" />
            <span>Timeline</span>
          </div>
        </div>

        {/* Workspaces Section */}
        <div className="px-3 flex-1 overflow-y-auto min-h-0 flex flex-col">
          <div className="px-2.5 mb-1.5 flex items-center justify-between">
            <span className="text-[11px] font-semibold text-stone-400 tracking-wider uppercase">
              Workspaces
            </span>
          </div>

          <nav className="flex flex-col gap-0.5">
            {workspaces.map((workspace) => {
              const isActive = workspace.id === activeWorkspaceId;
              const isEditing = workspace.id === editingId;
              const isHoveredTarget = workspace.id === hoveredWorkspaceId;

              return (
                <div
                  key={workspace.id}
                  data-workspace-target-id={workspace.id}
                  onClick={() => !isEditing && selectWorkspace(workspace.id)}
                  className={`group flex items-center justify-between px-2.5 py-1.5 rounded-lg text-[13px] font-medium transition-all duration-150 cursor-pointer ${
                    isHoveredTarget
                      ? "bg-rose-50 text-rose-600 ring-2 ring-rose-500/20"
                      : isActive
                      ? "bg-orange-100/50 text-orange-950 font-semibold"
                      : isDraggingCard
                      ? "text-stone-400 hover:text-stone-600"
                      : "text-stone-600 hover:text-stone-900 hover:bg-black/3"
                  }`}
                >
                  <div className="flex items-center gap-2.5 flex-1 min-w-0">
                    <LayoutTemplate
                      size={14}
                      className={
                        isHoveredTarget
                          ? "text-rose-500 shrink-0"
                          : isActive
                          ? "text-stone-900 shrink-0"
                          : "text-stone-400 shrink-0"
                      }
                    />
                    {isEditing ? (
                      <input
                        ref={inputRef}
                        type="text"
                        value={editValue}
                        onChange={(e) => setEditValue(e.target.value)}
                        onBlur={handleRenameSubmit}
                        onKeyDown={handleKeyDown}
                        className="bg-white border border-black/15 rounded px-1.5 py-0.5 w-full text-stone-900 outline-none text-[13px]"
                        onClick={(e) => e.stopPropagation()}
                      />
                    ) : (
                      <span className="truncate">{workspace.name}</span>
                    )}
                  </div>

                  {!isEditing && (
                    <div className="flex items-center opacity-0 group-hover:opacity-100 transition-opacity">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          openTimeline(workspace.id);
                        }}
                        className="p-1 text-stone-400 hover:text-gray-700 rounded"
                        title="Workspace Timeline"
                      >
                        <Clock size={12} />
                      </button>
                      <button
                        onClick={(e) => startEditing(workspace, e)}
                        className="p-1 text-stone-400 hover:text-gray-700 rounded"
                        title="Rename"
                      >
                        <Pencil size={12} />
                      </button>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteWorkspace(workspace.id);
                        }}
                        className="p-1 text-stone-400 hover:text-red-500 rounded"
                        title="Delete"
                      >
                        <Trash2 size={12} />
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
            {workspaces.length === 0 && (
              <p className="px-2.5 text-[12px] text-stone-400 py-2">No workspaces yet.</p>
            )}
          </nav>

          <button
            onClick={() => createWorkspace("New Workspace")}
            className="mt-2 flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-[12px] font-medium text-stone-500 hover:text-stone-900 hover:bg-black/3 transition-colors text-left cursor-pointer"
          >
            <Plus size={13} className="text-stone-400" />
            <span>New Workspace</span>
          </button>
        </div>

        {/* Footer: Settings at bottom */}
        <div className="px-3 pt-3 mt-auto border-t border-orange-200/50">
          <button
            onClick={() => setIsSettingsOpen(true)}
            className="w-full flex items-center gap-2.5 px-2.5 py-1.5 rounded-lg text-[13px] font-medium text-stone-600 hover:text-stone-900 hover:bg-black/3 transition-colors cursor-pointer"
          >
            <Settings size={14} className="text-stone-400" />
            <span>Settings</span>
          </button>
        </div>
      </aside>

      {isSettingsOpen && (
        <SettingsModal onClose={() => setIsSettingsOpen(false)} />
      )}
    </>
  );
}
