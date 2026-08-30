import { useState } from "react";
import { AlignLeft, Maximize2, X } from "lucide-react";
import { createPortal } from "react-dom";
import { BaseCard } from "./BaseCard";

interface TextCardProps {
  content: string;
  title?: string;
  itemId?: string;
  selected?: boolean;
}

export function TextCard({ content, title, itemId, selected }: TextCardProps) {
  const [expanded, setExpanded] = useState(false);
  const isLong = content.length > 200;
  const displayContent = expanded ? content : (isLong ? content.substring(0, 200) + "..." : content);

  return (
    <>
      <BaseCard icon={<AlignLeft size={14} />} title={title || "Text Snippet"} itemId={itemId} selected={selected}>
        <div className="text-sm text-gray-800 whitespace-pre-wrap wrap-break-word">
          {displayContent}
        </div>
        {!expanded && isLong && (
          <button 
            onClick={() => setExpanded(true)}
            className="mt-2 text-xs text-blue-500 hover:text-blue-600 font-medium flex items-center gap-1"
          >
            <Maximize2 size={12} /> Show more
          </button>
        )}
      </BaseCard>

      {/* Expanded Modal */}
      {expanded && createPortal(
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 backdrop-blur-sm p-4">
          <div className="bg-white rounded-2xl shadow-xl w-full max-w-2xl max-h-[80vh] flex flex-col">
            <div className="flex items-center justify-between px-4 py-3 border-b border-gray-100">
              <div className="flex items-center gap-2 text-gray-600">
                <AlignLeft size={16} />
                <span className="font-medium">{title || "Text Snippet"}</span>
              </div>
              <button onClick={() => setExpanded(false)} className="p-1 hover:bg-gray-100 rounded-lg text-gray-400">
                <X size={18} />
              </button>
            </div>
            <div className="p-6 overflow-y-auto text-gray-800 whitespace-pre-wrap font-mono text-sm leading-relaxed">
              {content}
            </div>
          </div>
        </div>,
        document.body
      )}
    </>
  );
}
