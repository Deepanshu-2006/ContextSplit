import React, { useRef, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

type AnimState = 'idle' | 'visible' | 'retracting';

export const CaptureTarget: React.FC = () => {
  const targetRef = useRef<HTMLDivElement>(null);
  const [notchState, setNotchState] = useState<string>('idle');
  const [animState, setAnimState] = useState<AnimState>('idle');
  const [successKey, setSuccessKey] = useState<number>(0);
  const animStateRef = useRef<AnimState>('idle');
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const retractTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isNotch = typeof window !== 'undefined' && getCurrentWindow().label === "macos-notch-target";
  
  const clearAllTimers = () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    if (retractTimerRef.current) {
      clearTimeout(retractTimerRef.current);
      retractTimerRef.current = null;
    }
  };

  const triggerSuccess = () => {
    clearAllTimers();
    setSuccessKey(prev => prev + 1);
    setAnimState('visible');
    animStateRef.current = 'visible';
    setNotchState('captured');

    // 1. Display "Saved" badge for 1.4 seconds
    timerRef.current = setTimeout(() => {
      setAnimState('retracting');
      animStateRef.current = 'retracting';

      // 2. Allow 280ms for the elastic retraction animation to finish, then reset cleanly to idle
      retractTimerRef.current = setTimeout(() => {
        setAnimState('idle');
        animStateRef.current = 'idle';
        setNotchState('idle');
      }, 280);
    }, 1400);
  };

  useEffect(() => {
    if (!isNotch) return;

    const handleStateChange = (payload: any) => {
      const stateStr = typeof payload === 'string' ? payload : (payload?.state || String(payload));
      const s = stateStr.toLowerCase();

      if (s === 'captured' || s === 'success') {
        // Always trigger success immediately on drop for rapid-fire support
        triggerSuccess();
      } else if (s === 'cancelled' || s === 'idle') {
        if (animStateRef.current === 'idle') {
          setNotchState('idle');
        }
      } else if (s === 'hover' || s === 'active' || s === 'drag_detected') {
        if (animStateRef.current === 'retracting') {
          clearAllTimers();
          setAnimState('idle');
          animStateRef.current = 'idle';
        }
        setNotchState(s);
      } else if (animStateRef.current === 'idle') {
        setNotchState(s);
      }
    };

    const unlisten1 = listen('target-state-changed', (event) => {
      handleStateChange(event.payload);
    });
    const unlisten2 = listen('notch-state-changed', (event) => {
      handleStateChange(event.payload);
    });
    const unlistenPause = listen('capture-pause-changed', (event) => {
      if (event.payload === true) {
        setNotchState('paused');
      } else if (animStateRef.current === 'idle') {
        setNotchState('idle');
      }
    });

    return () => {
      clearAllTimers();
      unlisten1.then(f => f());
      unlisten2.then(f => f());
      unlistenPause.then(f => f());
    };
  }, [isNotch]);
  
  if (!isNotch) return null;

  const isSuccessActive = animState === 'visible';
  const isRetracting = animState === 'retracting';
  const isIdle = animState === 'idle' && notchState === 'idle';

  let outlineStateClass = "opacity-0";
  if (isSuccessActive) {
    outlineStateClass = "opacity-100 notch-aura-success";
  } else if (isRetracting) {
    outlineStateClass = "opacity-0 notch-aura-success";
  } else if (notchState === 'drag_detected' || notchState === 'hover' || notchState === 'hovering') {
    outlineStateClass = "opacity-100 notch-aura-hover";
  } else if (notchState === 'active' || notchState === 'activated') {
    outlineStateClass = "opacity-100 notch-aura-active scale-[1.01]";
  } else if (notchState === 'cancelled' || notchState === 'failure') {
    outlineStateClass = "opacity-100 notch-aura-error";
  } else if (notchState === 'paused' || notchState === 'pause') {
    outlineStateClass = "opacity-90 border-b-2 border-x-2 border-amber-400/90 shadow-[0_2px_14px_rgba(251,191,36,0.7)]";
  }

  return (
    <div 
      ref={targetRef} 
      className="w-full flex items-start justify-center pointer-events-none select-none pt-0 -mt-0.5 relative"
    >
      {/* Notch Anchored Center Area */}
      <div className="relative flex items-start justify-center">

        {/* Animated Left Sliding Dynamic Island Wing: "✓ Saved" emerging directly from behind the notch */}
        {animState !== 'idle' && (
          <div 
            key={`wing-${successKey}`}
            className={`notch-wing-left ${
              isSuccessActive ? 'wing-visible' : 'wing-retracting'
            }`}
          >
            {/* Specular Shimmer Sweep */}
            <div className="absolute inset-0 w-1/2 h-full bg-gradient-to-r from-transparent via-white/25 to-transparent pointer-events-none animate-specular-sweep" />

            {/* Animated Clean Glowing Green Tick */}
            <svg 
              key={`tick-${successKey}`}
              className="w-4 h-4 text-emerald-400 shrink-0 drop-shadow-[0_0_8px_rgba(52,211,153,0.9)] animate-tick-elastic"
              viewBox="0 0 24 24" 
              fill="none" 
              stroke="currentColor" 
              strokeWidth="3.2" 
              strokeLinecap="round" 
              strokeLinejoin="round"
            >
              <polyline points="20 6 9 17 4 12" />
            </svg>

            {/* Crisp Apple SF Pro Typography with elastic unroll */}
            <div className="flex items-center pr-1.5 z-10">
              <span key={`text-${successKey}`} className="text-[13px] font-bold text-white tracking-[-0.01em] leading-none animate-text-unroll">
                Saved
              </span>
            </div>
          </div>
        )}

        {/* Exact MacBook Notch Silhouette Perimeter Aura (flush to top bezel, behind notch) */}
        <div 
          className={`notch-ambient-aura ${outlineStateClass} relative flex items-end justify-center pb-0.5`}
        >
          {/* Subtle tiny centered status dot indicator at bottom edge */}
          {!isIdle && (
            <div className="absolute bottom-0.5 left-1/2 -translate-x-1/2 flex items-center justify-center pointer-events-none">
              {isSuccessActive ? (
                <div className="w-1.5 h-1.5 rounded-full bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,1)]" />
              ) : notchState === 'active' || notchState === 'activated' ? (
                <div className="w-1.5 h-1.5 rounded-full bg-indigo-300 shadow-[0_0_8px_rgba(165,180,252,1)] animate-pulse" />
              ) : null}
            </div>
          )}
        </div>

      </div>
    </div>
  );
};

