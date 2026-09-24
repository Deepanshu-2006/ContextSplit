/**
 * ContextSplit macOS Notch State Machine (Section 15 Specification)
 *
 * Implements the 8 responsive states for the MacBook physical notch surface:
 * IDLE -> DRAG_STARTED -> DRAG_APPROACHING -> DROP_ZONE_ACTIVE -> CAPTURING -> CAPTURE_SUCCESS -> RETURN_TO_IDLE
 * (with CAPTURE_CANCELLED for drag leave / abort).
 */

export type NotchState = 
  | 'IDLE'
  | 'DRAG_STARTED'
  | 'DRAG_APPROACHING'
  | 'DROP_ZONE_ACTIVE'
  | 'CAPTURING'
  | 'CAPTURE_SUCCESS'
  | 'CAPTURE_CANCELLED'
  | 'RETURN_TO_IDLE';

export type DragItemType = 'image' | 'url' | 'file' | 'text';

export interface DraggedItemInfo {
  type: DragItemType;
  title?: string;
  previewUrl?: string;
}

export interface NotchProximityMetrics {
  cursorX: number;
  cursorY: number;
  distanceToCenter: number;
  proximityRatio: number;
  isInDropZone: boolean;
  isApproaching: boolean;
}

/**
 * Normalizes backend, legacy, and uppercase state strings into canonical NotchState.
 */
export function normalizeNotchState(rawState: string | undefined | null): NotchState {
  if (!rawState) return 'IDLE';
  const s = rawState.toUpperCase().replace(/-/g, '_').trim();

  switch (s) {
    case 'IDLE':
      return 'IDLE';

    case 'DRAG_STARTED':
    case 'DRAG_DETECTED':
    case 'APPROACH':
    case 'DRAG_APPROACH':
      return 'DRAG_STARTED';

    case 'DRAG_APPROACHING':
    case 'APPROACHING':
    case 'HOVER':
    case 'DRAG_HOVER':
      return 'DRAG_APPROACHING';

    case 'DROP_ZONE_ACTIVE':
    case 'DROP_TARGET':
    case 'CAPTURE_READY':
    case 'ACTIVE':
    case 'EXPANDED':
      return 'DROP_ZONE_ACTIVE';

    case 'CAPTURING':
    case 'SAVING':
      return 'CAPTURING';

    case 'CAPTURE_SUCCESS':
    case 'SUCCESS':
    case 'CAPTURED':
      return 'CAPTURE_SUCCESS';

    case 'CAPTURE_CANCELLED':
    case 'CANCELLED':
    case 'CANCELED':
      return 'CAPTURE_CANCELLED';

    case 'RETURN_TO_IDLE':
      return 'RETURN_TO_IDLE';

    default:
      return 'IDLE';
  }
}
