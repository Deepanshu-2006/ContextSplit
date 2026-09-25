/**
 * Supported types of cross-platform context interaction events.
 */
export type InteractionEventType =
  | 'context.open'
  | 'context.resume'
  | 'context.pause'
  | 'context.capture'
  | 'context.assign'
  | 'context.search'
  | 'context.preview'
  | 'context.mark_important'
  | 'context.create'
  | 'context.switch';

/**
 * Valid source surfaces from which an interaction event can originate.
 */
export type InteractionSourceSurface =
  | 'app'
  | 'launcher'
  | 'mac_notch'
  | 'windows_edge'
  | 'keyboard'
  | 'drag_drop'
  | 'future_automation';

/**
 * Common payload structure for a platform-independent context interaction event.
 */
export interface ContextInteractionEvent<T = Record<string, unknown>> {
  /** Uniquely identifies the event type */
  type: InteractionEventType;
  
  /** ISO 8601 timestamp of when the event occurred */
  timestamp: string;
  
  /** The platform UI surface that initiated the event */
  sourceSurface: InteractionSourceSurface;
  
  /** ID of the context this event pertains to, if applicable */
  contextReference?: string;
  
  /** ID of the target item/entity this event pertains to, if applicable */
  targetReference?: string;
  
  /** Flexible metadata payload for event-specific data */
  metadata?: T;
}

/**
 * Helper to construct a normalized context interaction event.
 */
export function createInteractionEvent<T = Record<string, unknown>>(
  type: InteractionEventType,
  sourceSurface: InteractionSourceSurface,
  options?: {
    contextReference?: string;
    targetReference?: string;
    metadata?: T;
  }
): ContextInteractionEvent<T> {
  return {
    type,
    timestamp: new Date().toISOString(),
    sourceSurface,
    contextReference: options?.contextReference,
    targetReference: options?.targetReference,
    metadata: options?.metadata,
  };
}
