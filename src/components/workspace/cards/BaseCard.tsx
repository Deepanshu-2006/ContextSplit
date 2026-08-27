import { ReactNode } from "react";
import { Handle, Position } from "@xyflow/react";

interface BaseCardProps {
  icon?: ReactNode;
  title?: string;
  children: ReactNode;
  className?: string;
}

export function BaseCard({ icon, title, children, className = "" }: BaseCardProps) {
  return (
    <div className={`bg-white rounded-xl border border-gray-200 shadow-sm overflow-hidden flex flex-col w-[300px] ${className}`}>
      {/* Invisible Handles for ReactFlow */}
      <Handle type="target" position={Position.Top} className="opacity-0" />
      
      {/* Header */}
      {(icon || title) && (
        <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-100 bg-gray-50/50">
          {icon && <div className="text-gray-400">{icon}</div>}
          {title && <h3 className="text-xs font-medium text-gray-600 truncate">{title}</h3>}
        </div>
      )}
      
      {/* Content */}
      <div className="p-3">
        {children}
      </div>

      <Handle type="source" position={Position.Bottom} className="opacity-0" />
    </div>
  );
}
