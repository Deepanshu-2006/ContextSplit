import { InteractionSurface } from './InteractionSurface';

/**
 * macOS-specific placeholder adapter for the hardware Notch interaction surface.
 * Defers rendering and OS-level communication to the specific Mac-only Tauri bindings.
 */
export class MacInteractionSurface implements InteractionSurface {
  show(): void {
    console.log('[MacSurface] show() -> dropping down from hardware notch');
  }

  hide(): void {
    console.log('[MacSurface] hide() -> retreating into hardware notch');
  }

  expand(): void {
    console.log('[MacSurface] expand() -> expanding notch dimensions');
  }

  collapse(): void {
    console.log('[MacSurface] collapse() -> returning to idle notch footprint');
  }

  highlight(): void {
    console.log('[MacSurface] highlight() -> subtle notch glow');
  }

  acceptDrag(): void {
    console.log('[MacSurface] acceptDrag() -> notch morphing to active drop target');
  }

  rejectDrag(): void {
    console.log('[MacSurface] rejectDrag() -> shaking notch animation');
  }

  showAction(actionText: string): void {
    console.log(`[MacSurface] showAction() -> rendering context label: ${actionText}`);
  }

  showSuccess(message?: string): void {
    console.log(`[MacSurface] showSuccess() -> snapping success animation. Msg: ${message}`);
  }

  showError(error?: string): void {
    console.log(`[MacSurface] showError() -> red reject flash. Error: ${error}`);
  }
}
