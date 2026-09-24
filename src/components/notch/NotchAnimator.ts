/**
 * ContextSplit macOS Notch Animator — Re-exports
 *
 * This module is kept for backward compatibility.
 * All animation logic lives in NotchAnimationController (spring engine).
 * All geometry lives in NotchGeometry.
 */

export { STATE_DIMENSIONS as NOTCH_STATE_DIMENSIONS } from './NotchGeometry';
export type { NotchDimensions } from './NotchGeometry';
export { NotchSpringEngine } from './NotchAnimationController';
export type { AnimatedNotchStyle } from './NotchAnimationController';
