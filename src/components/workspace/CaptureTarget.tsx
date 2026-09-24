import React from 'react';
import { NotchInteraction } from '../notch/NotchInteraction';

export { NotchInteraction } from '../notch/NotchInteraction';
export type { NotchState as NotchInteractionState, DragItemType as DragContentType } from '../notch/NotchStateMachine';

/**
 * CaptureTarget - macOS Notch Interactive Surface Entrypoint.
 *
 * Renders the dedicated NotchInteraction component physically attached to the top-center
 * of the MacBook display, behaving as a native Dynamic Island / NotchNook surface.
 */
export const CaptureTarget: React.FC = () => {
  return <NotchInteraction />;
};

export default CaptureTarget;
