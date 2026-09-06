import { useState, useEffect, useCallback } from "react";
import { BoxSelect, LayoutTemplate, Inbox, Clock, Trash2, LayoutList } from "lucide-react";
import { ReactFlow, Background, BackgroundVariant, Node, applyNodeChanges, NodeChange } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { confirm } from "@tauri-apps/plugin-dialog";
import { useWorkspaceStore } from "../../store/workspaceStore";
import { useInboxStore } from "../../store/inboxStore";
import { ItemNode } from "./nodes/ItemNode";
import { InboxView } from "../inbox/InboxView";

const nodeTypes = {
  itemNode: ItemNode,
};

export function Canvas() {
  const { 
    workspaces, activeWorkspaceId, activeItems, 
    updateItemPositions, moveItemsToWorkspace,
    setIsDraggingCard, setHoveredWorkspaceId,
    openTimeline, emptyInbox,
  } = useWorkspaceStore();
  const { viewMode, setViewMode } = useInboxStore();
  const activeWorkspace = workspaces.find((ws) => ws.id === activeWorkspaceId);
  const isInbox = activeWorkspaceId === "inbox";

  const [nodes, setNodes] = useState<Node[]>([]);

  // Sync local ReactFlow state with our global Zustand store when activeItems changes
  useEffect(() => {
    setNodes(prevNodes => 
      activeItems.map((item, index) => {
        let x, y;
        try {
          const meta = JSON.parse(item.metadata || '{}');
          if (typeof meta.pos_x === 'number' && typeof meta.pos_y === 'number') {
            x = meta.pos_x;
            y = meta.pos_y;
          }
        } catch (e) {}

        if (x === undefined || y === undefined) {
          // Fallback basic grid layout for items without saved positions
          const col = index % 3;
          const row = Math.floor(index / 3);
          x = 100 + col * 350;
          y = 100 + row * 200;
        }

        const existingNode = prevNodes.find(n => n.id === item.id);

        return {
          id: item.id,
          position: { x, y },
          data: { item },
          type: 'itemNode',
          dragHandle: '.custom-drag-handle',
          selected: existingNode ? existingNode.selected : false,
        };
      })
    );
  }, [activeItems]);

  const onNodesChange = useCallback(
    (changes: NodeChange[]) => setNodes((nds) => applyNodeChanges(changes, nds)),
    []
  );

  const onNodeDragStart = useCallback(() => {
    setIsDraggingCard(true);
  }, [setIsDraggingCard]);

  const onNodeDrag = useCallback(
    (_event: any) => {
      let clientX = 0;
      let clientY = 0;
      
      if ('clientX' in _event) {
        clientX = _event.clientX;
        clientY = _event.clientY;
      } else if ('changedTouches' in _event && _event.changedTouches.length > 0) {
        clientX = _event.changedTouches[0].clientX;
        clientY = _event.changedTouches[0].clientY;
      }

      const dropElement = document.elementFromPoint(clientX, clientY);
      const workspaceTarget = dropElement?.closest('[data-workspace-target-id]');
      
      if (workspaceTarget) {
        const targetWorkspaceId = workspaceTarget.getAttribute('data-workspace-target-id');
        if (targetWorkspaceId && targetWorkspaceId !== activeWorkspaceId) {
          setHoveredWorkspaceId(targetWorkspaceId);
          return;
        }
      }
      setHoveredWorkspaceId(null);
    },
    [activeWorkspaceId, setHoveredWorkspaceId]
  );

  const onNodeDragStop = useCallback(
    (_event: any, node: Node, draggedNodes: Node[]) => {
      setIsDraggingCard(false);
      setHoveredWorkspaceId(null);

      // We might have single-dragged or multi-dragged. If draggedNodes is missing/empty, use [node]
      const nodesToProcess = draggedNodes && draggedNodes.length > 0 ? draggedNodes : [node];

      // Find element under cursor to detect if dropped on a Sidebar item
      let clientX = 0;
      let clientY = 0;
      
      if ('clientX' in _event) {
        clientX = _event.clientX;
        clientY = _event.clientY;
      } else if ('changedTouches' in _event && _event.changedTouches.length > 0) {
        clientX = _event.changedTouches[0].clientX;
        clientY = _event.changedTouches[0].clientY;
      }

      const dropElement = document.elementFromPoint(clientX, clientY);
      const workspaceTarget = dropElement?.closest('[data-workspace-target-id]');
      
      if (workspaceTarget) {
        const targetWorkspaceId = workspaceTarget.getAttribute('data-workspace-target-id');
        if (targetWorkspaceId && targetWorkspaceId !== activeWorkspaceId) {
          const itemIds = nodesToProcess.map(n => n.id);
          moveItemsToWorkspace(itemIds, targetWorkspaceId);
          return;
        }
      }

      // If no valid drop target, just update positions
      const updates = nodesToProcess.map(n => ({
        id: n.id,
        x: n.position.x,
        y: n.position.y
      }));
      updateItemPositions(updates);
    },
    [updateItemPositions, activeWorkspaceId, moveItemsToWorkspace]
  );

  // If viewing Inbox and not in freeform Canvas mode, render the modern Sectioned/Timeline InboxView
  if (isInbox && viewMode !== 'canvas') {
    return (
      <InboxView
        items={activeItems}
        workspaces={workspaces}
        onMoveToWorkspace={(itemId, wsId) => moveItemsToWorkspace([itemId], wsId)}
        onSelectCanvasView={() => setViewMode('canvas')}
      />
    );
  }

  if (!activeWorkspace && !isInbox) {
    return (
      <div 
        className="flex-1 w-full h-full relative bg-[#FDFDFD] flex items-center justify-center"
      >
        <div className="flex flex-col items-center text-center p-8 max-w-sm opacity-60">
          <div className="w-16 h-16 rounded-2xl bg-gray-50 flex items-center justify-center mb-6">
            <LayoutTemplate className="text-gray-300" size={32} strokeWidth={1.5} />
          </div>
          <h2 className="text-lg font-medium text-gray-800 mb-2 tracking-tight">
            No Workspace Selected
          </h2>
          <p className="text-[13px] text-gray-500 leading-relaxed">
            Create a new workspace from the sidebar to start collecting your context.
          </p>
        </div>
      </div>
    );
  }

  const handleEmptyInbox = async () => {
    const isConfirmed = await confirm(
      "Are you sure you want to empty the inbox? All items in the inbox will be permanently deleted.",
      { title: "Empty Inbox", kind: "warning" }
    );
    if (isConfirmed) {
      await emptyInbox();
    }
  };

  const title = isInbox ? "Inbox (Canvas Mode)" : activeWorkspace?.name;
  const description = isInbox
    ? "Your Inbox is empty. Captured items will appear here."
    : "This workspace is empty. Drop images, code snippets, colors, or links anywhere on the canvas to add them to this context.";

  return (
    <div className="flex-1 w-full h-full relative bg-[#FDFDFD] overflow-hidden flex flex-col">
      <ReactFlow
        nodes={nodes}
        onNodesChange={onNodesChange}
        onNodeDragStart={onNodeDragStart}
        onNodeDrag={onNodeDrag}
        onNodeDragStop={onNodeDragStop}
        nodeTypes={nodeTypes}
        edges={[]}
        minZoom={0.1}
        maxZoom={4}
        defaultViewport={{ x: 0, y: 0, zoom: 1 }}
        proOptions={{ hideAttribution: true }}
        panOnScroll={true}
        selectionOnDrag={true}
        panOnDrag={false}
        zoomOnPinch={true}
        zoomOnScroll={false}
        zoomActivationKeyCode="Meta"
        fitView
      >
        <Background 
          color="#9CA3AF" 
          variant={BackgroundVariant.Dots} 
          gap={32} 
          size={1.5} 
          className="opacity-50"
        />
        
        {/* Floating Workspace Title & Actions */}
        <div className="absolute top-8 left-8 z-10 flex items-center gap-4">
          <div className="flex items-center gap-3 opacity-40 pointer-events-none">
            {isInbox ? (
              <Inbox size={20} strokeWidth={2} />
            ) : (
              <BoxSelect size={20} strokeWidth={2} />
            )}
            <h1 className="text-2xl font-semibold tracking-tight text-gray-900">
              {title}
            </h1>
          </div>

          {isInbox && (
            <button
              onClick={() => setViewMode('sections')}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-white/95 hover:bg-white border border-black/10 rounded-lg text-xs font-semibold text-gray-700 hover:text-indigo-600 shadow-sm backdrop-blur-xs transition-all cursor-pointer pointer-events-auto"
              title="Return to Sectioned Inbox"
            >
              <LayoutList size={13} className="text-indigo-600" />
              <span>Sectioned View</span>
            </button>
          )}

          {!isInbox && activeWorkspace && (
            <button
              onClick={() => openTimeline(activeWorkspace.id)}
              className="flex items-center gap-1.5 px-2.5 py-1 bg-white/80 hover:bg-white border border-black/5 hover:border-black/10 rounded-lg text-xs font-medium text-slate-600 hover:text-indigo-600 shadow-2xs backdrop-blur-xs transition-all cursor-pointer pointer-events-auto"
              title="View Context Timeline"
            >
              <Clock size={12} className="text-indigo-600" />
              <span>Timeline</span>
            </button>
          )}
        </div>

        {/* Floating Empty Inbox Trash Bin Button */}
        {isInbox && activeItems.length > 0 && (
          <div className="absolute bottom-6 right-6 z-20 pointer-events-auto">
            <button
              onClick={handleEmptyInbox}
              className="flex items-center gap-2 px-3.5 py-2.5 bg-white/95 hover:bg-rose-50 text-gray-600 hover:text-rose-600 border border-gray-200/80 hover:border-rose-200 shadow-md hover:shadow-lg rounded-xl backdrop-blur-md transition-all duration-200 cursor-pointer group"
              title="Empty Inbox"
            >
              <Trash2 size={16} className="text-gray-400 group-hover:text-rose-500 transition-colors" />
              <span className="text-xs font-semibold tracking-wide">Empty Inbox</span>
            </button>
          </div>
        )}

        {/* Empty State Overlay */}
        {activeItems.length === 0 && (
          <div className="absolute inset-0 pointer-events-none flex flex-col items-center justify-center p-8 z-10">
            <div className="flex flex-col items-center text-center opacity-40">
              <div className="w-12 h-12 rounded-xl bg-gray-50 flex items-center justify-center mb-4">
                {isInbox ? (
                  <Inbox className="text-gray-400" size={24} strokeWidth={1.5} />
                ) : (
                  <BoxSelect className="text-gray-400" size={24} strokeWidth={1.5} />
                )}
              </div>
              <p className="text-[13px] text-gray-500 max-w-60 leading-relaxed">
                {description}
              </p>
            </div>
          </div>
        )}
      </ReactFlow>
    </div>
  );
}
