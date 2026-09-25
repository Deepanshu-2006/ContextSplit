import { describe, it, expect } from 'vitest';
import { createInteractionResult } from './contextInteractionResults';

describe('Context Interaction Results Model', () => {
  it('should create a SUCCESS result with a resolved context', () => {
    const result = createInteractionResult('SUCCESS', {
      message: 'Item captured and assigned',
      resolvedContextId: 'ctx_abc'
    });

    expect(result.outcome).toBe('SUCCESS');
    expect(result.message).toBe('Item captured and assigned');
    expect(result.resolvedContextId).toBe('ctx_abc');
    expect(result.suggestedContextIds).toBeUndefined();
  });

  it('should create an AMBIGUOUS result with suggested candidates', () => {
    const result = createInteractionResult('AMBIGUOUS', {
      message: 'Choose context',
      suggestedContextIds: ['ctx_abc', 'ctx_xyz']
    });

    expect(result.outcome).toBe('AMBIGUOUS');
    expect(result.message).toBe('Choose context');
    expect(result.resolvedContextId).toBeUndefined();
    expect(result.suggestedContextIds).toEqual(['ctx_abc', 'ctx_xyz']);
  });

  it('should create an ERROR result with minimal payload', () => {
    const result = createInteractionResult('ERROR', {
      message: 'File format unsupported'
    });

    expect(result.outcome).toBe('ERROR');
    expect(result.message).toBe('File format unsupported');
  });

  it('should support custom metadata in the result', () => {
    const result = createInteractionResult('PENDING', {
      metadata: { jobId: 'job_123', progress: 0.5 }
    });

    expect(result.outcome).toBe('PENDING');
    expect(result.metadata).toEqual({ jobId: 'job_123', progress: 0.5 });
  });

  it('should support CANCELLED outcome', () => {
    const result = createInteractionResult('CANCELLED');

    expect(result.outcome).toBe('CANCELLED');
    expect(result.message).toBeUndefined();
  });
});
