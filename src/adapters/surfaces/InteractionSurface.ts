/**
 * Abstract interface representing a generic ContextSplit interaction surface.
 * Platforms (macOS Notch, Windows Edge, etc.) must implement this contract
 * to respond to Context Engine states and lifecycle events.
 */
export interface InteractionSurface {
  /**
   * Called to show or activate the surface from a completely hidden state.
   */
  show(): void;

  /**
   * Called to completely hide the surface.
   */
  hide(): void;

  /**
   * Called to expand the surface to its maximum functional size.
   */
  expand(): void;

  /**
   * Called to collapse the surface back to its minimal/idle footprint.
   */
  collapse(): void;

  /**
   * Called to draw attention to the surface (e.g., hover state or drag approach).
   */
  highlight(): void;

  /**
   * Called to visually accept a drag operation (e.g., spring tension, ready-to-drop styling).
   */
  acceptDrag(): void;

  /**
   * Called to visually reject a drag operation or indicate an invalid item.
   */
  rejectDrag(): void;

  /**
   * Called to display a specific contextual action (e.g., "Add to TripWise").
   * @param actionText The text or metadata for the resolved action.
   */
  showAction(actionText: string): void;

  /**
   * Called to display a positive completion visual (e.g., green checkmark).
   * @param message Optional message to display on success.
   */
  showSuccess(message?: string): void;

  /**
   * Called to display a negative completion visual (e.g., red X or shake).
   * @param error Optional error message.
   */
  showError(error?: string): void;
}
