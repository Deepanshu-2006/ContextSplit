/**
 * ContextSplit macOS Notch Geometry & Dimensional Specifications
 *
 * Defines the physical notch dimensions, per-state surface specs,
 * and spring physics constants for the Dynamic Island interaction.
 *
 * The notch surface is anchored at the absolute top-center of the
 * physical display (top: 0) and expands DOWNWARD symmetrically.
 */

// ─── Physical MacBook Notch Constants ────────────────────────────────
export const NOTCH_HARDWARE = {
  /** Physical cutout width in CSS px (logical, pre-Retina) */
  PHYSICAL_WIDTH: 178,
  /** Physical cutout depth from top bezel */
  PHYSICAL_HEIGHT: 32,
  /** Concave fillet joining bezel to notch edge */
  EAR_FILLET_RADIUS: 10,
  /** Exact bezel color — pitch black to merge with physical hardware */
  BEZEL_COLOR: '#000000',
} as const;

// ─── Spring Physics Parameters ───────────────────────────────────────
export interface SpringConfig {
  stiffness: number;
  damping: number;
  mass: number;
}

/**
 * Controlled spring: fast response, slight overshoot, no cartoon bounce.
 * Tuned for the "expensive, controlled" feel of Apple system animations.
 */
export const SPRING_DEFAULT: SpringConfig = { stiffness: 340, damping: 26, mass: 1 };
/** Quick spring for snappy compression on capture */
export const SPRING_SNAPPY: SpringConfig = { stiffness: 480, damping: 32, mass: 1 };
/** Gentle spring for returning to idle — no overshoot */
export const SPRING_COLLAPSE: SpringConfig = { stiffness: 220, damping: 28, mass: 1 };

// ─── Per-State Dimensional Specs ─────────────────────────────────────
export interface NotchDimensions {
  width: number;
  height: number;
  borderRadius: number;
  scale: number;
  opacity: number;
  spring: SpringConfig;
}

export const STATE_DIMENSIONS: Record<string, NotchDimensions> = {
  IDLE: {
    width: NOTCH_HARDWARE.PHYSICAL_WIDTH,
    height: 0,
    borderRadius: 0,
    scale: 1,
    opacity: 0,
    spring: SPRING_COLLAPSE,
  },
  DRAG_STARTED: {
    width: NOTCH_HARDWARE.PHYSICAL_WIDTH,
    height: NOTCH_HARDWARE.PHYSICAL_HEIGHT,
    borderRadius: 14,
    scale: 1,
    opacity: 0.98,
    spring: SPRING_DEFAULT,
  },
  DRAG_APPROACHING: {
    width: 210,
    height: 64,
    borderRadius: 22,
    scale: 1,
    opacity: 1,
    spring: SPRING_DEFAULT,
  },
  DROP_ZONE_ACTIVE: {
    width: 240,
    height: 76,
    borderRadius: 24,
    scale: 1.01,
    opacity: 1,
    spring: SPRING_DEFAULT,
  },
  CAPTURING: {
    width: 220,
    height: 104,
    borderRadius: 24,
    scale: 0.98,
    opacity: 1,
    spring: SPRING_SNAPPY,
  },
  CAPTURE_SUCCESS: {
    width: 260,
    height: 68,
    borderRadius: 20,
    scale: 1,
    opacity: 1,
    spring: SPRING_DEFAULT,
  },
  CAPTURE_CANCELLED: {
    width: 200,
    height: 36,
    borderRadius: 16,
    scale: 0.97,
    opacity: 0.85,
    spring: SPRING_COLLAPSE,
  },
  RETURN_TO_IDLE: {
    width: NOTCH_HARDWARE.PHYSICAL_WIDTH,
    height: 0,
    borderRadius: 0,
    scale: 1,
    opacity: 0,
    spring: SPRING_COLLAPSE,
  },
};

// ─── Timing Constants ────────────────────────────────────────────────
export const NOTCH_TIMINGS = {
  /** Time spent showing the 'Capturing...' spinner state */
  ABSORPTION_MS: 500,
  /** Success confirmation hold */
  SUCCESS_HOLD_MS: 1800,
  /** Collapse back to idle */
  COLLAPSE_MS: 350,
} as const;
