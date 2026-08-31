import { useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { WorkspaceScore } from "../../../services/db";
import { useWorkspaceStore } from "../../../store/workspaceStore";

interface ContextSuggestionProps {
  itemId: string;
  score: WorkspaceScore;
  onAccept: (workspaceId: string) => void;
  onDismiss: () => void;
}

export function ContextSuggestion({ score, onAccept, onDismiss }: ContextSuggestionProps) {
  const { workspaces } = useWorkspaceStore();
  const [showReasons, setShowReasons] = useState(false);

  const suggestedWorkspace = workspaces.find((w) => w.id === score.workspace_id);

  if (!suggestedWorkspace) {
    return null;
  }

  const confidence = Math.round(score.score * 100);

  return (
    <div className="bg-gradient-to-br from-blue-50 to-indigo-50/30 border-t border-blue-100 p-3 text-xs flex flex-col nodrag">
      <div className="flex flex-col gap-0.5">
        <span className="text-blue-900 font-medium">Looks related to {suggestedWorkspace.name}</span>
        <span className="text-blue-600/80 font-medium text-[10px] uppercase tracking-wider">{confidence}% confidence</span>
      </div>
      
      <div className="mt-2 mb-1">
        <button 
          onClick={() => setShowReasons(!showReasons)} 
          className="text-blue-500 hover:text-blue-700 font-medium flex items-center gap-1 text-[11px]"
        >
          Why? {showReasons ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
        </button>
      </div>
      
      {showReasons && score.reasons.length > 0 && (
        <ul className="text-[11px] text-blue-800/80 mb-3 ml-1 list-disc pl-3 flex flex-col gap-1">
          {score.reasons.map((reason, idx) => (
            <li key={idx}>{reason}</li>
          ))}
        </ul>
      )}
      
      <div className="flex items-center gap-2 mt-2 pt-2 border-t border-blue-200/50">
        <button
          onClick={() => onAccept(score.workspace_id)}
          className="flex-1 bg-blue-600 hover:bg-blue-700 text-white font-medium py-1.5 rounded-md transition-colors text-center text-[11px] shadow-sm"
        >
          Move to {suggestedWorkspace.name}
        </button>
        <button
          onClick={onDismiss}
          className="flex-1 bg-white hover:bg-gray-50 text-gray-700 border border-gray-200 font-medium py-1.5 rounded-md transition-colors text-center text-[11px] shadow-sm"
        >
          Dismiss
        </button>
      </div>
    </div>
  );
}
