import React, { useRef, useEffect, useState } from 'react';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { Check, X } from 'lucide-react';
import { useCursorProximity } from '../../hooks/useCursorProximity';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

export const CaptureTarget: React.FC = () => {
  const { captureState } = useWorkspaceStore();
  const targetRef = useRef<HTMLDivElement>(null);
  const proximity = useCursorProximity(targetRef);
  const [notchState, setNotchState] = useState<string>('idle');
  const isNotch = typeof window !== 'undefined' && getCurrentWindow().label === "macos-notch-target";
  
  useEffect(() => {
    if (isNotch) {
      const unlisten = listen('notch-state-changed', (event) => {
        setNotchState(event.payload as string);
      });
      return () => {
        unlisten.then(f => f());
      };
    }
  }, [isNotch]);
  
  if (!isNotch && captureState === 'hidden') return null;
  if (isNotch && notchState === 'idle') return null;

  const baseClasses = "fixed top-4 left-1/2 -translate-x-1/2 z-50 rounded-2xl flex items-center justify-center transition-all duration-200 ease-out border backdrop-blur-md overflow-hidden pointer-events-none w-48 h-14 origin-center";
  
  let stateClasses = "";
  let content = <span className="font-medium text-sm text-gray-800">Drop to ContextSplit</span>;

  const resolvedState = isNotch ? 
    (notchState === 'captured' ? 'success' : notchState === 'cancelled' ? 'failure' : null) 
    : captureState;

  if (resolvedState === 'success') {
    stateClasses = "bg-green-500/90 border-green-600 shadow-lg scale-100 opacity-100";
    content = (
      <div className="flex items-center gap-2 text-white">
        <Check size={18} strokeWidth={3} />
        <span className="font-semibold text-sm">Saved</span>
      </div>
    );
  } else if (resolvedState === 'failure') {
    stateClasses = "bg-red-500/90 border-red-600 shadow-lg scale-100 opacity-100 animate-pulse";
    content = (
      <div className="flex items-center gap-2 text-white">
        <X size={18} strokeWidth={3} />
        <span className="font-semibold text-sm">Failed</span>
      </div>
    );
  } else if (isNotch) {
    if (notchState === 'hovering') {
        stateClasses = "bg-white/90 border-black/10 shadow-md scale-90 opacity-95";
        content = <span className="font-medium text-sm text-gray-600">ContextSplit</span>;
    } else if (notchState === 'activated') {
        stateClasses = "bg-blue-100/95 border-blue-300 shadow-xl scale-110 opacity-100 ring-4 ring-blue-500/30";
        content = <span className="font-semibold text-sm text-blue-700">Release to capture</span>;
    }
  } else {
    // Proximity states handle the visual representation when capturing is 'visible'
    switch (proximity) {
      case 'far':
        stateClasses = "bg-white/70 border-black/5 shadow-sm scale-75 opacity-80";
        content = <span className="font-medium text-sm text-gray-500">ContextSplit</span>;
        break;
      case 'near':
        stateClasses = "bg-white/90 border-black/10 shadow-md scale-90 opacity-95";
        content = <span className="font-medium text-sm text-gray-600">ContextSplit</span>;
        break;
      case 'very_near':
        stateClasses = "bg-blue-50/90 border-blue-200 shadow-lg scale-100 opacity-100 ring-2 ring-blue-500/10";
        content = <span className="font-medium text-sm text-blue-600">Drop to capture</span>;
        break;
      case 'active':
        stateClasses = "bg-blue-100/95 border-blue-300 shadow-xl scale-110 opacity-100 ring-4 ring-blue-500/30";
        content = <span className="font-semibold text-sm text-blue-700">Drop to ContextSplit</span>;
        break;
    }
  }

  return (
    <div ref={targetRef} className={`${baseClasses} ${stateClasses}`}>
      <div className="flex items-center justify-center w-full h-full relative">
        <div className="absolute inset-0 bg-linear-to-b from-white/40 to-transparent pointer-events-none mix-blend-overlay"></div>
        <div className="relative z-10 flex items-center justify-center w-full text-center">
          {content}
        </div>
      </div>
    </div>
  );
};
