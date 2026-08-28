import { useState, useEffect, useCallback } from "react";
import { BoxSelect, LayoutTemplate, Inbox } from "lucide-react";
import { ReactFlow, Background, BackgroundVariant, Node, applyNodeChanges, NodeChange } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useWorkspaceStore } from "../../store/workspaceStore";
import { ItemNode } from "./nodes/ItemNode";

const nodeTypes = {
  itemNode: ItemNode,
};

export function Canvas() {
  const { workspaces, activeWorkspaceId, activeItems, updateItemPosition } = useWorkspaceStore();
  const activeWorkspace = workspaces.find((ws) => ws.id === activeWorkspaceId);
  const isInbox = activeWorkspaceId === "inbox";

  const [nodes, setNodes] = useState<Node[]>([]);

  // Sync local ReactFlow state with our global Zustand store when activeItems changes
  useEffect(() => {
    setNodes(
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

        return {
          id: item.id,
          position: { x, y },
          data: { item },
          type: 'itemNode',
          dragHandle: '.custom-drag-handle',
        };
      })
    );
  }, [activeItems]);

  const onNodesChange = useCallback(
    (changes: NodeChange[]) => setNodes((nds) => applyNodeChanges(changes, nds)),
    []
  );

  const onNodeDragStop = useCallback(
    (_event: any, node: Node) => {
      updateItemPosition(node.id, node.position.x, node.position.y);
    },
    [updateItemPosition]
  );

  if (!activeWorkspace && !isInbox) {
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

  const title = isInbox ? "Inbox" : activeWorkspace?.name;
  const description = isInbox
    ? "Your Inbox is empty. Captured items will appear here."
    : "This workspace is empty. Drop images, code snippets, colors, or links anywhere on the canvas to add them to this context.";

  return (
    <div className="flex-1 w-full h-full relative bg-[#FDFDFD] overflow-hidden flex flex-col">
      <ReactFlow
        nodes={nodes}
        onNodesChange={onNodesChange}
        onNodeDragStop={onNodeDragStop}
        nodeTypes={nodeTypes}
        edges={[]}
        minZoom={0.1}
        maxZoom={4}
        defaultViewport={{ x: 0, y: 0, zoom: 1 }}
        proOptions={{ hideAttribution: true }}
        fitView
      >
        <Background 
          color="#9CA3AF" 
          variant={BackgroundVariant.Dots} 
          gap={24} 
          size={2} 
        />
        
        {/* Empty State Overlay */}
        {activeItems.length === 0 && (
          <div className="absolute inset-0 pointer-events-none flex flex-col items-center justify-center p-8 z-10">
            <div className="w-16 h-16 rounded-2xl bg-gray-50 border border-gray-100 flex items-center justify-center mb-6 shadow-sm">
              {isInbox ? (
                <Inbox className="text-gray-300" size={32} strokeWidth={1.5} />
              ) : (
                <BoxSelect className="text-gray-300" size={32} strokeWidth={1.5} />
              )}
            </div>
            
            <h2 className="text-xl font-medium text-gray-900 mb-2 tracking-tight">
              {title}
            </h2>
            
            <p className="text-[14px] text-gray-500 max-w-sm text-center leading-relaxed">
              {description}
            </p>
          </div>
        )}
      </ReactFlow>
    </div>
  );
}
