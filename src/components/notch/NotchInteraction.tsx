import React, { useRef, useEffect, useState, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { ArrowDown, Check, Globe, Image as ImageIcon, FileText, FileCode, Loader2 } from 'lucide-react';
import { db } from '../../services/db';
import { NotchState, DraggedItemInfo, normalizeNotchState } from './NotchStateMachine';
import { DragTracker } from './DragTracker';
import { NotchSpringEngine, AnimatedNotchStyle } from './NotchAnimationController';
import { STATE_DIMENSIONS, NOTCH_HARDWARE, NOTCH_TIMINGS } from './NotchGeometry';

/**
 * NotchInteraction — macOS Dynamic Island Capture Surface
 *
 * A system-level overlay component that transforms the MacBook's physical notch
 * into an intelligent drag-and-drop capture surface.
 *
 * Architecture:
 * - Renders inside the `macos-notch-target` transparent overlay window
 * - Positioned by native Rust code at screen top-center using NSScreen coordinates
 * - Spring physics engine drives all dimensional changes (no CSS transitions)
 * - The surface expands DOWNWARD from top:0 (physically attached to the bezel)
 * - transform-origin: top center ensures the top edge never moves
 */
export const NotchInteraction: React.FC = () => {
  const [state, setState] = useState<NotchState>('IDLE');
  const [contextName, setContextName] = useState<string>('TripWise');
  const [draggedItem, setDraggedItem] = useState<DraggedItemInfo>({ type: 'image', title: 'Item' });
  const [animStyle, setAnimStyle] = useState<AnimatedNotchStyle>({
    width: NOTCH_HARDWARE.PHYSICAL_WIDTH,
    height: 0,
    borderRadius: 0,
    scale: 1,
    opacity: 0,
  });
  const [tiltAngle, setTiltAngle] = useState(0);
  const [successKey, setSuccessKey] = useState(0);

  const stateRef = useRef<NotchState>('IDLE');
  const trackerRef = useRef<DragTracker>(new DragTracker());
  const springRef = useRef<NotchSpringEngine | null>(null);
  const timersRef = useRef<ReturnType<typeof setTimeout>[]>([]);

  const isNotchWindow = typeof window !== 'undefined' && getCurrentWindow().label === 'macos-notch-target';

  // ── Initialize spring engine ──────────────────────────────────────
  useEffect(() => {
    if (!isNotchWindow) return;

    const engine = new NotchSpringEngine(
      { width: NOTCH_HARDWARE.PHYSICAL_WIDTH, height: 0, borderRadius: 0, scale: 1, opacity: 0 },
      STATE_DIMENSIONS.IDLE.spring,
      (style) => setAnimStyle(style)
    );
    springRef.current = engine;

    return () => engine.destroy();
  }, [isNotchWindow]);

  // ── Load active workspace name ────────────────────────────────────
  useEffect(() => {
    db.listWorkspaces()
      .then((ws) => {
        if (ws?.length) {
          const tw = ws.find((w) => w.name.toLowerCase().includes('tripwise'));
          setContextName(tw ? tw.name : ws[0].name);
        }
      })
      .catch(() => {});
  }, []);

  // ── Clear all timers ──────────────────────────────────────────────
  const clearTimers = useCallback(() => {
    timersRef.current.forEach(clearTimeout);
    timersRef.current = [];
  }, []);

  const addTimer = useCallback((fn: () => void, ms: number) => {
    timersRef.current.push(setTimeout(fn, ms));
  }, []);

  // ── Transition to state with spring animation ─────────────────────
  const transitionTo = useCallback(
    (next: NotchState, onSettle?: () => void) => {
      setState(next);
      stateRef.current = next;
      springRef.current?.animateToState(next, onSettle);
    },
    []
  );

  // ── Capture success sequence ──────────────────────────────────────
  const triggerSuccess = useCallback(() => {
    clearTimers();

    // 1. Compress into capturing
    transitionTo('CAPTURING');

    // 2. After absorption, show success
    addTimer(() => {
      setSuccessKey((k) => k + 1);
      transitionTo('CAPTURE_SUCCESS');

      // 3. Hold success, then collapse
      addTimer(() => {
        transitionTo('RETURN_TO_IDLE', () => {
          transitionTo('IDLE');
        });
      }, NOTCH_TIMINGS.SUCCESS_HOLD_MS);
    }, NOTCH_TIMINGS.ABSORPTION_MS);
  }, [clearTimers, addTimer, transitionTo]);

  const lastSuccessTimeRef = useRef<number>(0);

  // ── Listen for backend state changes ──────────────────────────────
  useEffect(() => {
    if (!isNotchWindow) return;

    const handlePayload = (payload: any) => {
      const raw = typeof payload === 'string' ? payload : payload?.state || String(payload);
      const next = normalizeNotchState(raw);

      if (next === 'CAPTURE_SUCCESS') {
        lastSuccessTimeRef.current = Date.now();
        triggerSuccess();
        return;
      }

      if (next === 'IDLE' || next === 'RETURN_TO_IDLE') {
        // If we just had a success within 500ms, ignore late IDLEs
        if (Date.now() - lastSuccessTimeRef.current < 500) return;
        
        // Don't interrupt capture animation if we are currently playing it (unless it's been a while)
        if (stateRef.current === 'CAPTURING' || stateRef.current === 'CAPTURE_SUCCESS' || (next === 'IDLE' && stateRef.current === 'RETURN_TO_IDLE')) return;
        
        clearTimers();
        transitionTo(next);
        if (next === 'IDLE') trackerRef.current.reset();
        return;
      }
      
      // Filter out delayed legacy 'Active' payloads from the just-completed drop
      if (next === 'DROP_ZONE_ACTIVE' || next === 'CAPTURING') {
        if (Date.now() - lastSuccessTimeRef.current < 500) {
          return;
        }
      }

      // If we receive a new active state (like DRAG_DETECTED, APPROACHING, DROP_ZONE_ACTIVE)
      // from the backend (and it's not a ghost event), it means a completely new drag session has started.
      // We must clear any leftover timers from the previous success animation immediately!
      clearTimers();
      transitionTo(next);
    };

    const u1 = listen('notch-state-changed', (e) => handlePayload(e.payload));
    const u2 = listen('target-state-changed', (e) => handlePayload(e.payload));

    return () => {
      clearTimers();
      u1.then((f) => f());
      u2.then((f) => f());
    };
  }, [isNotchWindow, clearTimers, transitionTo, triggerSuccess]);

  // ── Pointer tracking (tilt + proximity) ───────────────────────────
  useEffect(() => {
    if (!isNotchWindow) return;

    const onMove = (e: MouseEvent) => {
      const result = trackerRef.current.evaluate(e.clientX, e.clientY, window.innerWidth / 2);
      setTiltAngle(result.tiltAngle);
    };

    const onDragOver = (e: DragEvent) => {
      // PREVENT DEFAULT TO STOP NATIVE MAC GRAY RECTANGLE OVERLAY GHOST
      e.preventDefault(); 
      const types = Array.from(e.dataTransfer?.types || []);
      if (types.includes('text/uri-list')) {
        setDraggedItem({ type: 'url', title: 'Web Link' });
      } else if (types.includes('Files')) {
        setDraggedItem({ type: 'image', title: 'Photo' });
      } else if (types.includes('text/plain')) {
        setDraggedItem({ type: 'text', title: 'Text' });
      }
    };
    
    // Prevent default on document body too just in case
    const onDocumentDragOver = (e: DragEvent) => e.preventDefault();
    
    // Crucial: Prevent default on drop so the webview doesn't navigate to the file!
    const onDrop = (e: DragEvent) => {
      e.preventDefault();
      // Reset tracker state visually on drop
      trackerRef.current.reset();
      setTiltAngle(0);
      
      const files = e.dataTransfer?.files;
      if (files && files.length > 0) {
        const file = files[0];
        const isImage = file.type.startsWith('image/') || file.name.match(/\.(jpg|jpeg|png|gif|webp|bmp)$/i);
        
        if (isImage) {
          const url = URL.createObjectURL(file);
          setDraggedItem(prev => ({ ...prev, type: 'image', previewUrl: url, title: file.name }));
        } else {
          setDraggedItem(prev => ({ ...prev, title: file.name }));
        }
      }
    };

    window.addEventListener('mousemove', onMove, { passive: true });
    window.addEventListener('dragover', onDragOver, { passive: false });
    window.addEventListener('drop', onDrop, { passive: false });
    document.body.addEventListener('dragover', onDocumentDragOver, { passive: false });
    document.body.addEventListener('drop', onDrop, { passive: false });

    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('dragover', onDragOver);
      window.removeEventListener('drop', onDrop);
      document.body.removeEventListener('dragover', onDocumentDragOver);
      document.body.removeEventListener('drop', onDrop);
    };
  }, [isNotchWindow]);

  if (!isNotchWindow) return null;

  // ── Derived state flags ───────────────────────────────────────────
  const isIdle = state === 'IDLE' || state === 'RETURN_TO_IDLE';
  const isStarted = state === 'DRAG_STARTED';
  const isApproaching = state === 'DRAG_APPROACHING';
  const isDropZone = state === 'DROP_ZONE_ACTIVE';
  const isCapturing = state === 'CAPTURING';
  const isSuccess = state === 'CAPTURE_SUCCESS';

  // ── Item icon renderer ────────────────────────────────────────────
  const renderIcon = () => {
    const size = 'w-[18px] h-[18px]';
    switch (draggedItem.type) {
      case 'image': return <ImageIcon className={`${size} text-indigo-300`} />;
      case 'url': return <Globe className={`${size} text-sky-300`} />;
      case 'file': return <FileText className={`${size} text-emerald-300`} />;
      case 'text': return <FileCode className={`${size} text-amber-300`} />;
    }
  };

  // ── Spring-driven inline style for the notch capsule ──────────────
  const capsuleStyle: React.CSSProperties = {
    position: 'absolute',
    top: 0,
    left: '50%',
    width: `${animStyle.width}px`,
    height: `${animStyle.height}px`,
    borderBottomLeftRadius: `${animStyle.borderRadius}px`,
    borderBottomRightRadius: `${animStyle.borderRadius}px`,
    borderTopLeftRadius: 0,
    borderTopRightRadius: 0,
    opacity: animStyle.opacity,
    transform: `translate3d(-50%, 0, 0) scale(${animStyle.scale})`,
    transformOrigin: 'top center',
    // Surface appearance
    background: '#000000',
    backdropFilter: 'blur(40px)',
    WebkitBackdropFilter: 'blur(40px)',
    boxShadow: isIdle
      ? 'none'
      : `0 ${Math.round(animStyle.height * 0.12)}px ${Math.round(animStyle.height * 0.3)}px rgba(0,0,0,0.85),
         0 4px 12px rgba(0,0,0,0.5),
         inset 0 -0.5px 0.5px rgba(255,255,255,0.12)`,
    overflow: 'hidden',
    willChange: 'width, height, transform, opacity',
    pointerEvents: isIdle ? 'none' : 'auto',
  };

  // ── Ear fillet style (concave curve connecting bezel to capsule) ───
  const earOpacity = isIdle ? 0 : 1;
  const earTransition = 'opacity 180ms ease';

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        position: 'relative',
        userSelect: 'none',
        pointerEvents: 'none',
        overflow: 'hidden',
        fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Display", "SF Pro Text", system-ui, sans-serif',
      }}
    >
      {/* ═══ Notch Capsule — the Dynamic Island surface ═══ */}
      <div style={capsuleStyle}>
        {/* Left ear fillet */}
        <svg
          style={{
            position: 'absolute',
            left: -NOTCH_HARDWARE.EAR_FILLET_RADIUS,
            top: 0,
            width: NOTCH_HARDWARE.EAR_FILLET_RADIUS,
            height: NOTCH_HARDWARE.EAR_FILLET_RADIUS,
            opacity: earOpacity,
            transition: earTransition,
            pointerEvents: 'none',
          }}
          viewBox="0 0 10 10"
          fill="none"
        >
          <path d="M 0 0 C 5.52 0 10 4.48 10 10 L 10 0 Z" fill={NOTCH_HARDWARE.BEZEL_COLOR} />
        </svg>

        {/* Right ear fillet */}
        <svg
          style={{
            position: 'absolute',
            right: -NOTCH_HARDWARE.EAR_FILLET_RADIUS,
            top: 0,
            width: NOTCH_HARDWARE.EAR_FILLET_RADIUS,
            height: NOTCH_HARDWARE.EAR_FILLET_RADIUS,
            opacity: earOpacity,
            transition: earTransition,
            pointerEvents: 'none',
          }}
          viewBox="0 0 10 10"
          fill="none"
        >
          <path d="M 0 10 C 0 4.48 4.48 0 10 0 L 0 0 Z" fill={NOTCH_HARDWARE.BEZEL_COLOR} />
        </svg>

        {/* ═══ Internal Content — changes per state ═══ */}
        <div
          style={{
            width: '100%',
            height: '100%',
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            position: 'relative',
            overflow: 'hidden',
          }}
        >
          {/* ── DRAG_STARTED: Minimal notch wake indicator ── */}
          {isStarted && (
            <div style={{
              display: 'flex',
              alignItems: 'center',
              gap: '5px',
              opacity: 0,
              animation: 'notchFadeIn 160ms ease forwards',
            }}>
              <span style={{
                width: 5,
                height: 5,
                borderRadius: '50%',
                background: 'rgba(255,255,255,0.6)',
                animation: 'notchPulse 1.5s ease infinite',
              }} />
              <span style={{
                fontSize: '11px',
                fontWeight: 500,
                letterSpacing: '-0.01em',
                color: 'rgba(255,255,255,0.75)',
              }}>
                ContextSplit
              </span>
            </div>
          )}

          {/* ── DRAG_APPROACHING ── */}
          {isApproaching && !isDropZone && (
            <div style={{
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'flex-start',
              width: '100%',
              height: '100%',
              paddingTop: NOTCH_HARDWARE.PHYSICAL_HEIGHT + 10,
            }}>
              {/* Header row: ↓ Capture to ContextSplit */}
              <div style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                opacity: 0,
                animation: 'notchSlideDown 200ms ease 10ms forwards',
              }}>
                <ArrowDown style={{ width: 10, height: 10, color: 'rgba(255,255,255,0.6)' }} />
                <span style={{
                  fontSize: '13px',
                  fontWeight: 500,
                  color: 'rgba(255,255,255,0.6)',
                  letterSpacing: '-0.015em',
                }}>
                  Capture to <span style={{ fontWeight: 600, color: 'rgba(255,255,255,0.85)' }}>{contextName}</span>
                </span>
              </div>
            </div>
          )}

          {/* ── DROP_ZONE_ACTIVE (Horizontal Apple Layout) ── */}
          {isDropZone && (
            <div style={{
              display: 'flex',
              flexDirection: 'row',
              alignItems: 'center',
              justifyContent: 'center',
              width: '100%',
              height: '100%',
              paddingTop: NOTCH_HARDWARE.PHYSICAL_HEIGHT + 4,
              gap: '12px',
              opacity: 0,
              animation: 'notchFadeIn 150ms ease forwards, notchHoverFloat 3s ease-in-out infinite',
            }}>
              {/* Left: Thumbnail/Icon */}
              <div style={{
                width: 34,
                height: 34,
                borderRadius: 9,
                background: 'rgba(255,255,255,0.08)',
                backdropFilter: 'blur(16px)',
                WebkitBackdropFilter: 'blur(16px)',
                border: '1px solid rgba(255,255,255,0.15)',
                boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
                overflow: 'hidden',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                color: 'rgba(255,255,255,0.9)',
                transform: `rotate(${tiltAngle}deg)`,
                opacity: 0,
                animation: 'notchScaleIn 400ms cubic-bezier(0.175, 0.885, 0.32, 1.275) forwards',
              }}>
                {draggedItem.previewUrl ? (
                  <img
                    src={draggedItem.previewUrl}
                    alt="preview"
                    style={{ width: '100%', height: '100%', objectFit: 'cover' }}
                  />
                ) : (
                  renderIcon()
                )}
              </div>

              {/* Right: Text Stack */}
              <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-start', gap: '0px' }}>
                <span style={{ 
                  fontSize: '13px', 
                  fontWeight: 600, 
                  color: '#ffffff', 
                  letterSpacing: '-0.01em',
                  lineHeight: '1.2',
                }}>
                  Release to capture
                </span>
                <span style={{ 
                  fontSize: '11px', 
                  fontWeight: 500, 
                  color: 'rgba(255,255,255,0.5)', 
                  letterSpacing: '0.01em',
                  lineHeight: '1.2',
                }}>
                  {contextName}
                </span>
              </div>
            </div>
          )}

          {/* ── CAPTURING: Compression + spinner ── */}
          {isCapturing && (
            <div style={{
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              gap: 6,
              paddingTop: NOTCH_HARDWARE.PHYSICAL_HEIGHT - 4,
              opacity: 0,
              animation: 'notchFadeIn 160ms ease forwards',
            }}>
              <Loader2 style={{
                width: 14,
                height: 14,
                color: 'rgba(255,255,255,0.7)',
                animation: 'spin 0.7s linear infinite',
              }} />
              <span style={{
                fontSize: '11.5px',
                fontWeight: 500,
                color: 'rgba(255,255,255,0.8)',
              }}>
                Capturing...
              </span>
            </div>
          )}

          {/* ── CAPTURE_SUCCESS: Confirmation pill ── */}
          {isSuccess && (
            <div
              key={`success-${successKey}`}
              style={{
                display: 'flex',
                flexDirection: 'row',
                alignItems: 'center',
                justifyContent: 'center',
                width: '100%',
                height: '100%',
                paddingTop: NOTCH_HARDWARE.PHYSICAL_HEIGHT + 4,
                gap: '12px',
                opacity: 0,
                animation: 'notchFadeIn 200ms ease forwards',
              }}
            >
              {/* Left: Subtle Green Check Icon */}
              <div style={{
                width: 34,
                height: 34,
                borderRadius: '50%',
                background: 'rgba(34, 197, 94, 0.12)', // Subtle green accent
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                opacity: 0,
                animation: 'notchSuccessIcon 500ms cubic-bezier(0.175, 0.885, 0.32, 1.275) 50ms forwards',
              }}>
                <Check style={{ width: 18, height: 18, color: '#4ade80' }} />
              </div>
              
              {/* Right: Text Stack */}
              <div style={{
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'flex-start',
                opacity: 0,
                animation: 'notchSuccessText 400ms cubic-bezier(0.175, 0.885, 0.32, 1.275) 100ms forwards',
              }}>
                <span style={{ fontSize: '13px', fontWeight: 600, color: '#ffffff', letterSpacing: '-0.01em', lineHeight: '1.2' }}>
                  Captured
                </span>
                <span style={{ fontSize: '11px', fontWeight: 500, color: 'rgba(255,255,255,0.6)', letterSpacing: '0.01em', lineHeight: '1.2' }}>
                  to {contextName}
                </span>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default NotchInteraction;
