import { useState } from "react";
import { Check, X, Info } from "lucide-react";
import { WorkspaceScore } from "../../../services/db";
import { useWorkspaceStore } from "../../../store/workspaceStore";

interface ContextSuggestionProps {
  itemId: string;
  score: WorkspaceScore;
  onAccept: (workspaceId: string) => void;
  onDismiss: () => void;
}

export function ContextSuggestion({ itemId, score, onAccept, onDismiss }: ContextSuggestionProps) {
  const { workspaces } = useWorkspaceStore();
  const [showReasons, setShowReasons] = useState(false);

  const suggestedWorkspace = workspaces.find((w) => w.id === score.workspace_id);

  if (!suggestedWorkspace) {
    return null;
  }

  return (
    <div className="bg-blue-50/80 border-t border-blue-100 px-3 py-2 text-xs flex flex-col gap-2 nodrag">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-1.5 text-blue-900 font-medium">
          <span>✨</span>
          <span>Looks related to {suggestedWorkspace.name}</span>
          <button
            onClick={() => setShowReasons(!showReasons)}
            className="text-blue-500 hover:text-blue-700 p-0.5 rounded-full hover:bg-blue-100 transition-colors ml-1"
            title="Why this suggestion?"
          >
            <Info size={12} />
          </button>
        </div>
        
        <div className="flex items-center gap-1">
          <button
            onClick={() => onAccept(score.workspace_id)}
            className="p-1 text-blue-600 hover:bg-blue-100 rounded-md transition-colors"
            title="Accept Suggestion"
          >
            <Check size={14} />
          </button>
          <button
            onClick={onDismiss}
            className="p-1 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-md transition-colors"
            title="Dismiss"
          >
            <X size={14} />
          </button>
        </div>
      </div>
      
      {showReasons && score.reasons.length > 0 && (
        <div className="text-[10px] text-blue-800/80 bg-blue-100/50 rounded p-1.5 flex flex-col gap-1 mt-1">
          {score.reasons.map((reason, idx) => (
            <div key={idx} className="flex items-start gap-1.5">
              <span className="text-blue-400 mt-0.5">•</span>
              <span>{reason}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
