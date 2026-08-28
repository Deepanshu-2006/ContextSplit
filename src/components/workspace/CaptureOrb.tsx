import React, { useRef } from 'react';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { Check, X } from 'lucide-react';
import { useCursorProximity } from '../../hooks/useCursorProximity';

export const CaptureOrb: React.FC = () => {
  const { captureState } = useWorkspaceStore();
  const targetRef = useRef<HTMLDivElement>(null);
  
  // Custom thresholds for the orb so it expands more eagerly
  const proximity = useCursorProximity(targetRef, {
    nearDistance: 250,
    veryNearDistance: 120,
    activeDistance: 60,
  });

  if (captureState === 'hidden') return null;

  // The Orb relies on a fixed container (e.g. w-48 h-16) to avoid layout jitter,
  // scaling a perfectly round or pill-shaped inner element.
  const baseClasses = "fixed top-0 left-1/2 -translate-x-1/2 w-48 h-16 z-50 flex items-center justify-center pointer-events-none";
  
  let innerClasses = "flex items-center justify-center transition-all duration-300 ease-out shadow-lg overflow-hidden backdrop-blur-md ";
  let content = <div className="w-4 h-4 rounded-full bg-gray-500 animate-pulse"></div>;

  if (captureState === 'success') {
    innerClasses += "w-40 h-12 rounded-full bg-green-500/90 border border-green-600 scale-100 opacity-100";
    content = (
      <div className="flex items-center gap-2 text-white">
        <Check size={18} strokeWidth={3} />
        <span className="font-semibold text-sm">Saved</span>
      </div>
    );
  } else if (captureState === 'failure') {
    innerClasses += "w-40 h-12 rounded-full bg-red-500/90 border border-red-600 scale-100 opacity-100 animate-pulse";
    content = (
      <div className="flex items-center gap-2 text-white">
        <X size={18} strokeWidth={3} />
        <span className="font-semibold text-sm">Failed</span>
      </div>
    );
  } else {
    // Proximity states for the Orb
    switch (proximity) {
      case 'far':
        // Small floating dot
        innerClasses += "w-10 h-10 rounded-full bg-white/70 border border-black/10 scale-75 opacity-80";
        content = <div className="w-3 h-3 rounded-full bg-gray-400"></div>;
        break;
      case 'near':
        // Larger circle
        innerClasses += "w-12 h-12 rounded-full bg-white/90 border border-black/10 scale-100 opacity-95";
        content = <div className="w-4 h-4 rounded-full bg-gray-600"></div>;
        break;
      case 'very_near':
        // Expands into a pill
        innerClasses += "w-40 h-12 rounded-full bg-blue-50/90 border border-blue-200 scale-100 opacity-100 ring-2 ring-blue-500/10";
        content = <span className="font-medium text-sm text-blue-600">Drop to capture</span>;
        break;
      case 'active':
        // Full pill target
        innerClasses += "w-48 h-14 rounded-full bg-blue-100/95 border border-blue-300 scale-105 opacity-100 ring-4 ring-blue-500/30";
        content = <span className="font-semibold text-sm text-blue-700">Drop to ContextSplit</span>;
        break;
    }
  }

  return (
    <div ref={targetRef} className={baseClasses}>
      <div className={innerClasses}>
        {content}
      </div>
    </div>
  );
};
