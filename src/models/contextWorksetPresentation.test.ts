import { describe, it, expect } from 'vitest';
import {
  createWorksetPresentationModel,
  createEmptyWorksetPresentation,
  toWorksetPresentationItem,
  WorksetPresentationModel,
} from './contextWorksetPresentation';
import { MinimalContextWorkset, MinimalWorksetItem } from './contextWorkset';

describe('Context Workset Presentation Model', () => {
  const mockContextId = 'ctx-workset-ui-1';

  const createMockItem = (overrides: Partial<MinimalWorksetItem> = {}): MinimalWorksetItem => ({
    id: 'item-1',
    title: 'Component Spec',
    itemType: 'file',
    relevanceTier: 'primary',
    provenance: 'confirmed',
    physicalLocation: 'workspace',
    workspaceId: mockContextId,
    contextId: mockContextId,
    confidence: 0.95,
    isSuggestedAddition: false,
    isImportant: false,
    reasons: ['Active working file'],
    status: 'available',
    appName: 'VS Code',
    domain: null,
    contentSnippet: 'Specification notes',
    createdAt: '2026-10-22T08:00:00Z',
    lastActiveAt: '2026-10-22T09:00:00Z',
    ...overrides,
  });

  // =========================================================================
  // 1. Empty Worksets (Requirements 1, 2, 6)
  // =========================================================================
  describe('1. Empty Worksets', () => {
    it('accurately represents a valid empty workset with zero-invention guarantees', () => {
      const emptyWorkset: MinimalContextWorkset = {
        contextId: mockContextId,
        contextName: 'Clean Workspace',
        lifecycleState: 'active',
        worksetState: 'insufficient_evidence',
        activeTaskId: null,
        activeTaskTitle: null,
        taskConfidence: null,
        generatedAt: '2026-10-22T09:00:00Z',
        lastActiveAt: null,
        confidence: 0.2,
        confidenceBand: 'low',
        readinessBand: 'low',
        readinessExplanation: null,
        recentActivitySummary: null,
        items: [],
        primaryItems: [],
        supportingItems: [],
        referenceItems: [],
        suggestedAdditions: [],
        totalItemCount: 0,
      };

      const presentation = createWorksetPresentationModel(emptyWorkset);

      expect(presentation.contextId).toBe(mockContextId);
      expect(presentation.contextName).toBe('Clean Workspace');
      expect(presentation.lifecycleState).toBe('active');
      expect(presentation.populationState).toBe('empty');
      expect(presentation.hasItems).toBe(false);
      expect(presentation.hasTask).toBe(false);
      expect(presentation.hasSuggestions).toBe(false);
      expect(presentation.totalItemCount).toBe(0);

      // Invariant 1 & 2: Zero invention of nonexistent data
      expect(presentation.activeTaskId).toBeNull();
      expect(presentation.activeTaskTitle).toBeNull();
      expect(presentation.taskConfidence).toBeNull();
      expect(presentation.recentActivitySummary).toBeNull();
      expect(presentation.readinessExplanation).toBeNull();
      expect(presentation.primaryFocus).toEqual([]);
      expect(presentation.references).toEqual([]);
      expect(presentation.suggestedAdditions).toEqual([]);
    });

    it('creates an empty fallback presentation via createEmptyWorksetPresentation', () => {
      const presentation = createEmptyWorksetPresentation('ctx-empty-1', 'New Project', 'active');

      expect(presentation.contextId).toBe('ctx-empty-1');
      expect(presentation.contextName).toBe('New Project');
      expect(presentation.lifecycleState).toBe('active');
      expect(presentation.populationState).toBe('empty');
      expect(presentation.hasItems).toBe(false);
      expect(presentation.hasTask).toBe(false);
      expect(presentation.hasSuggestions).toBe(false);
      expect(presentation.activeTaskTitle).toBeNull();
      expect(presentation.recentActivitySummary).toBeNull();
      expect(presentation.readinessBand).toBe('low');
      expect(presentation.readinessExplanation).toContain('Context workspace is empty');
    });

    it('creates an archived empty fallback with blocked readiness band', () => {
      const presentation = createEmptyWorksetPresentation('ctx-archived', 'Legacy Workspace', 'archived');

      expect(presentation.contextId).toBe('ctx-archived');
      expect(presentation.lifecycleState).toBe('archived');
      expect(presentation.populationState).toBe('empty');
      expect(presentation.readinessBand).toBe('blocked');
      expect(presentation.readinessExplanation).toContain('Context is archived');
    });
  });

  // =========================================================================
  // 2. Complete Worksets (Requirements 3, 5, 7, 8, 9)
  // =========================================================================
  describe('2. Complete Worksets', () => {
    it('creates a bounded complete presentation model with pre-resolved action descriptors', () => {
      // 5 primary items (should be bounded to top 3)
      const primaryItems = Array.from({ length: 5 }, (_, i) =>
        createMockItem({
          id: `primary-${i + 1}`,
          title: `Core File ${i + 1}`,
          itemType: 'file',
          relevanceTier: 'primary',
          confidence: 0.95 - i * 0.05,
        })
      );

      // 2 supporting + 2 reference items (should be bounded to top 3 references)
      const supportingItems = [
        createMockItem({
          id: 'supporting-1',
          title: 'Design Spec Link',
          itemType: 'url',
          relevanceTier: 'supporting',
          domain: 'figma.com',
          confidence: 0.88,
        }),
      ];

      const referenceItems = [
        createMockItem({
          id: 'ref-1',
          title: 'Architecture Doc',
          itemType: 'file',
          relevanceTier: 'reference',
          confidence: 0.85,
        }),
        createMockItem({
          id: 'ref-2',
          title: 'API Schema',
          itemType: 'file',
          relevanceTier: 'reference',
          confidence: 0.80,
        }),
      ];

      // 3 suggested additions (should be bounded to top 2)
      const suggestedAdditions = Array.from({ length: 3 }, (_, i) =>
        createMockItem({
          id: `suggested-${i + 1}`,
          title: `Inbox Link ${i + 1}`,
          itemType: 'url',
          relevanceTier: 'suggested',
          provenance: 'inferred',
          isSuggestedAddition: true,
          confidence: 0.85 - i * 0.05,
        })
      );

      const completeWorkset: MinimalContextWorkset = {
        contextId: mockContextId,
        contextName: 'Frontend Infrastructure',
        lifecycleState: 'active',
        worksetState: 'active',
        activeTaskId: 'task-auth-rewrite',
        activeTaskTitle: 'Refactor Authentication Pipeline',
        taskConfidence: 0.95,
        generatedAt: '2026-10-22T09:00:00Z',
        lastActiveAt: '2026-10-22T09:00:00Z',
        confidence: 0.92,
        confidenceBand: 'high',
        readinessBand: 'ready',
        readinessExplanation: 'All working set files and tasks are ready for resumption.',
        recentActivitySummary: 'Modified token authentication middleware in VS Code',
        items: [...primaryItems, ...supportingItems, ...referenceItems, ...suggestedAdditions],
        primaryItems,
        supportingItems,
        referenceItems,
        suggestedAdditions,
        totalItemCount: 12,
      };

      const presentation: WorksetPresentationModel = createWorksetPresentationModel(completeWorkset);

      expect(presentation.populationState).toBe('complete');
      expect(presentation.hasItems).toBe(true);
      expect(presentation.hasTask).toBe(true);
      expect(presentation.hasSuggestions).toBe(true);
      expect(presentation.totalItemCount).toBe(12);

      // Task grounding
      expect(presentation.activeTaskId).toBe('task-auth-rewrite');
      expect(presentation.activeTaskTitle).toBe('Refactor Authentication Pipeline');
      expect(presentation.taskConfidence).toBe(0.95);

      // Activity grounding
      expect(presentation.recentActivitySummary).toBe('Modified token authentication middleware in VS Code');
      expect(presentation.readinessBand).toBe('ready');

      // Bounded presentation collections (Requirement 9)
      expect(presentation.primaryFocus).toHaveLength(3); // Capped at top 3
      expect(presentation.primaryFocus[0].id).toBe('primary-1');
      expect(presentation.primaryFocus[1].id).toBe('primary-2');
      expect(presentation.primaryFocus[2].id).toBe('primary-3');

      expect(presentation.references).toHaveLength(3); // Capped at top 3
      expect(presentation.references[0].id).toBe('supporting-1');
      expect(presentation.references[1].id).toBe('ref-1');
      expect(presentation.references[2].id).toBe('ref-2');

      expect(presentation.suggestedAdditions).toHaveLength(2); // Capped at top 2
      expect(presentation.suggestedAdditions[0].id).toBe('suggested-1');
      expect(presentation.suggestedAdditions[1].id).toBe('suggested-2');

      // Epistemic provenance distinction (Requirement 4)
      expect(presentation.primaryFocus[0].provenance).toBe('confirmed');
      expect(presentation.primaryFocus[0].isSuggestedAddition).toBe(false);
      expect(presentation.suggestedAdditions[0].provenance).toBe('inferred');
      expect(presentation.suggestedAdditions[0].isSuggestedAddition).toBe(true);

      // Action resolution pre-computed (Requirement 5)
      expect(presentation.primaryFocus[0].action.actionType).toBe('open');
      expect(presentation.primaryFocus[0].action.label).toBe('Open File');
      expect(presentation.primaryFocus[0].action.isExecutable).toBe(true);

      expect(presentation.references[0].action.actionType).toBe('open');
      expect(presentation.references[0].action.label).toBe('Open in Browser');

      expect(presentation.suggestedAdditions[0].action.actionType).toBe('accept_suggestion');
      expect(presentation.suggestedAdditions[0].action.label).toBe('Accept into Context');
    });
  });

  // =========================================================================
  // 3. Partially Populated Worksets (Requirement 6)
  // =========================================================================
  describe('3. Partially Populated Worksets', () => {
    it('classifies worksets with items but no task as partial', () => {
      const itemsWorkset: MinimalContextWorkset = {
        contextId: mockContextId,
        contextName: 'Exploratory Spike',
        lifecycleState: 'active',
        worksetState: 'recently_active',
        activeTaskId: null,
        activeTaskTitle: null,
        taskConfidence: null,
        generatedAt: '2026-10-22T09:00:00Z',
        lastActiveAt: '2026-10-22T08:30:00Z',
        confidence: 0.65,
        confidenceBand: 'medium',
        readinessBand: 'partial',
        readinessExplanation: null,
        recentActivitySummary: null,
        items: [createMockItem({ id: 'item-1' })],
        primaryItems: [createMockItem({ id: 'item-1' })],
        supportingItems: [],
        referenceItems: [],
        suggestedAdditions: [],
        totalItemCount: 1,
      };

      const presentation = createWorksetPresentationModel(itemsWorkset);

      expect(presentation.populationState).toBe('partial');
      expect(presentation.hasItems).toBe(true);
      expect(presentation.hasTask).toBe(false);
      expect(presentation.activeTaskTitle).toBeNull();
      expect(presentation.primaryFocus).toHaveLength(1);
    });

    it('classifies worksets with an active task but zero primary items as partial', () => {
      const taskOnlyWorkset: MinimalContextWorkset = {
        contextId: mockContextId,
        contextName: 'New Goal Context',
        lifecycleState: 'active',
        worksetState: 'active',
        activeTaskId: 'task-goal-1',
        activeTaskTitle: 'Draft Project Architecture Proposal',
        taskConfidence: 0.88,
        generatedAt: '2026-10-22T09:00:00Z',
        lastActiveAt: null,
        confidence: 0.70,
        confidenceBand: 'medium',
        readinessBand: 'partial',
        readinessExplanation: null,
        recentActivitySummary: null,
        items: [],
        primaryItems: [],
        supportingItems: [],
        referenceItems: [],
        suggestedAdditions: [],
        totalItemCount: 0,
      };

      const presentation = createWorksetPresentationModel(taskOnlyWorkset);

      expect(presentation.populationState).toBe('partial');
      expect(presentation.hasItems).toBe(false);
      expect(presentation.hasTask).toBe(true);
      expect(presentation.activeTaskTitle).toBe('Draft Project Architecture Proposal');
      expect(presentation.primaryFocus).toHaveLength(0);
    });

    it('classifies worksets with only inferred inbox suggestions as partial', () => {
      const suggestionsOnlyWorkset: MinimalContextWorkset = {
        contextId: mockContextId,
        contextName: 'Candidate Ingestion',
        lifecycleState: 'active',
        worksetState: 'insufficient_evidence',
        activeTaskId: null,
        activeTaskTitle: null,
        taskConfidence: null,
        generatedAt: '2026-10-22T09:00:00Z',
        lastActiveAt: null,
        confidence: 0.50,
        confidenceBand: 'medium',
        readinessBand: 'partial',
        readinessExplanation: null,
        recentActivitySummary: null,
        items: [
          createMockItem({
            id: 'sug-1',
            relevanceTier: 'suggested',
            provenance: 'inferred',
            isSuggestedAddition: true,
          }),
        ],
        primaryItems: [],
        supportingItems: [],
        referenceItems: [],
        suggestedAdditions: [
          createMockItem({
            id: 'sug-1',
            relevanceTier: 'suggested',
            provenance: 'inferred',
            isSuggestedAddition: true,
          }),
        ],
        totalItemCount: 1,
      };

      const presentation = createWorksetPresentationModel(suggestionsOnlyWorkset);

      expect(presentation.populationState).toBe('partial');
      expect(presentation.hasItems).toBe(true);
      expect(presentation.hasTask).toBe(false);
      expect(presentation.hasSuggestions).toBe(true);
      expect(presentation.primaryFocus).toHaveLength(0);
      expect(presentation.suggestedAdditions).toHaveLength(1);
    });
  });

  // =========================================================================
  // 4. Unavailable, Missing, and Deleted Items (Requirements 1, 8)
  // =========================================================================
  describe('4. Unavailable, Missing, and Deleted Items in Presentation', () => {
    it('presents deleted items with disabled actions and explicit status', () => {
      const deletedItem = createMockItem({
        id: 'item-deleted',
        title: '[Deleted Item]',
        status: 'deleted',
        relevanceTier: 'reference',
      });

      const presentationItem = toWorksetPresentationItem(deletedItem);

      expect(presentationItem.id).toBe('item-deleted');
      expect(presentationItem.title).toBe('[Deleted Item]');
      expect(presentationItem.status).toBe('deleted');
      expect(presentationItem.action.isExecutable).toBe(false);
      expect(presentationItem.action.label).toBe('Deleted Item');
      expect(presentationItem.action.unsupportedReason).toContain('deleted and cannot be opened');
    });

    it('presents missing items with disabled actions and explicit status', () => {
      const missingItem = createMockItem({
        id: 'item-missing',
        title: '[Unavailable Item]',
        status: 'missing',
        relevanceTier: 'reference',
      });

      const presentationItem = toWorksetPresentationItem(missingItem);

      expect(presentationItem.id).toBe('item-missing');
      expect(presentationItem.title).toBe('[Unavailable Item]');
      expect(presentationItem.status).toBe('missing');
      expect(presentationItem.action.isExecutable).toBe(false);
      expect(presentationItem.action.label).toBe('Unavailable');
      expect(presentationItem.action.unsupportedReason).toContain('missing or unavailable');
    });
  });

  // =========================================================================
  // 5. Zero-Invention Guarantees (Requirements 1, 2)
  // =========================================================================
  describe('5. Zero-Invention Invariants', () => {
    it('strictly maps whitespace or empty optional strings to null', () => {
      const worksetWithEmptyStrings: MinimalContextWorkset = {
        contextId: mockContextId,
        contextName: 'Test Context',
        lifecycleState: 'active',
        worksetState: 'active',
        activeTaskId: '   ',
        activeTaskTitle: '   ', // whitespace only
        taskConfidence: null,
        generatedAt: '2026-10-22T09:00:00Z',
        lastActiveAt: '   ',
        confidence: 0.5,
        confidenceBand: 'low',
        readinessBand: 'ready',
        readinessExplanation: '   ',
        recentActivitySummary: '   ',
        items: [],
        primaryItems: [],
        supportingItems: [],
        referenceItems: [],
        suggestedAdditions: [],
        totalItemCount: 0,
      };

      const presentation = createWorksetPresentationModel(worksetWithEmptyStrings);

      expect(presentation.hasTask).toBe(false);
      expect(presentation.activeTaskTitle).toBeNull();
      expect(presentation.activeTaskId).toBeNull();
      expect(presentation.recentActivitySummary).toBeNull();
      expect(presentation.readinessExplanation).toBeNull();
      expect(presentation.lastActiveAt).toBeNull();
    });
  });
});
