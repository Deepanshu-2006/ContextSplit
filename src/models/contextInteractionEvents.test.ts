import { describe, it, expect } from 'vitest';
import { createInteractionEvent, ContextInteractionEvent } from './contextInteractionEvents';

describe('Context Interaction Events', () => {
  it('should create a basic interaction event with correct defaults', () => {
    const event = createInteractionEvent('context.resume', 'launcher');
    
    expect(event.type).toBe('context.resume');
    expect(event.sourceSurface).toBe('launcher');
    
    // Validate timestamp is ISO string
    expect(event.timestamp).toBeDefined();
    const parsedDate = new Date(event.timestamp);
    expect(parsedDate.toISOString()).toBe(event.timestamp);
    
    // Optional fields should be undefined
    expect(event.contextReference).toBeUndefined();
    expect(event.targetReference).toBeUndefined();
    expect(event.metadata).toBeUndefined();
  });

  it('should create an event with context reference', () => {
    const event = createInteractionEvent('context.pause', 'app', {
      contextReference: 'ctx_123'
    });
    
    expect(event.type).toBe('context.pause');
    expect(event.sourceSurface).toBe('app');
    expect(event.contextReference).toBe('ctx_123');
    expect(event.targetReference).toBeUndefined();
  });

  it('should create an event with target reference (e.g. capture item)', () => {
    const event = createInteractionEvent('context.capture', 'mac_notch', {
      targetReference: 'item_456'
    });
    
    expect(event.type).toBe('context.capture');
    expect(event.sourceSurface).toBe('mac_notch');
    expect(event.targetReference).toBe('item_456');
    expect(event.contextReference).toBeUndefined();
  });

  it('should create an assignment event with both references and metadata', () => {
    const metadataPayload = { confidence: 0.95, method: 'drag_and_drop' };
    const event = createInteractionEvent('context.assign', 'drag_drop', {
      contextReference: 'ctx_123',
      targetReference: 'item_456',
      metadata: metadataPayload
    });
    
    expect(event.type).toBe('context.assign');
    expect(event.sourceSurface).toBe('drag_drop');
    expect(event.contextReference).toBe('ctx_123');
    expect(event.targetReference).toBe('item_456');
    expect(event.metadata).toEqual(metadataPayload);
  });
});
