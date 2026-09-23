import { create } from 'zustand';
import { db, ContextCandidate, CandidateWithItems, CandidateScore, ContextReasoningResult, AIFailureReason } from '../services/db';
import { useWorkspaceStore } from './workspaceStore';
import { toast } from 'sonner';

export const HIGH_CONFIDENCE_THRESHOLD = 0.70;

interface DiscoveryState {
  candidates: ContextCandidate[];
  highConfidenceCandidate: ContextCandidate | null;
  selectedCandidateId: string | null;
  selectedCandidateDetails: CandidateWithItems | null;
  selectedCandidateScore: CandidateScore | null;
  aiReasoningResult: ContextReasoningResult | null;
  aiUnavailable: boolean;
  aiUnavailabilityReason: string | null;
  aiUnavailabilityCategory: AIFailureReason | null;
  isAiReasoningLoading: boolean;
  isReviewOpen: boolean;
  isLoading: boolean;
  
  // Actions
  checkForCandidates: () => Promise<void>;
  runDiscoveryScan: () => Promise<void>;
  reviewCandidate: (id: string) => Promise<void>;
  dismissCandidate: (id: string) => Promise<void>;
  updateCandidateName: (id: string, newName: string) => Promise<void>;
  suggestCandidateNameAI: (candidateId: string) => Promise<string | null>;
  removeItemFromCandidate: (candidateId: string, itemId: string) => Promise<void>;
  createWorkspaceFromCandidate: (candidateId: string, workspaceName: string) => Promise<void>;
  closeReview: () => void;
}

