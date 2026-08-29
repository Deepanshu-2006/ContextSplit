import { ReactNode, useState, useRef, useEffect } from "react";
import { Handle, Position } from "@xyflow/react";
import { MoreHorizontal, Inbox, BoxSelect } from "lucide-react";
import { useWorkspaceStore } from "../../../store/workspaceStore";
import { ContextInspector } from "../ContextInspector";
import { Search } from "lucide-react";

interface BaseCardProps {
  icon?: ReactNode;
  title?: string;
  children: ReactNode;
  className?: string;
  itemId?: string;
  selected?: boolean;
}

export function BaseCard({ icon, title, children, className = "", itemId, selected }: BaseCardProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  
  const { workspaces, activeWorkspaceId, moveItemToWorkspace } = useWorkspaceStore();

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  return (
    <div className={`bg-white rounded-xl border ${selected ? 'border-gray-800 shadow-xl ring-1 ring-gray-900/10' : 'border-black/5 shadow-sm hover:shadow-md hover:border-black/10'} overflow-hidden flex flex-col w-[300px] transition-all duration-200 ${className}`}>
      {/* Invisible Handles for ReactFlow */}
      <Handle type="target" position={Position.Top} className="opacity-0" />
      
      {/* Header (Drag Handle) */}
      {(icon || title || itemId) && (
        <div className="custom-drag-handle flex items-center gap-2 px-3 py-2.5 border-b border-black/[0.03] bg-white relative cursor-grab active:cursor-grabbing">
          {icon && <div className="text-gray-400">{icon}</div>}
          {title && <h3 className="text-[12px] font-medium text-gray-700 tracking-tight truncate flex-1">{title}</h3>}
          {!title && <div className="flex-1" />}
          
          {itemId && (
            <div ref={menuRef} className="nodrag">
              <button 
                onClick={() => setMenuOpen(!menuOpen)}
                className="p-1 rounded-md hover:bg-black/5 text-gray-400 hover:text-gray-900 transition-colors"
                title="Move Item"
              >
                <MoreHorizontal size={14} />
              </button>
              
              {menuOpen && (
                <div className="absolute right-2 top-8 w-48 bg-white rounded-lg shadow-lg border border-gray-200 py-1 z-50">
                  <div className="px-3 py-1 text-[10px] font-semibold text-gray-400 uppercase tracking-wider border-b border-gray-100 pb-2 mb-1">
                    Actions
                  </div>
                  
                  <button
                    className="w-full text-left px-3 py-2 text-sm text-gray-700 hover:bg-gray-50 flex items-center gap-2 border-b border-gray-100"
                    onClick={() => {
                      setInspectorOpen(true);
                      setMenuOpen(false);
                    }}
                  >
                    <Search size={14} className="text-gray-400" />
                    Inspect Context
                  </button>

                  <div className="px-3 py-2 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
                    Move to
                  </div>
                  
                  {activeWorkspaceId !== "inbox" && (
                    <button
                      className="w-full text-left px-3 py-2 text-sm text-gray-700 hover:bg-gray-50 flex items-center gap-2"
                      onClick={() => {
                        moveItemToWorkspace(itemId, "inbox");
                        setMenuOpen(false);
                      }}
                    >
                      <Inbox size={14} className="text-gray-400" />
                      Inbox
                    </button>
                  )}
                  
                  {workspaces
                    .filter(ws => ws.id !== activeWorkspaceId)
                    .map(ws => (
                      <button
                        key={ws.id}
                        className="w-full text-left px-3 py-2 text-sm text-gray-700 hover:bg-gray-50 flex items-center gap-2"
                        onClick={() => {
                          moveItemToWorkspace(itemId, ws.id);
                          setMenuOpen(false);
                        }}
                      >
                        <BoxSelect size={14} className="text-gray-400" />
                        <span className="truncate">{ws.name}</span>
                      </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}
      
      {/* Content */}
      <div className="p-3 nodrag">
        {children}
      </div>

      <Handle type="source" position={Position.Bottom} className="opacity-0" />
      
      {inspectorOpen && itemId && (
        <ContextInspector 
          itemId={itemId} 
          onClose={() => setInspectorOpen(false)} 
        />
      )}
    </div>
  );
}
