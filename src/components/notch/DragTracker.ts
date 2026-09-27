/**
 * ContextSplit macOS Drag Tracker
 *
 * Lightweight proximity evaluation for cursor distance to the notch center.
 * Used to compute inertial tilt during drag.
 *
 * Performance-critical: no allocations in the hot path.
 */

export interface DragEvalResult {
  /** Raw distance from cursor to screen top-center */
  distance: number;
  /** Is cursor within the active drop zone rectangle */
  isInDropZone: boolean;
  /** Is cursor within the approach activation radius */
  isApproaching: boolean;
  /** Inertial rotation tilt in degrees (subtle) */
  tiltAngle: number;
  /** Magnetic pull horizontal offset */
  pullX?: number;
  /** Magnetic pull vertical offset */
  pullY?: number;
}

// Activation radius (logical pixels from notch center)
const APPROACH_RADIUS = 250;
// Drop zone half-width from center
const DROP_ZONE_HALF_W = 155;
// Drop zone max Y from top
const DROP_ZONE_MAX_Y = 115;
// Maximum magnetic pull in px
const MAX_PULL_PX = 24;
// Maximum tilt angle in degrees
const MAX_TILT_DEG = 6;

export class DragTracker {
  private prevCursorX = 0;
  private prevTimestamp = 0;
  private smoothTilt = 0;

  /**
   * Evaluate cursor position relative to notch center.
   * @param cx Cursor X (within overlay window)
   * @param cy Cursor Y (within overlay window)
   * @param centerX Notch center X (typically window.innerWidth / 2)
   */
  public evaluate(cx: number, cy: number, centerX: number): DragEvalResult {
    const dx = centerX - cx;
    // Distance from cursor to top-center of screen (notch origin)
    const dist = Math.sqrt(dx * dx + cy * cy);

    // Inertial tilt from horizontal velocity
    const now = performance.now();
    const dt = Math.max(1, now - this.prevTimestamp);
    const vx = (cx - this.prevCursorX) / dt;
    this.prevCursorX = cx;
    this.prevTimestamp = now;

    const rawTilt = vx * 1.8;
    // Smooth exponential decay
    this.smoothTilt += (Math.max(-MAX_TILT_DEG, Math.min(MAX_TILT_DEG, rawTilt)) - this.smoothTilt) * 0.3;

    const isInDropZone = Math.abs(dx) <= DROP_ZONE_HALF_W && cy >= 0 && cy <= DROP_ZONE_MAX_Y;
    const isApproaching = dist <= APPROACH_RADIUS;

    // Magnetic pull calculation
    let pullX = 0;
    let pullY = 0;

    if (isApproaching && dist > 0) {
      const t = 1 - dist / APPROACH_RADIUS;
      const strength = t * t * t; // Cubic ease-in for natural feel
      pullX = (dx / dist) * MAX_PULL_PX * strength;
      pullY = (-cy / dist) * MAX_PULL_PX * strength * 0.5;
    }

    return {
      pullX,
      pullY,
      distance: Math.round(dist),
      isInDropZone,
      isApproaching,
      tiltAngle: Number(this.smoothTilt.toFixed(1)),
    };
  }

  public reset(): void {
    this.prevCursorX = 0;
    this.prevTimestamp = 0;
    this.smoothTilt = 0;
  }
}
