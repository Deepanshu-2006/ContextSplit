import { useState, useEffect, useCallback } from "react";
import { BoxSelect, LayoutTemplate, Clock } from "lucide-react";
import { ReactFlow, Background, BackgroundVariant, Node, applyNodeChanges, NodeChange } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useWorkspaceStore } from "../../store/workspaceStore";
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
    openTimeline,
  } = useWorkspaceStore();
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

  // If viewing Inbox, render the clean vertical InboxView
  if (isInbox) {
    return (
      <InboxView
        items={activeItems}
        workspaces={workspaces}
        onMoveToWorkspace={(itemId, wsId) => moveItemsToWorkspace([itemId], wsId)}
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

  const title = activeWorkspace?.name || "Workspace";
  const description =
    "This workspace is empty. Drop images, code snippets, colors, or links anywhere on the canvas to add them to this context.";

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
        minZoom={0.2}
        maxZoom={1.5}
        defaultViewport={{ x: 0, y: 0, zoom: 1 }}
        proOptions={{ hideAttribution: true }}
        panOnScroll={true}
        selectionOnDrag={true}
        panOnDrag={false}
        zoomOnPinch={true}
        zoomOnScroll={false}
        zoomActivationKeyCode="Meta"
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
            <BoxSelect size={20} strokeWidth={2} />
            <h1 className="text-2xl font-semibold tracking-tight text-gray-900">
              {title}
            </h1>
          </div>

          {activeWorkspace && (
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

        {/* Empty State Overlay */}
        {activeItems.length === 0 && (
          <div className="absolute inset-0 pointer-events-none flex flex-col items-center justify-center p-8 z-10">
            <div className="flex flex-col items-center text-center opacity-40">
              <div className="w-12 h-12 rounded-xl bg-gray-50 flex items-center justify-center mb-4">
                <BoxSelect className="text-gray-400" size={24} strokeWidth={1.5} />
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
