import React, { useRef, useEffect, useState } from 'react';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { Check, X, ArrowDownToLine, Sparkles } from 'lucide-react';
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
      const unlisten = listen('target-state-changed', (event) => {
        setNotchState(event.payload as string);
      });
      const unlistenLegacy = listen('notch-state-changed', (event) => {
        setNotchState(event.payload as string);
      });
      return () => {
        unlisten.then(f => f());
        unlistenLegacy.then(f => f());
      };
    }
  }, [isNotch]);
  
  if (!isNotch && captureState === 'hidden') return null;
  if (isNotch && notchState === 'idle') return null;

  const baseClasses = "fixed top-2 left-1/2 -translate-x-1/2 z-50 rounded-2xl flex items-center justify-center transition-all duration-200 ease-out border backdrop-blur-md overflow-hidden pointer-events-none w-52 h-14 origin-top";
  
  let stateClasses = "";
  let content: React.ReactNode = <span className="font-medium text-sm text-gray-800">Drop to ContextSplit</span>;

  if (isNotch) {
    switch (notchState) {
      case 'drag_detected':
        stateClasses = "bg-white/70 dark:bg-zinc-900/70 border-black/10 shadow-sm scale-85 opacity-75";
        content = (
          <div className="flex items-center gap-1.5 text-gray-600 dark:text-gray-300">
            <Sparkles size={14} className="text-blue-500 animate-pulse" />
            <span className="font-medium text-xs">ContextSplit</span>
          </div>
        );
        break;
      case 'hover':
      case 'hovering':
        stateClasses = "bg-white/95 dark:bg-zinc-900/95 border-black/10 dark:border-white/15 shadow-lg scale-100 opacity-100 ring-2 ring-black/5";
        content = (
          <div className="flex items-center gap-2 text-gray-700 dark:text-zinc-200">
            <ArrowDownToLine size={16} className="text-blue-500" />
            <span className="font-semibold text-xs tracking-wide">ContextSplit Capture</span>
          </div>
        );
        break;
      case 'active':
      case 'activated':
        stateClasses = "bg-blue-600/95 text-white border-blue-400 shadow-2xl scale-105 opacity-100 ring-4 ring-blue-400/40 animate-pulse";
        content = (
          <div className="flex items-center gap-2 text-white">
            <ArrowDownToLine size={18} strokeWidth={2.5} />
            <span className="font-semibold text-sm tracking-wide">Release to capture</span>
          </div>
        );
        break;
      case 'captured':
      case 'success':
        stateClasses = "bg-emerald-600/95 text-white border-emerald-400 shadow-xl scale-100 opacity-100";
        content = (
          <div className="flex items-center gap-2 text-white">
            <Check size={18} strokeWidth={3} />
            <span className="font-semibold text-sm">Captured</span>
          </div>
        );
        break;
      case 'cancelled':
      case 'failure':
        stateClasses = "bg-red-500/90 text-white border-red-400 shadow-lg scale-95 opacity-90";
        content = (
          <div className="flex items-center gap-2 text-white">
            <X size={16} strokeWidth={2.5} />
            <span className="font-medium text-xs">Cancelled</span>
          </div>
        );
        break;
      default:
        stateClasses = "bg-white/90 border-black/10 scale-90 opacity-90";
        content = <span className="font-medium text-xs text-gray-600">ContextSplit</span>;
        break;
    }
  } else {
    // Proximity states handle the visual representation when capturing is 'visible'
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
