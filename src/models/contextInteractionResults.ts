/**
 * The standard outcomes of any context interaction (e.g., drag and drop capture, resume attempt).
 */
export type InteractionOutcome =
  | 'SUCCESS'
  | 'PENDING'
  | 'AMBIGUOUS'
  | 'REJECTED'
  | 'ERROR'
  | 'CANCELLED';

/**
 * Structured response payload that the Context Engine returns to an 
 * interaction surface (Notch, Windows UI, etc.) after a capture or command.
 */
export interface ContextInteractionResult<T = Record<string, unknown>> {
  /** The definitive outcome determined by the Context Engine */
  outcome: InteractionOutcome;
  
  /** Human-readable message explaining the outcome (e.g. "Choose context", "Capture failed") */
  message?: string;
  
  /** Identifier of the primary context involved, if successfully resolved */
  resolvedContextId?: string;
  
  /** List of suggested context IDs if the outcome is AMBIGUOUS */
  suggestedContextIds?: string[];
  
  /** Any supplementary data specific to the interaction */
  metadata?: T;
}

/**
 * Factory helper for standardizing Interaction Results.
 */
export function createInteractionResult<T = Record<string, unknown>>(
  outcome: InteractionOutcome,
  options?: {
    message?: string;
    resolvedContextId?: string;
    suggestedContextIds?: string[];
    metadata?: T;
  }
): ContextInteractionResult<T> {
  return {
    outcome,
    message: options?.message,
    resolvedContextId: options?.resolvedContextId,
    suggestedContextIds: options?.suggestedContextIds,
    metadata: options?.metadata,
  };
}
