import React, { useRef, useEffect, useState } from 'react';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { Check, X, ArrowDownToLine, Sparkles, PauseCircle } from 'lucide-react';
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
    // Top-flush container for macOS Notch overlay
    const baseNotchContainer = "fixed top-0 left-1/2 -translate-x-1/2 z-50 flex items-start justify-center pointer-events-none transition-all duration-300 ease-out";
    
    let innerClasses = "flex items-center justify-center transition-all duration-300 ease-out backdrop-blur-xl overflow-hidden ";
    let content: React.ReactNode = null;

    switch (notchState) {
      case 'idle':
        // Hardware-matching unobtrusive camera notch pill flush to the top
        innerClasses += "w-36 h-6 rounded-b-xl bg-black/85 dark:bg-black/95 border-b border-x border-white/10 shadow-sm opacity-70 hover:opacity-100";
        content = (
          <div className="flex items-center gap-1.5 opacity-60">
            <div className="w-2 h-2 rounded-full bg-zinc-800 border border-zinc-700/60 shadow-inner"></div>
            <div className="w-1 h-1 rounded-full bg-blue-500/80 animate-pulse"></div>
          </div>
        );
        break;

      case 'drag_detected':
        // Global drag detected -> notch wakes up and pulses gently
        innerClasses += "w-44 h-8 rounded-b-2xl bg-zinc-950/90 border-b border-x border-blue-500/30 shadow-md ring-1 ring-blue-500/20 animate-pulse";
        content = (
          <div className="flex items-center gap-1.5 text-zinc-300">
            <Sparkles size={13} className="text-blue-400 animate-spin" style={{ animationDuration: '3s' }} />
            <span className="font-medium text-[11px] tracking-wide text-zinc-300">ContextSplit</span>
          </div>
        );
        break;

      case 'hover':
      case 'hovering':
        // Content dragged into proximity -> expands into prominent capture capsule
        innerClasses += "w-56 h-12 rounded-b-2xl bg-white/95 dark:bg-zinc-900/95 border-b border-x border-black/10 dark:border-white/15 shadow-2xl scale-100 opacity-100 ring-2 ring-black/5";
        content = (
          <div className="flex items-center gap-2 text-gray-700 dark:text-zinc-200">
            <ArrowDownToLine size={16} className="text-blue-500 animate-bounce" />
            <span className="font-semibold text-xs tracking-wide">ContextSplit Capture</span>
          </div>
        );
        break;

      case 'active':
      case 'activated':
        // Center drop activation zone -> prominent blue highlight
        innerClasses += "w-60 h-14 rounded-b-2xl bg-blue-600/95 text-white border-b border-x border-blue-400 shadow-2xl scale-105 opacity-100 ring-4 ring-blue-400/40 animate-pulse";
        content = (
          <div className="flex items-center gap-2 text-white">
            <ArrowDownToLine size={18} strokeWidth={2.5} />
            <span className="font-semibold text-sm tracking-wide">Release to capture</span>
          </div>
        );
        break;

      case 'paused':
      case 'pause':
        // Capture paused indicator
        innerClasses += "w-48 h-12 rounded-b-2xl bg-amber-600/95 text-white border-b border-x border-amber-400 shadow-xl scale-100 opacity-100 ring-2 ring-amber-400/40";
        content = (
          <div className="flex items-center gap-2 text-white">
            <PauseCircle size={18} strokeWidth={2.5} />
            <span className="font-semibold text-xs tracking-wide">Capture Paused</span>
          </div>
        );
        break;

      case 'captured':
      case 'success':
        // Successful capture feedback
        innerClasses += "w-52 h-12 rounded-b-2xl bg-emerald-600/95 text-white border-b border-x border-emerald-400 shadow-xl scale-100 opacity-100";
        content = (
          <div className="flex items-center gap-2 text-white">
            <Check size={18} strokeWidth={3} />
            <span className="font-semibold text-sm">Captured</span>
          </div>
        );
        break;

      case 'cancelled':
      case 'failure':
        // Cancelled or invalid drop feedback
        innerClasses += "w-44 h-11 rounded-b-2xl bg-red-500/90 text-white border-b border-x border-red-400 shadow-lg scale-95 opacity-90";
        content = (
          <div className="flex items-center gap-2 text-white">
            <X size={16} strokeWidth={2.5} />
            <span className="font-medium text-xs">Cancelled</span>
          </div>
        );
        break;

      default:
        innerClasses += "w-36 h-6 rounded-b-xl bg-black/85 border-b border-x border-white/10 shadow-sm opacity-70";
        content = <div className="w-2 h-2 rounded-full bg-zinc-800"></div>;
        break;
    }

    return (
      <div ref={targetRef} className={baseNotchContainer}>
        <div className={innerClasses}>
          {content}
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
