import { InteractionSurface } from './InteractionSurface';

/**
 * Windows-specific placeholder adapter for the interaction surface.
 * Handles the mapping between the generic contract and the transparent 
 * always-on-top window or edge-panel used on Windows.
 */
export class WindowsInteractionSurface implements InteractionSurface {
  show(): void {
    console.log('[WindowsSurface] show() -> fading in floating panel');
  }

  hide(): void {
    console.log('[WindowsSurface] hide() -> fading out floating panel');
  }

  expand(): void {
    console.log('[WindowsSurface] expand() -> scaling up floating pill');
  }

  collapse(): void {
    console.log('[WindowsSurface] collapse() -> returning floating pill to minimized icon');
  }

  highlight(): void {
    console.log('[WindowsSurface] highlight() -> glowing panel border');
  }

  acceptDrag(): void {
    console.log('[WindowsSurface] acceptDrag() -> panel highlights as valid drop zone');
  }

  rejectDrag(): void {
    console.log('[WindowsSurface] rejectDrag() -> panel flashes red');
  }

  showAction(actionText: string): void {
    console.log(`[WindowsSurface] showAction() -> rendering tooltip: ${actionText}`);
  }

  showSuccess(message?: string): void {
    console.log(`[WindowsSurface] showSuccess() -> rendering checkmark icon. Msg: ${message}`);
  }

  showError(error?: string): void {
    console.log(`[WindowsSurface] showError() -> rendering error icon. Error: ${error}`);
  }
}
