/**
 * ContextSplit macOS Notch Spring Animation Engine
 *
 * Real spring physics using the underdamped harmonic oscillator equation.
 * Drives width, height, borderRadius, scale, opacity via requestAnimationFrame
 * for buttery-smooth 60-120fps animation.
 *
 * This replaces CSS transition hacks with actual physical simulation.
 */

import { SpringConfig, STATE_DIMENSIONS } from './NotchGeometry';
import { NotchState } from './NotchStateMachine';

// ─── Spring Solver ───────────────────────────────────────────────────

interface SpringState {
  value: number;
  velocity: number;
}

/**
 * Steps a single spring one frame forward.
 * Uses semi-implicit Euler integration of the damped harmonic oscillator:
 *   F = -stiffness * (x - target) - damping * velocity
 *   acceleration = F / mass
 */
function stepSpring(
  current: SpringState,
  target: number,
  config: SpringConfig,
  dt: number
): SpringState {
  const displacement = current.value - target;
  const springForce = -config.stiffness * displacement;
  const dampingForce = -config.damping * current.velocity;
  const acceleration = (springForce + dampingForce) / config.mass;

  const newVelocity = current.velocity + acceleration * dt;
  const newValue = current.value + newVelocity * dt;

  return { value: newValue, velocity: newVelocity };
}

/**
 * Checks if a spring has settled (close enough to target with negligible velocity).
 */
function isSettled(current: SpringState, target: number, threshold = 0.5): boolean {
  return Math.abs(current.value - target) < threshold && Math.abs(current.velocity) < threshold;
}

// ─── Animated Properties ─────────────────────────────────────────────

export interface AnimatedNotchStyle {
  width: number;
  height: number;
  borderRadius: number;
  scale: number;
  opacity: number;
}

interface AnimatedProperty {
  spring: SpringState;
  target: number;
}

/**
 * The NotchSpringEngine manages 5 spring-animated properties and
 * provides a high-performance rAF loop that calls back with interpolated styles.
 */
export class NotchSpringEngine {
  private properties: Record<keyof AnimatedNotchStyle, AnimatedProperty>;
  private springConfig: SpringConfig;
  private rafId: number | null = null;
  private lastTime: number = 0;
  private onUpdate: (style: AnimatedNotchStyle) => void;
  private onSettle: (() => void) | null = null;

  constructor(
    initial: AnimatedNotchStyle,
    springConfig: SpringConfig,
    onUpdate: (style: AnimatedNotchStyle) => void
  ) {
    this.springConfig = springConfig;
    this.onUpdate = onUpdate;
    this.properties = {
      width: { spring: { value: initial.width, velocity: 0 }, target: initial.width },
      height: { spring: { value: initial.height, velocity: 0 }, target: initial.height },
      borderRadius: { spring: { value: initial.borderRadius, velocity: 0 }, target: initial.borderRadius },
      scale: { spring: { value: initial.scale, velocity: 0 }, target: initial.scale },
      opacity: { spring: { value: initial.opacity, velocity: 0 }, target: initial.opacity },
    };
  }

  /**
   * Animate to a new target state with optional spring config override.
   */
  public animateTo(target: AnimatedNotchStyle, config?: SpringConfig, onSettle?: () => void): void {
    if (config) this.springConfig = config;
    this.onSettle = onSettle || null;

    this.properties.width.target = target.width;
    this.properties.height.target = target.height;
    this.properties.borderRadius.target = target.borderRadius;
    this.properties.scale.target = target.scale;
    this.properties.opacity.target = target.opacity;

    if (this.rafId === null) {
      this.lastTime = performance.now();
      this.tick();
    }
  }

  /**
   * Immediately snap to target (no animation).
   */
  public snapTo(target: AnimatedNotchStyle): void {
    for (const key of Object.keys(this.properties) as (keyof AnimatedNotchStyle)[]) {
      this.properties[key].spring = { value: target[key], velocity: 0 };
      this.properties[key].target = target[key];
    }
    this.onUpdate(this.getCurrentStyle());
  }

  /**
   * Get the current interpolated style values.
   */
  public getCurrentStyle(): AnimatedNotchStyle {
    return {
      width: this.properties.width.spring.value,
      height: this.properties.height.spring.value,
      borderRadius: this.properties.borderRadius.spring.value,
      scale: this.properties.scale.spring.value,
      opacity: this.properties.opacity.spring.value,
    };
  }

  /**
   * Convenience: animate to a named NotchState.
   */
  public animateToState(state: NotchState, onSettle?: () => void): void {
    const dim = STATE_DIMENSIONS[state] || STATE_DIMENSIONS.IDLE;
    this.animateTo(
      {
        width: dim.width,
        height: dim.height,
        borderRadius: dim.borderRadius,
        scale: dim.scale,
        opacity: dim.opacity,
      },
      dim.spring,
      onSettle
    );
  }

  private tick = (): void => {
    const now = performance.now();
    // Clamp dt to avoid spiral-of-death on tab switch
    const dt = Math.min((now - this.lastTime) / 1000, 0.064);
    this.lastTime = now;

    let allSettled = true;
    const keys = Object.keys(this.properties) as (keyof AnimatedNotchStyle)[];

    for (const key of keys) {
      const prop = this.properties[key];
      // Per-property thresholds: scale/opacity need sub-pixel precision
      const threshold = key === 'opacity' ? 0.005 : key === 'scale' ? 0.002 : 0.4;
      if (!isSettled(prop.spring, prop.target, threshold)) {
        prop.spring = stepSpring(prop.spring, prop.target, this.springConfig, dt);
        allSettled = false;
      } else {
        prop.spring.value = prop.target;
        prop.spring.velocity = 0;
      }
    }

    this.onUpdate(this.getCurrentStyle());

    if (allSettled) {
      this.rafId = null;
      if (this.onSettle) {
        this.onSettle();
        this.onSettle = null;
      }
    } else {
      this.rafId = requestAnimationFrame(this.tick);
    }
  };

  public destroy(): void {
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
  }
}
