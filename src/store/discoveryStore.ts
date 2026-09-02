import { create } from 'zustand';
import { db, ContextCandidate, CandidateWithItems, CandidateScore } from '../services/db';
import { useWorkspaceStore } from './workspaceStore';

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

      // 1. Fetch current candidate member items
      const details = await db.getCandidateItems(candidateId);
      const itemIds = details.items.map((m) => m.item.id);

      // 2. Create the new Workspace with the chosen name
      const newWs = await db.createWorkspace(workspaceName.trim() || 'New Workspace');

      // 3. Move items to the newly created workspace
      if (itemIds.length > 0) {
        await Promise.all(itemIds.map((id) => db.moveItem(id, newWs.id)));
      }

      // 4. Mark candidate as accepted
      await db.updateCandidateStatus(candidateId, 'accepted');

      // 5. Record user feedback signal
      try {
        await db.createContextSignal(
          newWs.id,
          null,
          'suggestion_accepted',
          null,
          JSON.stringify({
            candidate_id: candidateId,
            workspace_id: newWs.id,
            item_count: itemIds.length,
            created_at: new Date().toISOString(),
          })
        );
      } catch (sigErr) {
        console.warn('Could not record suggestion_accepted signal:', sigErr);
      }

      // 6. Close review modal
      get().closeReview();

      // 7. Refresh workspaces in workspaceStore and navigate to the new workspace
      await useWorkspaceStore.getState().loadWorkspaces();
      useWorkspaceStore.getState().selectWorkspace(newWs.id);

      // Refresh candidate list
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
    });
  },
}));

