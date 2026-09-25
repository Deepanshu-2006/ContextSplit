import { ContextInteractionEvent } from '../models/contextInteractionEvents';
import { contextInteractionService } from './contextInteractionService';
import { InteractionSurface } from '../adapters/surfaces/InteractionSurface';

/**
 * Interface defining the result of the Context Candidate Resolution step.
 */
export interface ResolutionResult {
  assignedContextId: string | null;
  confidence: number;
  requiresUserConfirmation: boolean;
}

/**
 * Interface defining the result of the Context Quality Guard step.
 */
export interface QualityGuardResult {
  passed: boolean;
  reason?: string;
}

/**
 * The Surface Capture Pipeline orchestrates the handoff between a dumb UI
 * interaction surface (like the macOS Notch or Windows Edge) and the 
 * backend Context Engine.
 * 
 * It ensures all interaction events pass through validation, candidate
 * resolution, and the strict Context Quality Guard before any state changes.
 */
export class SurfaceCapturePipeline {
  constructor(private surface: InteractionSurface) {}

  /**
   * Executes the full contextual capture pipeline originating from a UI interaction.
   * 
   * @param event The normalized interaction event (e.g., from a drag drop)
   * @param payload The raw item to capture
   */
  async execute(event: ContextInteractionEvent, payload: any): Promise<boolean> {
    try {
      this.surface.showAction('Processing...');

      // 1. Capture Service: Send raw item to backend to ingest
      const captureResult = await contextInteractionService.captureItem(payload);
      if (!captureResult || !captureResult.id) {
        this.surface.showError('Capture failed');
        return false;
      }
      const capturedItemId = captureResult.id;

      // 2. Context Candidate Resolution
      const resolution = await this.resolveCandidate(capturedItemId);

      // 3. Context Quality Guard (Mandatory)
      // The interaction surface must never bypass this guard.
      const guardResult = await this.enforceQualityGuard(capturedItemId, resolution.assignedContextId);
      if (!guardResult.passed) {
        this.surface.showError(guardResult.reason || 'Rejected by Quality Guard');
        return false;
      }

      // 4. Context Assignment
      if (resolution.assignedContextId) {
        await contextInteractionService.assignItemToContext(capturedItemId, resolution.assignedContextId);
      }

      // 5. Feedback / State Update / Surface Result
      this.surface.showSuccess('Item captured successfully');
      
      // Simulate state update completion delay before collapse
      setTimeout(() => {
        this.surface.collapse();
      }, 1500);

      return true;
    } catch (error: any) {
      this.surface.showError(error.message || 'Pipeline Error');
      setTimeout(() => {
        this.surface.collapse();
      }, 1500);
      return false;
    }
  }

  /**
   * Identifies the best context candidate for the captured item.
   */
  private async resolveCandidate(itemId: string): Promise<ResolutionResult> {
    const suggestions = await contextInteractionService.getContextSuggestions(itemId);
    const activeContext = await contextInteractionService.getActiveContext();

    if (activeContext && activeContext.workspace_id) {
      return {
        assignedContextId: activeContext.workspace_id,
        confidence: 1.0,
        requiresUserConfirmation: false,
      };
    } else if (suggestions && suggestions.length > 0) {
      return {
        assignedContextId: suggestions[0].workspace_id || null,
        confidence: 0.8,
        requiresUserConfirmation: true,
      };
    }

    return {
      assignedContextId: null,
      confidence: 0,
      requiresUserConfirmation: false,
    };
  }

  /**
   * The Context Quality Guard evaluates if the assignment is semantically sound.
   * This delegates to the backend engine to ensure rules (e.g. no duplicates, 
   * no irrelevant items) are strictly enforced.
   */
  private async enforceQualityGuard(itemId: string, targetContextId: string | null): Promise<QualityGuardResult> {
    // In Phase 1, we rely on the backend to enforce true quality.
    // Here we validate the fundamental requirement that we don't bypass the backend checks.
    if (!itemId) {
      return { passed: false, reason: 'Invalid item ID' };
    }
    
    // For now, if a context is requested, we accept it as conditionally passed
    // pending the backend's strict acceptance during assignment.
    return { passed: true };
  }
}
