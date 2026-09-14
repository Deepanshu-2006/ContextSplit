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
    const isIdle = notchState === 'idle';

    // Top-flush container for macOS Notch overlay, hidden and retracted when idle
    const baseNotchContainer = `w-full flex items-start justify-center pointer-events-none origin-top apple-notch-container-transition ${
      isIdle ? '-translate-y-full opacity-0' : 'translate-y-0 opacity-100'
    }`;
    
    let innerClasses = "flex items-center justify-between px-3.5 apple-notch-pill-transition overflow-hidden origin-top select-none mx-auto ";
    let content: React.ReactNode = null;

    switch (notchState) {
      case 'drag_detected':
        // Global drag detected -> notch expands smoothly from MacBook bezel
        innerClasses += "w-[240px] h-[42px] rounded-b-[22px] apple-notch-glass border-b border-x border-white/15";
        content = (
          <div className="flex items-center justify-between w-full">
            <div className="flex items-center gap-2.5">
              <div className="w-6 h-6 rounded-full bg-linear-to-br from-indigo-500/30 to-purple-500/30 border border-indigo-400/40 flex items-center justify-center">
                <Sparkles size={13} className="text-indigo-300 animate-apple-pulse" />
              </div>
              <span className="font-medium text-[13px] tracking-tight text-white/90">ContextSplit</span>
            </div>
            <div className="w-2 h-2 rounded-full bg-indigo-400/80 animate-apple-pulse shadow-[0_0_8px_rgba(129,140,248,0.8)]" />
          </div>
        );
        break;

      case 'hover':
      case 'hovering':
        // Approaching notch -> expands with drop cue
        innerClasses += "w-[270px] h-[48px] rounded-b-[24px] apple-notch-glass border-b border-x border-white/20 shadow-2xl ring-1 ring-white/10";
        content = (
          <div className="flex items-center justify-between w-full">
            <div className="flex items-center gap-2.5">
              <div className="w-7 h-7 rounded-full bg-linear-to-b from-indigo-500/40 to-blue-600/40 border border-indigo-400/50 flex items-center justify-center shadow-inner">
                <ArrowDownToLine size={14} className="text-indigo-300 animate-apple-bounce" />
              </div>
              <div className="flex flex-col text-left">
                <span className="font-semibold text-[13px] tracking-tight text-white">Drop to Capture</span>
                <span className="font-medium text-[10px] tracking-tight text-white/50">Auto-organize context</span>
              </div>
            </div>
            <div className="w-2 h-2 rounded-full bg-indigo-400 shadow-[0_0_8px_rgba(129,140,248,0.9)]" />
          </div>
        );
        break;

      case 'active':
      case 'activated':
        // Inside drop activation zone -> prominent Apple Intelligence aura glow
        innerClasses += "w-[300px] h-[54px] rounded-b-[26px] apple-notch-glass apple-notch-active-glow border-b border-x border-indigo-400/60 scale-[1.02]";
        content = (
          <div className="flex items-center justify-between w-full px-1">
            <div className="flex items-center gap-2.5">
              <div className="w-8 h-8 rounded-full bg-linear-to-tr from-indigo-600 to-purple-600 flex items-center justify-center shadow-[0_0_14px_rgba(99,102,241,0.6)] border border-indigo-300/60">
                <ArrowDownToLine size={16} strokeWidth={2.5} className="text-white animate-apple-bounce" />
              </div>
              <div className="flex flex-col text-left">
                <span className="font-semibold text-[13.5px] tracking-tight text-white">Release to Capture</span>
                <span className="font-medium text-[10px] tracking-tight text-indigo-300/80">Save to ContextSplit</span>
              </div>
            </div>
            <div className="px-2 py-0.5 rounded-full bg-indigo-500/20 border border-indigo-400/40 text-[10px] font-semibold text-indigo-200">
              Ready
            </div>
          </div>
        );
        break;

      case 'paused':
      case 'pause':
        innerClasses += "w-[240px] h-[44px] rounded-b-[22px] apple-notch-glass border-b border-x border-amber-500/40";
        content = (
          <div className="flex items-center justify-between w-full">
            <div className="flex items-center gap-2.5">
              <div className="w-6 h-6 rounded-full bg-amber-500/30 border border-amber-400/40 flex items-center justify-center">
                <PauseCircle size={14} className="text-amber-300" />
              </div>
              <span className="font-medium text-[12.5px] tracking-tight text-white">Capture Paused</span>
            </div>
            <div className="w-2 h-2 rounded-full bg-amber-400" />
          </div>
        );
        break;

      case 'captured':
      case 'success':
        // Success pill with Apple spring pop animation
        innerClasses += "w-[270px] h-[48px] rounded-b-[24px] apple-notch-glass apple-notch-success-glow border-b border-x border-emerald-400/60";
        content = (
          <div className="flex items-center justify-between w-full">
            <div className="flex items-center gap-2.5">
              <div className="w-7 h-7 rounded-full bg-linear-to-tr from-emerald-600 to-teal-500 flex items-center justify-center shadow-[0_0_12px_rgba(16,185,129,0.6)] border border-emerald-300/60 animate-apple-pop">
                <Check size={16} strokeWidth={3} className="text-white" />
              </div>
              <div className="flex flex-col text-left">
                <span className="font-semibold text-[13px] tracking-tight text-white">Captured to Inbox</span>
                <span className="font-medium text-[10px] tracking-tight text-emerald-300/80">Context updated</span>
              </div>
            </div>
            <div className="w-2 h-2 rounded-full bg-emerald-400 shadow-[0_0_8px_rgba(16,185,129,0.9)]" />
          </div>
        );
        break;

      case 'cancelled':
      case 'failure':
        innerClasses += "w-[230px] h-[42px] rounded-b-[22px] apple-notch-glass border-b border-x border-red-500/40";
        content = (
          <div className="flex items-center justify-between w-full">
            <div className="flex items-center gap-2">
              <div className="w-6 h-6 rounded-full bg-red-500/30 border border-red-400/40 flex items-center justify-center">
                <X size={14} className="text-red-300" />
              </div>
              <span className="font-medium text-[12px] tracking-tight text-zinc-300">Cancelled</span>
            </div>
            <div className="w-2 h-2 rounded-full bg-red-400/70" />
          </div>
        );
        break;

      default:
        innerClasses += "w-[170px] h-0 opacity-0 pointer-events-none";
        content = null;
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
