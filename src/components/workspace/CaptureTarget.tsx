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
      let timer: ReturnType<typeof setTimeout>;
      const unlisten = listen('target-state-changed', (event) => {
        const state = event.payload as string;
        setNotchState(state);
        if (state === 'captured' || state === 'cancelled') {
          clearTimeout(timer);
          timer = setTimeout(() => {
            setNotchState('idle');
          }, 1800);
        }
      });
      const unlistenLegacy = listen('notch-state-changed', (event) => {
        const state = event.payload as string;
        setNotchState(state);
        if (state === 'captured' || state === 'cancelled') {
          clearTimeout(timer);
          timer = setTimeout(() => {
            setNotchState('idle');
          }, 1800);
        }
      });
      const unlistenPause = listen('capture-pause-changed', (event) => {
        if (event.payload === true) {
          setNotchState('paused');
        } else if (notchState === 'paused') {
          setNotchState('idle');
        }
      });
      return () => {
        clearTimeout(timer);
        unlisten.then(f => f());
        unlistenLegacy.then(f => f());
        unlistenPause.then(f => f());
      };
    }
  }, [isNotch, notchState]);
  
  // If in main window and captureState is hidden, do not render
  if (!isNotch && captureState === 'hidden') return null;

  if (isNotch) {
    const isIdle = notchState === 'idle';

    let outlineStateClass = "opacity-0";
    if (notchState === 'drag_detected' || notchState === 'hover' || notchState === 'hovering') {
      outlineStateClass = "opacity-100 notch-aura-hover";
    } else if (notchState === 'active' || notchState === 'activated') {
      outlineStateClass = "opacity-100 notch-aura-active scale-[1.01]";
    } else if (notchState === 'captured' || notchState === 'success') {
      outlineStateClass = "opacity-100 notch-aura-success";
    } else if (notchState === 'cancelled' || notchState === 'failure') {
      outlineStateClass = "opacity-100 notch-aura-error";
    } else if (notchState === 'paused' || notchState === 'pause') {
      outlineStateClass = "opacity-90 border-b-2 border-x-2 border-amber-400/90 shadow-[0_2px_14px_rgba(251,191,36,0.7)]";
    }

    return (
      <div 
        ref={targetRef} 
        className="w-full flex items-start justify-center pointer-events-none select-none pt-0 -mt-0.5"
      >
        {/* Exact MacBook Notch Silhouette Perimeter Aura (flush to top bezel, behind notch) */}
        <div 
          className={`notch-ambient-aura ${outlineStateClass} relative flex items-end justify-center pb-0.5`}
        >
          {/* Subtle tiny centered status dot indicator at bottom edge */}
          {!isIdle && (
            <div className="absolute bottom-0.5 left-1/2 -translate-x-1/2 flex items-center justify-center pointer-events-none">
              {notchState === 'active' || notchState === 'activated' ? (
                <div className="w-1.5 h-1.5 rounded-full bg-indigo-300 shadow-[0_0_8px_rgba(165,180,252,1)] animate-pulse" />
              ) : notchState === 'captured' || notchState === 'success' ? (
                <div className="w-1.5 h-1.5 rounded-full bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,1)]" />
              ) : null}
            </div>
          )}
        </div>
      </div>
    );
  }

  // In-App (main window) capture target presentation
  const baseClasses = "fixed top-2 left-1/2 -translate-x-1/2 z-50 rounded-2xl flex items-center justify-center transition-all duration-200 ease-out border backdrop-blur-md overflow-hidden pointer-events-none w-52 h-14 origin-top";
  
  let stateClasses = "";
  let content: React.ReactNode = <span className="font-medium text-sm text-gray-800">Drop to ContextSplit</span>;

  if (captureState === 'success') {
    stateClasses = "bg-green-500/90 border-green-600 shadow-lg scale-100 opacity-100";
    content = (
      <div className="flex items-center gap-2 text-white">
        <Check size={18} strokeWidth={3} />
        <span className="font-semibold text-sm">Saved</span>
      </div>
    );
  } else if (captureState === 'failure') {
    stateClasses = "bg-red-500/90 border-red-600 shadow-lg scale-100 opacity-100 animate-pulse";
    content = (
      <div className="flex items-center gap-2 text-white">
        <X size={18} strokeWidth={3} />
        <span className="font-semibold text-sm">Failed</span>
      </div>
    );
  } else {
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