export const useDiscoveryStore = create<DiscoveryState>((set, get) => ({
  candidates: [],
  highConfidenceCandidate: null,
  selectedCandidateId: null,
  selectedCandidateDetails: null,
  selectedCandidateScore: null,
  aiReasoningResult: null,
  aiUnavailable: false,
  aiUnavailabilityReason: null,
  aiUnavailabilityCategory: null,
  isAiReasoningLoading: false,
  isReviewOpen: false,
  isLoading: false,

  checkForCandidates: async () => {
    try {
      const [detected, suggested] = await Promise.all([
        db.listContextCandidates('detected'),
        db.listContextCandidates('suggested'),
      ]);
      const combined = [...detected, ...suggested];
      
      // Filter candidates with confidence >= 0.70 (high confidence only)
      const highConfidenceList = combined.filter(
        (c) => c.confidence >= HIGH_CONFIDENCE_THRESHOLD && (c.status === 'detected' || c.status === 'suggested')
      );

      const highDetected = detected.filter((c) => c.confidence >= HIGH_CONFIDENCE_THRESHOLD);

      set({
        candidates: highConfidenceList,
        highConfidenceCandidate: highDetected.length > 0 ? highDetected[0] : null,
      });
    } catch (err) {
      console.error('Failed to check for context candidates:', err);
    }
  },

  runDiscoveryScan: async () => {
    try {
      set({ isLoading: true });
      const newCandidates = await db.runContextDiscovery();
      await get().checkForCandidates();
      const highConfidenceCandidates = get().candidates;

      if (newCandidates && newCandidates.length > 0) {
        toast.success(`Discovered ${newCandidates.length} new context candidate${newCandidates.length > 1 ? 's' : ''}!`);
        await get().reviewCandidate(newCandidates[0].id);
      } else if (highConfidenceCandidates.length > 0) {
        toast.info(`Found ${highConfidenceCandidates.length} context candidate${highConfidenceCandidates.length > 1 ? 's' : ''} ready for review.`);
        await get().reviewCandidate(highConfidenceCandidates[0].id);
      } else {
        toast.info('No new context clusters detected yet. ContextSplit discovers new contexts when you collect 3+ related items in your Inbox.');
      }
    } catch (err: any) {
      console.error('Failed to run discovery scan:', err);
      toast.error(`Discovery scan failed: ${err?.message || err}`);
    } finally {
      set({ isLoading: false });
    }
  },

  reviewCandidate: async (id: string) => {
    try {
      set({ isLoading: true, selectedCandidateId: id, isReviewOpen: true, aiReasoningResult: null });
      
      // Advance candidate status to 'suggested'
      await db.updateCandidateStatus(id, 'suggested');
      
      // Fetch details and score breakdown
      const [details, score] = await Promise.all([
        db.getCandidateItems(id),
        db.scoreContextCandidate(id),
      ]);

      set({
        selectedCandidateDetails: details,
        selectedCandidateScore: score,
        // Remove from current notification popup once in review
        highConfidenceCandidate: null,
      });

      // Refresh candidate list
      await get().checkForCandidates();

      // Asynchronously run AI reasoning on the first item for context explanation
      // This runs in background so it doesn't block the modal opening
      if (details.items.length > 0) {
        set({ isAiReasoningLoading: true });
        const firstItemId = details.items[0].item.id;
        try {
          const aiResponse = await db.analyzeItemContextAI(firstItemId, false);
          if (aiResponse.ai_unavailable) {
            // AI failed gracefully — deterministic modal still shows fully
            set({
              aiReasoningResult: null,
              aiUnavailable: true,
              aiUnavailabilityReason: aiResponse.unavailability_reason,
              aiUnavailabilityCategory: aiResponse.failure_reason,
            });
          } else {
            set({
              aiReasoningResult: aiResponse.ai_result,
              aiUnavailable: false,
              aiUnavailabilityReason: null,
              aiUnavailabilityCategory: null,
            });
          }
        } catch (aiErr) {
          // Unexpected JS-side error (e.g. IPC failure) — treat as unavailable
          console.warn('AI reasoning IPC error:', aiErr);
          set({
            aiReasoningResult: null,
            aiUnavailable: true,
            aiUnavailabilityReason: 'AI analysis could not be loaded',
            aiUnavailabilityCategory: 'internal_error',
          });
        } finally {
          set({ isAiReasoningLoading: false });
        }
      }
    } catch (err) {
      console.error('Failed to load candidate review details:', err);
    } finally {
      set({ isLoading: false });
    }
  },

  dismissCandidate: async (id: string) => {
    try {
      // 1. Update candidate status in database to 'dismissed'
      await db.updateCandidateStatus(id, 'dismissed');

      // 2. Record user dismissal as context feedback signal
      try {
        await db.createContextSignal(
          null,
          null,
          'suggestion_dismissed',
          null,
          JSON.stringify({
            candidate_id: id,
            action: 'dismiss_candidate',
            dismissed_at: new Date().toISOString(),
          })
        );
      } catch (signalErr) {
        console.warn('Could not record dismissal signal:', signalErr);
      }

      // 3. Clear current notification and select next if any
      const { candidates } = get();
      const remainingHigh = candidates.filter(
        (c) => c.id !== id && c.confidence >= HIGH_CONFIDENCE_THRESHOLD && c.status === 'detected'
      );

      set({
        highConfidenceCandidate: remainingHigh.length > 0 ? remainingHigh[0] : null,
        isReviewOpen: false,
        selectedCandidateId: null,
        selectedCandidateDetails: null,
        selectedCandidateScore: null,
        aiReasoningResult: null,
        aiUnavailable: false,
        aiUnavailabilityReason: null,
        aiUnavailabilityCategory: null,
      });

      // Refresh list
      await get().checkForCandidates();
    } catch (err) {
      console.error('Failed to dismiss candidate:', err);
    }
  },

  updateCandidateName: async (id: string, newName: string) => {
    if (!newName.trim()) return;
    try {
      const updated = await db.updateCandidateLabel(id, newName.trim());
      
      const { selectedCandidateDetails } = get();
      if (selectedCandidateDetails && selectedCandidateDetails.candidate.id === id) {
        set({
          selectedCandidateDetails: {
            ...selectedCandidateDetails,
            candidate: updated,
          },
        });
      }

      await get().checkForCandidates();
    } catch (err) {
      console.error('Failed to update candidate name:', err);
    }
  },

  suggestCandidateNameAI: async (candidateId: string) => {
    try {
      const suggested = await db.suggestCandidateNameAI(candidateId);
      if (suggested && suggested.trim()) {
        return suggested.trim();
      }
    } catch (err) {
      console.warn('AI candidate naming unavailable or failed:', err);
    }
    return null;
  },

  removeItemFromCandidate: async (candidateId: string, itemId: string) => {
    try {
      await db.removeCandidateItem(candidateId, itemId);
      
      // Update local items state
      const { selectedCandidateDetails } = get();
      if (selectedCandidateDetails && selectedCandidateDetails.candidate.id === candidateId) {
        const updatedMembers = selectedCandidateDetails.items.filter(
          (m) => m.item.id !== itemId
        );
        set({
          selectedCandidateDetails: {
            ...selectedCandidateDetails,
            items: updatedMembers,
          },
        });

        // If candidate still has items, refresh score breakdown
        if (updatedMembers.length > 0) {
          const remainingIds = updatedMembers.map((m) => m.item.id);
          const newScore = await db.scoreCandidateGroup(remainingIds);
          set({ selectedCandidateScore: newScore });
        }
      }
    } catch (err) {
      console.error('Failed to remove item from candidate:', err);
    }
  },

  createWorkspaceFromCandidate: async (candidateId: string, workspaceName: string) => {
    try {
      set({ isLoading: true });

      // 1. Atomically create workspace, assign items, update status to accepted, and record feedback signal
      const newWs = await db.createWorkspaceFromCandidate(candidateId, workspaceName.trim());

      // 2. Close review modal
      get().closeReview();

      // 3. Refresh workspaces in workspaceStore and navigate to the newly created workspace
      await useWorkspaceStore.getState().loadWorkspaces();
      useWorkspaceStore.getState().selectWorkspace(newWs.id);

      // 4. Refresh candidate list
      await get().checkForCandidates();
    } catch (err) {
      console.error('Failed to create workspace from candidate:', err);
    } finally {
      set({ isLoading: false });
    }
  },

  closeReview: () => {
    set({
      isReviewOpen: false,
      selectedCandidateId: null,
      selectedCandidateDetails: null,
      selectedCandidateScore: null,
      aiReasoningResult: null,
      aiUnavailable: false,
      aiUnavailabilityReason: null,
      aiUnavailabilityCategory: null,
    });
  },
}));
