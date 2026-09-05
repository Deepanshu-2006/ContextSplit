import React, { useRef, useEffect, useState } from 'react';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { Check, X, ArrowDownToLine } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

export const CaptureOrb: React.FC = () => {
  const { captureState } = useWorkspaceStore();
  const targetRef = useRef<HTMLDivElement>(null);
  const [targetState, setTargetState] = useState<string>('idle');
  const isWindowsOrb = typeof window !== 'undefined' && getCurrentWindow().label === "windows-orb-target";

  useEffect(() => {
    if (isWindowsOrb) {
      const unlisten = listen('windows-target-state-changed', (event) => {
        setTargetState(event.payload as string);
      });
      return () => {
        unlisten.then(f => f());
      };
    }
  }, [isWindowsOrb]);

  // If not in the windows-orb-target window and workspace captureState is hidden, don't render
  if (!isWindowsOrb && captureState === 'hidden') return null;

  // Resolved state based on window type
  const activeState = isWindowsOrb ? targetState : captureState;

  // Base container - floating near top center
  const baseClasses = "fixed top-2 left-1/2 -translate-x-1/2 z-50 flex items-center justify-center pointer-events-none transition-all duration-300 ease-out";

  let innerClasses = "flex items-center justify-center transition-all duration-300 ease-out shadow-lg overflow-hidden backdrop-blur-md ";
  let content: React.ReactNode = null;

  switch (activeState) {
    case 'idle':
      // 1. Target remains unobtrusive when idle
      innerClasses += "w-20 h-3 rounded-full bg-black/20 dark:bg-white/20 hover:bg-black/40 border border-white/20 shadow-sm scale-90 opacity-60";
      content = <div className="w-6 h-1 rounded-full bg-white/70"></div>;
      break;

    case 'prominent':
    case 'hovering':
    case 'visible':
      // 2 & 3. User begins dragging supported content -> Target becomes visually prominent
      innerClasses += "w-44 h-12 rounded-2xl bg-white/95 dark:bg-zinc-900/95 border border-black/10 dark:border-white/15 shadow-xl scale-100 opacity-100 ring-2 ring-black/5";
      content = (
        <div className="flex items-center gap-2 text-gray-700 dark:text-zinc-200">
          <ArrowDownToLine size={16} className="text-blue-500 animate-bounce" />
          <span className="font-medium text-xs tracking-wide">ContextSplit Capture</span>
        </div>
      );
      break;

    case 'activated':
      // 4. When dragged content enters the target -> show an activated/highlight state
      innerClasses += "w-52 h-14 rounded-2xl bg-blue-600/95 text-white border border-blue-400 shadow-2xl scale-105 opacity-100 ring-4 ring-blue-400/40";
      content = (
        <div className="flex items-center gap-2 text-white animate-pulse">
          <ArrowDownToLine size={18} strokeWidth={2.5} />
          <span className="font-semibold text-sm tracking-wide">Release to capture</span>
        </div>
      );
      break;

    case 'captured':
    case 'success':
      // 5 & 6. On release -> captured confirmation state
      innerClasses += "w-44 h-12 rounded-2xl bg-emerald-600/95 text-white border border-emerald-400 shadow-xl scale-100 opacity-100";
      content = (
        <div className="flex items-center gap-2 text-white">
          <Check size={18} strokeWidth={3} />
          <span className="font-semibold text-sm">Captured</span>
        </div>
      );
      break;

    case 'cancelled':
    case 'failure':
      // 7. If drag leaves or fails -> return state feedback
      innerClasses += "w-40 h-11 rounded-2xl bg-red-500/90 text-white border border-red-400 shadow-lg scale-95 opacity-90";
      content = (
        <div className="flex items-center gap-2 text-white">
          <X size={16} strokeWidth={2.5} />
          <span className="font-medium text-xs">Cancelled</span>
        </div>
      );
      break;

    default:
      innerClasses += "w-20 h-3 rounded-full bg-black/20 border border-white/20 shadow-sm opacity-60";
      content = <div className="w-6 h-1 rounded-full bg-white/70"></div>;
      break;
  }

  return (
    <div ref={targetRef} className={baseClasses}>
      <div className={innerClasses}>
        {content}
      </div>
    </div>
  );
};
