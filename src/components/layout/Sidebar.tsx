import { useState, useRef, useEffect } from "react";
import { Plus, LayoutTemplate, Trash2, Pencil, Inbox } from "lucide-react";
import { Workspace } from "../../services/db";
import { useWorkspaceStore } from "../../store/workspaceStore";

export function Sidebar() {
  const {
    workspaces,
    activeWorkspaceId,
    inboxCount,
    selectWorkspace,
    createWorkspace,
    renameWorkspace,
    deleteWorkspace,
  } = useWorkspaceStore();

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
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

  return (
    <aside className="w-64 h-full bg-[#FAFAFA] border-r border-black/[0.04] flex flex-col pt-8 pb-4 select-none">
      <div className="px-6 mb-8 flex items-center gap-2">
        <div className="w-5 h-5 rounded-md bg-black shadow-sm" />
        <h1 className="font-semibold text-[15px] tracking-tight text-gray-900">
          ContextSplit
        </h1>
      </div>

      <div className="px-4 mb-6">
        <div
          onClick={() => selectWorkspace("inbox")}
          className={`flex items-center justify-between px-2 py-1.5 rounded-lg text-[13px] font-medium transition-colors cursor-pointer ${
            activeWorkspaceId === "inbox"
              ? "bg-black/5 text-gray-900"
              : "text-gray-500 hover:text-gray-900 hover:bg-black/[0.02]"
          }`}
        >
          <div className="flex items-center gap-2.5">
            <Inbox size={14} className={activeWorkspaceId === "inbox" ? "text-gray-900" : "text-gray-400"} />
            <span>Inbox</span>
          </div>
          {inboxCount > 0 && (
            <span className="bg-black/5 text-gray-500 px-1.5 py-0.5 rounded-md text-[11px] font-semibold">
              {inboxCount}
            </span>
          )}
        </div>
      </div>

      <div className="px-4 mb-2">
        <p className="px-2 text-xs font-medium text-gray-400 tracking-wider uppercase mb-2">
          Workspaces
        </p>
        <nav className="flex flex-col gap-0.5">
          {workspaces.map((workspace) => {
            const isActive = workspace.id === activeWorkspaceId;
            const isEditing = workspace.id === editingId;

            return (
              <div
                key={workspace.id}
                onClick={() => !isEditing && selectWorkspace(workspace.id)}
                className={`group flex items-center justify-between px-2 py-1.5 rounded-lg text-[13px] font-medium transition-colors cursor-pointer ${
                  isActive
                    ? "bg-black/5 text-gray-900"
                    : "text-gray-500 hover:text-gray-900 hover:bg-black/[0.02]"
                }`}
              >
                <div className="flex items-center gap-2.5 flex-1 min-w-0">
                  <LayoutTemplate
                    size={14}
                    className={isActive ? "text-gray-900 shrink-0" : "text-gray-400 shrink-0"}
                  />
                  {isEditing ? (
                    <input
                      ref={inputRef}
                      type="text"
                      value={editValue}
                      onChange={(e) => setEditValue(e.target.value)}
                      onBlur={handleRenameSubmit}
                      onKeyDown={handleKeyDown}
                      className="bg-white border border-black/10 rounded px-1.5 py-0.5 w-full text-gray-900 outline-none text-[13px]"
                      onClick={(e) => e.stopPropagation()}
                    />
                  ) : (
                    <span className="truncate">{workspace.name}</span>
                  )}
                </div>

                {!isEditing && (
                  <div className="flex items-center opacity-0 group-hover:opacity-100 transition-opacity">
                    <button
                      onClick={(e) => startEditing(workspace, e)}
                      className="p-1 text-gray-400 hover:text-gray-900 rounded"
                      title="Rename"
                    >
                      <Pencil size={12} />
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        deleteWorkspace(workspace.id);
                      }}
                      className="p-1 text-gray-400 hover:text-red-500 rounded"
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
            <p className="px-2 text-[13px] text-gray-400 py-2">No workspaces yet.</p>
          )}
        </nav>
      </div>

      <div className="px-4 mt-auto">
        <button 
          onClick={() => createWorkspace("New Workspace")}
          className="w-full flex items-center justify-center gap-2 px-2 py-2 rounded-lg border border-black/5 text-[13px] font-medium text-gray-600 hover:bg-black/5 hover:text-gray-900 transition-colors shadow-sm bg-white"
        >
          <Plus size={14} />
          New Workspace
        </button>
      </div>
    </aside>
  );
}
