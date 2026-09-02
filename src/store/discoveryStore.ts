import { create } from 'zustand';
import { db, ContextCandidate, CandidateWithItems, CandidateScore } from '../services/db';

export const HIGH_CONFIDENCE_THRESHOLD = 0.70;

interface DiscoveryState {
  candidates: ContextCandidate[];
  highConfidenceCandidate: ContextCandidate | null;
  selectedCandidateId: string | null;
  selectedCandidateDetails: CandidateWithItems | null;
  selectedCandidateScore: CandidateScore | null;
  isReviewOpen: boolean;
  isLoading: boolean;
  
  // Actions
  checkForCandidates: () => Promise<void>;
  runDiscoveryScan: () => Promise<void>;
  reviewCandidate: (id: string) => Promise<void>;
  dismissCandidate: (id: string) => Promise<void>;
  updateCandidateName: (id: string, newName: string) => Promise<void>;
  closeReview: () => void;
}

export const useDiscoveryStore = create<DiscoveryState>((set, get) => ({
  candidates: [],
  highConfidenceCandidate: null,
  selectedCandidateId: null,
  selectedCandidateDetails: null,
  selectedCandidateScore: null,
  isReviewOpen: false,
  isLoading: false,

  checkForCandidates: async () => {
    try {
      const candidates = await db.listContextCandidates('detected');
      
      // Filter candidates with confidence >= 0.70 (high confidence only)
      const highConfidenceList = candidates.filter(
        (c) => c.confidence >= HIGH_CONFIDENCE_THRESHOLD && c.status === 'detected'
      );

      set({
        candidates,
        highConfidenceCandidate: highConfidenceList.length > 0 ? highConfidenceList[0] : null,
      });
    } catch (err) {
      console.error('Failed to check for context candidates:', err);
    }
  },

  runDiscoveryScan: async () => {
    try {
      set({ isLoading: true });
      await db.runContextDiscovery();
      await get().checkForCandidates();
    } catch (err) {
      console.error('Failed to run discovery scan:', err);
    } finally {
      set({ isLoading: false });
    }
  },

  reviewCandidate: async (id: string) => {
    try {
      set({ isLoading: true, selectedCandidateId: id, isReviewOpen: true });
      
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

  closeReview: () => {
    set({
      isReviewOpen: false,
      selectedCandidateId: null,
      selectedCandidateDetails: null,
      selectedCandidateScore: null,
    });
  },
}));
