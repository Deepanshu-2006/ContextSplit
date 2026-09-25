import { MacInteractionSurface } from '../adapters/surfaces/MacInteractionSurface';
import { SurfaceCapturePipeline } from '../services/surfaceCapturePipeline';
import { createInteractionEvent } from '../models/contextInteractionEvents';

/**
 * A development-only simulator that executes a full macOS notch drag-and-drop
 * lifecycle. It uses fake UI events but calls the REAL Context Engine APIs
 * via the InteractionLayer and SurfaceCapturePipeline.
 * 
 * This guarantees the interaction logic works against the actual Rust backend
 * before we build the native OS windowing logic.
 */
export class MacNotchSimulator {
  private surface: MacInteractionSurface;
  private pipeline: SurfaceCapturePipeline;

  constructor() {
    this.surface = new MacInteractionSurface();
    this.pipeline = new SurfaceCapturePipeline(this.surface);
  }

  /**
   * Simulates a user dragging and dropping an item onto the macOS notch.
   * 
   * @param payload The simulated payload (e.g., { type: 'repo', path: '/PayX' })
   */
  async simulateDrop(payload: any): Promise<boolean> {
    console.log('\n--- [MacNotchSimulator] Starting Interaction Sequence ---');
    
    console.log('[Simulator] 1. State: IDLE');
    
    // Simulate user cursor moving to top of screen
    console.log('[Simulator] 2. State: HOVER (cursor approaches)');
    this.surface.highlight();
    
    // Notch expands
    console.log('[Simulator] 3. State: EXPANDED');
    this.surface.expand();

    // User drags an item in
    console.log('[Simulator] 4. State: DRAG_ENTER');
    this.surface.acceptDrag();

    // User holds item there
    console.log('[Simulator] 5. State: DRAG_ACTIVE');
    
    // (ACTION_READY would optionally be resolved via previewing here)
    console.log('[Simulator] 6. State: ACTION_READY (Pending drop)');

    // User releases the mouse
    console.log('[Simulator] 7. State: PROCESSING (User dropped payload)');
    
    const event = createInteractionEvent('context.capture', 'mac_notch', {
      metadata: { simulated: true }
    });

    // The pipeline will trigger 'PROCESSING' (showAction), assignment, and SUCCESS/ERROR
    const result = await this.pipeline.execute(event, payload);

    if (result) {
      console.log('[Simulator] 8. State: SUCCESS (Item captured & assigned by Real Context Engine)');
    } else {
      console.log('[Simulator] 8. State: ERROR (Item rejected by Quality Guard or Engine)');
    }

    console.log('[Simulator] 9. State: COLLAPSED (Resetting UI)');
    this.surface.collapse();
    
    console.log('[Simulator] 10. State: IDLE');
    console.log('--- [MacNotchSimulator] Sequence Complete ---\n');

    return result;
  }
}

// Attach to window for easy debugging from the developer console
if (typeof window !== 'undefined') {
  (window as any).runMacNotchSimulator = async (payload: any = { content: 'PayX repository path' }) => {
    const simulator = new MacNotchSimulator();
    await simulator.simulateDrop(payload);
  };
}
