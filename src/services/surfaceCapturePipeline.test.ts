import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SurfaceCapturePipeline } from './surfaceCapturePipeline';
import { InteractionSurface } from '../adapters/surfaces/InteractionSurface';
import { contextInteractionService } from './contextInteractionService';
import { createInteractionEvent } from '../models/contextInteractionEvents';

// Mock the contextInteractionService
vi.mock('./contextInteractionService', () => ({
  contextInteractionService: {
    captureItem: vi.fn(),
    getActiveContext: vi.fn(),
    getContextSuggestions: vi.fn(),
    assignItemToContext: vi.fn(),
  }
}));

describe('SurfaceCapturePipeline', () => {
  let mockSurface: InteractionSurface;
  let pipeline: SurfaceCapturePipeline;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();

    mockSurface = {
      show: vi.fn(),
      hide: vi.fn(),
      expand: vi.fn(),
      collapse: vi.fn(),
      highlight: vi.fn(),
      acceptDrag: vi.fn(),
      rejectDrag: vi.fn(),
      showAction: vi.fn(),
      showSuccess: vi.fn(),
      showError: vi.fn(),
    };

    pipeline = new SurfaceCapturePipeline(mockSurface);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('should successfully execute pipeline and assign to active context', async () => {
    const event = createInteractionEvent('context.capture', 'mac_notch');
    const payload = { content: 'test.pdf' };
    
    vi.mocked(contextInteractionService.captureItem).mockResolvedValueOnce({ id: 'item_123' });
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce({ workspace_id: 'ctx_abc' });
    
    const result = await pipeline.execute(event, payload);
    
    expect(result).toBe(true);
    expect(mockSurface.showAction).toHaveBeenCalledWith('Processing...');
    expect(contextInteractionService.captureItem).toHaveBeenCalledWith(payload);
    expect(contextInteractionService.assignItemToContext).toHaveBeenCalledWith('item_123', 'ctx_abc');
    expect(mockSurface.showSuccess).toHaveBeenCalledWith('Item captured successfully');
    
    // Fast-forward timers to check collapse
    vi.advanceTimersByTime(1500);
    expect(mockSurface.collapse).toHaveBeenCalled();
  });

  it('should fail gracefully if captureItem returns no ID', async () => {
    const event = createInteractionEvent('context.capture', 'windows_edge');
    const payload = { content: 'invalid' };
    
    vi.mocked(contextInteractionService.captureItem).mockResolvedValueOnce(null);
    
    const result = await pipeline.execute(event, payload);
    
    expect(result).toBe(false);
    expect(mockSurface.showError).toHaveBeenCalledWith('Capture failed');
    expect(contextInteractionService.assignItemToContext).not.toHaveBeenCalled();
  });

  it('should enforce Quality Guard and reject invalid items', async () => {
    const event = createInteractionEvent('context.capture', 'mac_notch');
    const payload = { content: 'test.pdf' };
    
    // Simulate capture returning an item with NO id (which trips our basic quality guard mock in this phase)
    vi.mocked(contextInteractionService.captureItem).mockResolvedValueOnce({ id: '' });
    
    const result = await pipeline.execute(event, payload);
    
    expect(result).toBe(false);
    expect(mockSurface.showError).toHaveBeenCalledWith('Capture failed');
    expect(contextInteractionService.assignItemToContext).not.toHaveBeenCalled();
  });
});
