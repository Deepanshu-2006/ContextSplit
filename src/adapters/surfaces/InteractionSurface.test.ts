import { describe, it, expect, vi } from 'vitest';
import { MacInteractionSurface } from './MacInteractionSurface';
import { WindowsInteractionSurface } from './WindowsInteractionSurface';
import { InteractionSurface } from './InteractionSurface';

describe('Interaction Surface Adapters', () => {
  it('MacInteractionSurface implements the interface and executes placeholder logic', () => {
    const consoleSpy = vi.spyOn(console, 'log');
    const macSurface: InteractionSurface = new MacInteractionSurface();

    macSurface.show();
    expect(consoleSpy).toHaveBeenCalledWith('[MacSurface] show() -> dropping down from hardware notch');

    macSurface.acceptDrag();
    expect(consoleSpy).toHaveBeenCalledWith('[MacSurface] acceptDrag() -> notch morphing to active drop target');

    macSurface.showAction('Add to active');
    expect(consoleSpy).toHaveBeenCalledWith('[MacSurface] showAction() -> rendering context label: Add to active');

    macSurface.showSuccess('Done');
    expect(consoleSpy).toHaveBeenCalledWith('[MacSurface] showSuccess() -> snapping success animation. Msg: Done');

    consoleSpy.mockRestore();
  });

  it('WindowsInteractionSurface implements the interface and executes placeholder logic', () => {
    const consoleSpy = vi.spyOn(console, 'log');
    const windowsSurface: InteractionSurface = new WindowsInteractionSurface();

    windowsSurface.show();
    expect(consoleSpy).toHaveBeenCalledWith('[WindowsSurface] show() -> fading in floating panel');

    windowsSurface.acceptDrag();
    expect(consoleSpy).toHaveBeenCalledWith('[WindowsSurface] acceptDrag() -> panel highlights as valid drop zone');

    windowsSurface.showAction('Add to active');
    expect(consoleSpy).toHaveBeenCalledWith('[WindowsSurface] showAction() -> rendering tooltip: Add to active');

    windowsSurface.showSuccess('Done');
    expect(consoleSpy).toHaveBeenCalledWith('[WindowsSurface] showSuccess() -> rendering checkmark icon. Msg: Done');

    consoleSpy.mockRestore();
  });
});
