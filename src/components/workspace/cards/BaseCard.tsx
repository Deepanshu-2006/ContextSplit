import { ReactNode, useState, useRef, useEffect } from "react";
import { createPortal } from "react-dom";
import { Handle, Position } from "@xyflow/react";
import { MoreHorizontal, Inbox, BoxSelect, Search } from "lucide-react";
import { useWorkspaceStore } from "../../../store/workspaceStore";
import { ContextInspector } from "../ContextInspector";
import { db, WorkspaceScore } from "../../../services/db";
import { ContextSuggestion } from "./ContextSuggestion";

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
  const menuBtnRef = useRef<HTMLButtonElement>(null);
  const [menuPos, setMenuPos] = useState<{ top: number; right: number } | null>(null);
  
  const { workspaces, activeWorkspaceId, moveItemToWorkspace } = useWorkspaceStore();
  const [suggestionScore, setSuggestionScore] = useState<WorkspaceScore | null>(null);
  const [suggestionDismissed, setSuggestionDismissed] = useState(false);

  useEffect(() => {
    async function fetchSuggestion() {
      if (activeWorkspaceId === "inbox" && itemId) {
        try {
          // Check if it was dismissed
          const signals = await db.listContextSignals(null, itemId);
          const isDismissed = signals.some(s => 
            s.signal_type === "user_action" && 
            s.payload.includes("suggestion_dismissed")
          );
          
          if (isDismissed) {
            setSuggestionDismissed(true);
            return;
          }
          
          const scores = await db.scoreItem(itemId);
          if (scores.length > 0 && scores[0].confidence_band !== "Low") {
            setSuggestionScore(scores[0]);
          }
        } catch (e) {
          console.error("Failed to fetch suggestions", e);
        }
      }
    }
    fetchSuggestion();
  }, [activeWorkspaceId, itemId]);

  const handleAcceptSuggestion = async (suggestedWorkspaceId: string) => {
    if (!itemId) return;
    setSuggestionDismissed(true); // hide immediately
    try {
      await db.createContextSignal(suggestedWorkspaceId, itemId, "user_action", null, JSON.stringify({ action: "suggestion_accepted" }));
      moveItemToWorkspace(itemId, suggestedWorkspaceId);
    } catch (e) {
      console.error(e);
    }
  };

  const handleDismissSuggestion = async () => {
    if (!itemId || !suggestionScore) return;
    setSuggestionDismissed(true);
    try {
      await db.createContextSignal(null, itemId, "user_action", null, JSON.stringify({ action: "suggestion_dismissed", suggested_workspace: suggestionScore.workspace_id }));
    } catch (e) {
      console.error(e);
    }
  };

  const handleManualMove = async (wsId: string) => {
    if (!itemId) return;
    
    // If a suggestion was visible and they chose another workspace, log it
    if (suggestionScore && !suggestionDismissed && suggestionScore.workspace_id !== wsId) {
       try {
         await db.createContextSignal(wsId, itemId, "user_action", null, JSON.stringify({ action: "suggestion_changed", suggested_workspace: suggestionScore.workspace_id }));
       } catch(e) {}
    }
    
    moveItemToWorkspace(itemId, wsId);
    setMenuOpen(false);
  };

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (
        menuRef.current && !menuRef.current.contains(event.target as Node) &&
        menuBtnRef.current && !menuBtnRef.current.contains(event.target as Node)
      ) {
        setMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  function openMenu() {
    if (menuBtnRef.current) {
      const rect = menuBtnRef.current.getBoundingClientRect();
      setMenuPos({ top: rect.bottom + 6, right: window.innerWidth - rect.right });
    }
    setMenuOpen(v => !v);
  }

  return (
    <div className={`bg-white rounded-xl border ${selected ? 'border-gray-800 shadow-xl ring-1 ring-gray-900/10' : 'border-black/5 shadow-sm hover:shadow-md hover:border-black/10'} overflow-hidden flex flex-col w-75 transition-all duration-200 ${className}`}>
      {/* Invisible Handles for ReactFlow */}
      <Handle type="target" position={Position.Top} className="opacity-0" />
      
      {/* Header (Drag Handle) */}
      {(icon || title || itemId) && (
        <div className="custom-drag-handle flex items-center gap-2 px-3 py-2.5 border-b border-black/3 bg-white relative active:cursor-grabbing">
          {icon && <div className="text-gray-400">{icon}</div>}
          {title && <h3 className="text-[12px] font-medium text-gray-700 tracking-tight truncate flex-1">{title}</h3>}
          {!title && <div className="flex-1" />}
          
          {itemId && (
            <div className="nodrag">
              <button
                ref={menuBtnRef}
                onClick={openMenu}
                className="p-1 rounded-md hover:bg-black/5 text-gray-400 hover:text-gray-900 transition-colors"
                title="Actions"
              >
                <MoreHorizontal size={14} />
              </button>

              {menuOpen && menuPos && createPortal(
                <div
                  ref={menuRef}
                  style={{ top: menuPos.top, right: menuPos.right }}
                  className="fixed w-52 bg-white rounded-xl shadow-xl border border-gray-200/80 py-1.5 z-[9999] overflow-hidden"
                >
                  {/* Inspect */}
                  <div className="px-3 pt-1 pb-0.5 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
                    Actions
                  </div>
                  <button
                    className="w-full text-left px-3 py-2 text-[13px] text-gray-700 hover:bg-gray-50 flex items-center gap-2.5 transition-colors"
                    onClick={() => { setInspectorOpen(true); setMenuOpen(false); }}
                  >
                    <Search size={14} className="text-indigo-500" />
                    <span>Inspect Context</span>
                  </button>

                  {/* Move to */}
                  <div className="my-1 border-t border-gray-100" />
                  <div className="px-3 pt-1 pb-0.5 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
                    Move to
                  </div>

                  {activeWorkspaceId !== "inbox" && (
                    <button
                      className="w-full text-left px-3 py-2 text-[13px] text-gray-700 hover:bg-gray-50 flex items-center gap-2.5 transition-colors"
                      onClick={() => handleManualMove("inbox")}
                    >
                      <Inbox size={14} className="text-gray-400" />
                      <span>Inbox</span>
                    </button>
                  )}

                  {workspaces
                    .filter(ws => ws.id !== activeWorkspaceId)
                    .map(ws => (
                      <button
                        key={ws.id}
                        className="w-full text-left px-3 py-2 text-[13px] text-gray-700 hover:bg-gray-50 flex items-center gap-2.5 transition-colors"
                        onClick={() => handleManualMove(ws.id)}
                      >
                        <BoxSelect size={14} className="text-gray-400" />
                        <span className="truncate">{ws.name}</span>
                      </button>
                    ))}
                </div>,
                document.body
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
      
      {/* Context Suggestion */}
      {suggestionScore && !suggestionDismissed && itemId && (
        <ContextSuggestion 
          itemId={itemId}
          score={suggestionScore}
          onAccept={handleAcceptSuggestion}
          onDismiss={handleDismissSuggestion}
        />
      )}
      
      {inspectorOpen && itemId && (
        <ContextInspector 
          itemId={itemId} 
          onClose={() => setInspectorOpen(false)} 
        />
      )}
    </div>
  );
}
