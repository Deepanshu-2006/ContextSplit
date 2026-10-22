import { describe, it, expect } from 'vitest';
import {
  deriveMinimalWorkset,
  normalizeWorksetItem,
  partitionWorksetItems,
  adaptEngineWorksetToMinimal,
  createLauncherWorksetSlice,
  resolveWorksetItemAction,
  MinimalWorksetItem,
  RawWorksetItemInput,
} from './contextWorkset';
import type { ContextWorkset } from '../services/contextWorkset';

describe('Context Workset Contract & Integration Tests', () => {
  // =========================================================================
  // Requirement 1 & 2: Minimal Information & Existing Models Reuse
  // =========================================================================
  describe('1. Minimal Workset Representation & Existing Models Reuse', () => {
    it('constructs a minimal workset containing only essential resumption information', () => {
      const workset = deriveMinimalWorkset({
        contextId: 'ctx-tripwise-1',
        contextName: 'TripWise Mobile',
        lifecycleState: 'active',
        activeTaskId: 'task-auth-1',
        activeTaskTitle: 'Implement OAuth Login Flow',
        taskConfidence: 0.92,
        rawItems: [
          {
            id: 'item-1',
            title: 'OAuthFlow.tsx',
            itemType: 'file',
            relevanceTier: 'primary',
            confidence: 0.95,
            workspaceId: 'ctx-tripwise-1',
          },
          {
            id: 'item-2',
            title: 'https://auth0.com/docs/api',
            itemType: 'url',
            domain: 'auth0.com',
            relevanceTier: 'supporting',
            confidence: 0.82,
            workspaceId: 'ctx-tripwise-1',
          },
        ],
      });

      expect(workset.contextId).toBe('ctx-tripwise-1');
      expect(workset.contextName).toBe('TripWise Mobile');
      expect(workset.activeTaskTitle).toBe('Implement OAuth Login Flow');
      expect(workset.totalItemCount).toBe(2);
      expect(workset.primaryItems).toHaveLength(1);
      expect(workset.primaryItems[0].title).toBe('OAuthFlow.tsx');
      expect(workset.supportingItems).toHaveLength(1);
      expect(workset.supportingItems[0].domain).toBe('auth0.com');
      expect(workset.confidenceBand).toBe('high');
      expect(workset.readinessBand).toBe('ready');
    });

    it('gracefully handles empty context without inventing tasks or phantom items', () => {
      const workset = deriveMinimalWorkset({
        contextId: 'ctx-empty',
        contextName: 'New Project',
        rawItems: [],
      });

      expect(workset.contextId).toBe('ctx-empty');
      expect(workset.activeTaskId).toBeNull();
      expect(workset.activeTaskTitle).toBeNull();
      expect(workset.items).toHaveLength(0);
      expect(workset.primaryItems).toHaveLength(0);
      expect(workset.totalItemCount).toBe(0);
      expect(workset.confidenceBand).toBe('low');
      expect(workset.readinessBand).toBe('partial');
    });
  });

  // =========================================================================
  // Requirement 3: Confirmed Membership vs Inferred Relevance
  // =========================================================================
  describe('2. Confirmed Membership vs Inferred Relevance', () => {
    it('distinguishes confirmed context items from suggested inbox additions', () => {
      const confirmedItem: RawWorksetItemInput = {
        id: 'item-confirmed-1',
        title: 'Project Roadmap.md',
        itemType: 'file',
        workspaceId: 'ctx-aurora-1',
        confidence: 0.85,
        relevanceTier: 'primary',
      };

      const suggestedItem: RawWorksetItemInput = {
        id: 'item-inferred-1',
        title: 'Figma Mockup Link',
        itemType: 'url',
        workspaceId: null, // Physically in Inbox
        isSuggestedAddition: true,
        confidence: 0.72,
      };

      const normalizedConfirmed = normalizeWorksetItem(confirmedItem, 'ctx-aurora-1');
      const normalizedSuggested = normalizeWorksetItem(suggestedItem, 'ctx-aurora-1');

      expect(normalizedConfirmed.provenance).toBe('confirmed');
      expect(normalizedConfirmed.isSuggestedAddition).toBe(false);
      expect(normalizedConfirmed.physicalLocation).toBe('workspace');

      expect(normalizedSuggested.provenance).toBe('inferred');
      expect(normalizedSuggested.isSuggestedAddition).toBe(true);
      expect(normalizedSuggested.physicalLocation).toBe('inbox');
      expect(normalizedSuggested.relevanceTier).toBe('suggested');
    });

    it('ensures confirmed context items outrank inferred suggestions in the workset ordering', () => {
      const workset = deriveMinimalWorkset({
        contextId: 'ctx-1',
        contextName: 'Core Platform',
        rawItems: [
          {
            id: 'suggested-high-conf',
            title: 'Suggested API Docs',
            itemType: 'url',
            isSuggestedAddition: true,
            confidence: 0.99, // Even with higher raw score
          },
          {
            id: 'confirmed-lower-conf',
            title: 'Workspace Readme.md',
            itemType: 'file',
            workspaceId: 'ctx-1',
            relevanceTier: 'primary',
            confidence: 0.70,
          },
        ],
      });

      // Confirmed item must appear first
      expect(workset.items[0].id).toBe('confirmed-lower-conf');
      expect(workset.items[0].provenance).toBe('confirmed');
      expect(workset.items[1].id).toBe('suggested-high-conf');
      expect(workset.items[1].provenance).toBe('inferred');
    });
  });

  // =========================================================================
  // Requirement 4: Selective Relevance (Not All Workspace Items Enter Workset)
  // =========================================================================
  describe('3. Selective Relevance vs Workspace Accumulation', () => {
    it('excludes stale, unlinked workspace clutter below minimum relevance threshold', () => {
      const workset = deriveMinimalWorkset({
        contextId: 'ctx-1',
        contextName: 'Analytics Engine',
        rawItems: [
          {
            id: 'active-code',
            title: 'parser.rs',
            relevanceTier: 'primary',
            confidence: 0.90,
            workspaceId: 'ctx-1',
          },
          {
            id: 'stale-clutter-1',
            title: 'Old Random Download.png',
            relevanceTier: 'reference',
            confidence: 0.15, // Below 0.28 threshold
            workspaceId: 'ctx-1',
            isImportant: false,
          },
          {
            id: 'stale-clutter-2',
            title: 'Temporary log.txt',
            relevanceTier: 'reference',
            confidence: 0.20, // Below 0.28 threshold
            workspaceId: 'ctx-1',
            isImportant: false,
          },
        ],
      });

      // Only the active primary item meets the relevance cut
      expect(workset.totalItemCount).toBe(1);
      expect(workset.items.map((i) => i.id)).toEqual(['active-code']);
    });

    it('retains older items if user explicitly pinned them as important memory', () => {
      const workset = deriveMinimalWorkset({
        contextId: 'ctx-1',
        contextName: 'Analytics Engine',
        rawItems: [
          {
            id: 'pinned-spec',
            title: 'Architecture Specification v1.pdf',
            relevanceTier: 'reference',
            confidence: 0.10, // Very low recency score
            isImportant: true, // But user marked as important!
            workspaceId: 'ctx-1',
          },
        ],
      });

      expect(workset.totalItemCount).toBe(1);
      expect(workset.items[0].id).toBe('pinned-spec');
      expect(workset.items[0].isImportant).toBe(true);
    });
  });

  // =========================================================================
  // Requirement 5 & 6: Physical Location vs Association & Multi-Context Support
  // =========================================================================
  describe('4. Physical Location vs Contextual Association & Multi-Context', () => {
    it('preserves physical inbox placement while establishing contextual association', () => {
      const inboxSuggested: RawWorksetItemInput = {
        id: 'inbox-item-42',
        title: 'Feedback Note from Slack',
        itemType: 'text',
        workspaceId: null, // Lives in Inbox
        isSuggestedAddition: true,
        confidence: 0.78,
      };

      const normalized = normalizeWorksetItem(inboxSuggested, 'ctx-project-b');
      expect(normalized.physicalLocation).toBe('inbox');
      expect(normalized.workspaceId).toBeNull();
      expect(normalized.contextId).toBe('ctx-project-b');
      expect(normalized.isSuggestedAddition).toBe(true);
    });

    it('supports the same item being referenced in multiple contexts without cross-contamination', () => {
      const sharedResource: RawWorksetItemInput = {
        id: 'design-tokens-url',
        title: 'https://design.corp.com/tokens',
        itemType: 'url',
        domain: 'design.corp.com',
        workspaceId: 'ctx-design-system', // Physically created in Design System
        relevanceTier: 'primary',
        confidence: 0.95,
      };

      // In Context A (Home workspace): Confirmed primary
      const inContextA = normalizeWorksetItem(sharedResource, 'ctx-design-system');
      expect(inContextA.contextId).toBe('ctx-design-system');
      expect(inContextA.provenance).toBe('confirmed');
      expect(inContextA.physicalLocation).toBe('workspace');

      // In Context B (Consuming app): Inferred reference
      const inContextB = normalizeWorksetItem(
        {
          ...sharedResource,
          relevanceTier: 'reference',
          provenance: 'inferred',
          confidence: 0.65,
        },
        'ctx-consumer-app'
      );
      expect(inContextB.contextId).toBe('ctx-consumer-app');
      expect(inContextB.provenance).toBe('inferred');
      expect(inContextB.physicalLocation).toBe('workspace'); // Physical workspace remains ctx-design-system
      expect(inContextB.workspaceId).toBe('ctx-design-system');
      expect(inContextB.relevanceTier).toBe('reference');
    });
  });

  // =========================================================================
  // Requirement 7: Existing Relevance & Calibrated Confidence
  // =========================================================================
  describe('5. Existing Relevance Tiers & Calibrated Confidence', () => {
    it('partitions items correctly across primary, supporting, reference, and suggested tiers', () => {
      const items: MinimalWorksetItem[] = [
        {
          id: '1',
          title: 'Primary 1',
          itemType: 'file',
          relevanceTier: 'primary',
          provenance: 'confirmed',
          physicalLocation: 'workspace',
          workspaceId: 'ctx-1',
          contextId: 'ctx-1',
          confidence: 0.9,
          isSuggestedAddition: false,
          isImportant: true,
          reasons: [],
          status: 'available',
        },
        {
          id: '2',
          title: 'Supporting 1',
          itemType: 'url',
          relevanceTier: 'supporting',
          provenance: 'confirmed',
          physicalLocation: 'workspace',
          workspaceId: 'ctx-1',
          contextId: 'ctx-1',
          confidence: 0.8,
          isSuggestedAddition: false,
          isImportant: false,
          reasons: [],
          status: 'available',
        },
        {
          id: '3',
          title: 'Ref 1',
          itemType: 'url',
          relevanceTier: 'reference',
          provenance: 'confirmed',
          physicalLocation: 'workspace',
          workspaceId: 'ctx-1',
          contextId: 'ctx-1',
          confidence: 0.6,
          isSuggestedAddition: false,
          isImportant: false,
          reasons: [],
          status: 'available',
        },
        {
          id: '4',
          title: 'Inbox Suggestion',
          itemType: 'text',
          relevanceTier: 'suggested',
          provenance: 'inferred',
          physicalLocation: 'inbox',
          workspaceId: null,
          contextId: 'ctx-1',
          confidence: 0.7,
          isSuggestedAddition: true,
          isImportant: false,
          reasons: [],
          status: 'available',
        },
      ];

      const partitioned = partitionWorksetItems(items);
      expect(partitioned.primaryItems).toHaveLength(1);
      expect(partitioned.primaryItems[0].id).toBe('1');
      expect(partitioned.supportingItems).toHaveLength(1);
      expect(partitioned.supportingItems[0].id).toBe('2');
      expect(partitioned.referenceItems).toHaveLength(1);
      expect(partitioned.referenceItems[0].id).toBe('3');
      expect(partitioned.suggestedAdditions).toHaveLength(1);
      expect(partitioned.suggestedAdditions[0].id).toBe('4');
    });

    it('caps primary items at 5 to prevent visual clutter and demotes excess to supporting', () => {
      const primaryItems: MinimalWorksetItem[] = Array.from({ length: 7 }, (_, i) => ({
        id: `primary-${i + 1}`,
        title: `Component ${i + 1}`,
        itemType: 'file',
        relevanceTier: 'primary',
        provenance: 'confirmed',
        physicalLocation: 'workspace',
        workspaceId: 'ctx-1',
        contextId: 'ctx-1',
        confidence: 0.9 - i * 0.05,
        isSuggestedAddition: false,
        isImportant: false,
        reasons: [],
        status: 'available',
      }));

      const partitioned = partitionWorksetItems(primaryItems);
      expect(partitioned.primaryItems).toHaveLength(5);
      expect(partitioned.supportingItems).toHaveLength(2); // Excess demoted safely
    });
  });

  // =========================================================================
  // Requirement 8: Missing, Deleted, Archived, or Unavailable Items
  // =========================================================================
  describe('6. Missing, Deleted, Archived, and Unavailable Items Handling', () => {
    it('normalizes deleted items with clean predictable title and status', () => {
      const deletedItem: RawWorksetItemInput = {
        id: 'deleted-1',
        isDeleted: true,
        relevanceTier: 'primary',
      };

      const normalized = normalizeWorksetItem(deletedItem, 'ctx-1');
      expect(normalized.status).toBe('deleted');
      expect(normalized.title).toBe('[Deleted Item]');
    });

    it('prevents deleted and missing items from entering primary focus', () => {
      const items: MinimalWorksetItem[] = [
        {
          id: 'del-item',
          title: '[Deleted Item]',
          itemType: 'file',
          relevanceTier: 'primary', // Tagged primary before deletion
          provenance: 'confirmed',
          physicalLocation: 'workspace',
          workspaceId: 'ctx-1',
          contextId: 'ctx-1',
          confidence: 0.9,
          isSuggestedAddition: false,
          isImportant: false,
          reasons: [],
          status: 'deleted',
        },
      ];

      const partitioned = partitionWorksetItems(items);
      expect(partitioned.primaryItems).toHaveLength(0);
      expect(partitioned.referenceItems).toHaveLength(1);
    });

    it('disables actions on deleted or missing items in the launcher interface', () => {
      const deletedItem: MinimalWorksetItem = {
        id: 'del-1',
        title: '[Deleted Item]',
        itemType: 'file',
        relevanceTier: 'reference',
        provenance: 'confirmed',
        physicalLocation: 'workspace',
        workspaceId: 'ctx-1',
        contextId: 'ctx-1',
        confidence: 0.5,
        isSuggestedAddition: false,
        isImportant: false,
        reasons: [],
        status: 'deleted',
      };

      const action = resolveWorksetItemAction(deletedItem);
      expect(action.isExecutable).toBe(false);
      expect(action.label).toBe('Deleted Item');
      expect(action.unsupportedReason).toContain('deleted');
    });

    it('resolves executable actions for healthy available items', () => {
      const urlItem: MinimalWorksetItem = {
        id: 'url-1',
        title: 'API Docs',
        itemType: 'url',
        relevanceTier: 'primary',
        provenance: 'confirmed',
        physicalLocation: 'workspace',
        workspaceId: 'ctx-1',
        contextId: 'ctx-1',
        confidence: 0.9,
        isSuggestedAddition: false,
        isImportant: false,
        reasons: [],
        status: 'available',
      };

      const action = resolveWorksetItemAction(urlItem);
      expect(action.isExecutable).toBe(true);
      expect(action.actionType).toBe('open');
      expect(action.label).toBe('Open in Browser');

      const textItem: MinimalWorksetItem = {
        ...urlItem,
        id: 'text-1',
        itemType: 'text',
      };
      const textAction = resolveWorksetItemAction(textItem);
      expect(textAction.isExecutable).toBe(true);
      expect(textAction.actionType).toBe('copy');
      expect(textAction.label).toBe('Copy Note');
    });
  });

  // =========================================================================
  // Requirement 9 & 10: Zero AI Requirement & Optional AI Enrichment
  // =========================================================================
  describe('7. Generation Without AI & Optional Background Enrichment', () => {
    it('generates a complete workset without requiring an AI service or API key', () => {
      const startTime = performance.now();
      const workset = deriveMinimalWorkset({
        contextId: 'ctx-offline',
        contextName: 'Offline Mode Test',
        activeTaskTitle: 'Refactor SQLite Index',
        rawItems: [
          {
            id: 'item-offline-1',
            title: 'mod.rs',
            itemType: 'file',
            relevanceTier: 'primary',
            confidence: 0.95,
            workspaceId: 'ctx-offline',
          },
        ],
      });
      const elapsed = performance.now() - startTime;

      expect(workset.totalItemCount).toBe(1);
      expect(workset.aiEnrichment).toBeNull();
      // Derivation must complete instantly (< 20ms) purely in memory
      expect(elapsed).toBeLessThan(50);
    });

    it('accepts optional AI enrichment outside the core retrieval path without breaking invariants', () => {
      const workset = deriveMinimalWorkset({
        contextId: 'ctx-ai-enriched',
        contextName: 'AI Test Context',
        rawItems: [
          {
            id: 'item-1',
            title: 'spec.md',
            itemType: 'file',
            relevanceTier: 'primary',
            confidence: 0.9,
            workspaceId: 'ctx-ai-enriched',
          },
        ],
        aiEnrichment: {
          summary: 'User was editing spec.md for billing webhook integration.',
          keyConcepts: ['Stripe', 'Webhooks', 'Idempotency'],
          suggestedNextActions: ['Test webhook replay', 'Deploy migration'],
          enrichedAt: '2026-10-22T04:15:00Z',
          provider: 'local-fastembed',
        },
      });

      expect(workset.aiEnrichment).not.toBeNull();
      expect(workset.aiEnrichment?.keyConcepts).toEqual(['Stripe', 'Webhooks', 'Idempotency']);
      expect(workset.totalItemCount).toBe(1);
    });
  });

  // =========================================================================
  // Requirement 11: Backend DTO Compatibility (No Persistent DB Model)
  // =========================================================================
  describe('8. Backend Engine DTO Compatibility & Ephemeral Projection', () => {
    it('adapts real backend ContextWorkset DTO cleanly into MinimalContextWorkset', () => {
      const mockEngineWorkset: ContextWorkset = {
        context_id: 'ctx-engine-1',
        context_name: 'Aurora Website',
        task_id: 'task-hero-1',
        task_title: 'Hero Animation Layout',
        generated_at: '2026-10-22T04:00:00Z',
        last_active_at: '2026-10-22T03:55:00Z',
        workset_state: 'active',
        recent_activity: 'Edited Hero.tsx and referenced framer-motion docs',
        confidence: 0.94,
        confidence_band: 'High',
        provenance: 'confirmed',
        explanation: {
          summary: 'High-confidence workset centered on Hero Animation',
          why_reasons: ['Task is actively in progress'],
          supporting_evidence: ['Active edits in Hero.tsx'],
          confidence_band: 'High',
          provenance: 'confirmed',
        },
        items: [
          {
            id: 'hero-tsx',
            title: 'Hero.tsx',
            item_type: 'file',
            relevance: 'primary',
            relevance_score: 0.95,
            reasons: ['directly tied to current task'],
            provenance: 'confirmed',
            created_at: '2026-10-22T03:00:00Z',
            last_active_at: '2026-10-22T03:55:00Z',
            app_name: 'VS Code',
            domain: null,
            content_snippet: 'export function Hero() { ... }',
            is_suggested_addition: false,
          },
          {
            id: 'framer-docs',
            title: 'Framer Motion Docs',
            item_type: 'url',
            relevance: 'supporting',
            relevance_score: 0.82,
            reasons: ['supporting reference for active task'],
            provenance: 'confirmed',
            created_at: '2026-10-22T03:10:00Z',
            last_active_at: '2026-10-22T03:50:00Z',
            app_name: 'Arc',
            domain: 'framer.com',
            content_snippet: 'https://framer.com/motion',
            is_suggested_addition: false,
          },
          {
            id: 'inbox-suggestion-1',
            title: 'Inbox Note on Keyframes',
            item_type: 'text',
            relevance: 'related',
            relevance_score: 0.68,
            reasons: ['suggested from inbox'],
            provenance: 'inferred',
            created_at: '2026-10-22T03:40:00Z',
            last_active_at: '2026-10-22T03:40:00Z',
            app_name: null,
            domain: null,
            content_snippet: 'Use spring physics for hero entrance',
            is_suggested_addition: true,
          },
        ],
        relevant_items: [],
        important_references: [],
        related_tasks: [],
      };

      const minimal = adaptEngineWorksetToMinimal(mockEngineWorkset, {
        lifecycleState: 'active',
        readinessBand: 'ready',
      });

      expect(minimal.contextId).toBe('ctx-engine-1');
      expect(minimal.contextName).toBe('Aurora Website');
      expect(minimal.activeTaskTitle).toBe('Hero Animation Layout');
      expect(minimal.primaryItems).toHaveLength(1);
      expect(minimal.primaryItems[0].title).toBe('Hero.tsx');
      expect(minimal.supportingItems).toHaveLength(1);
      expect(minimal.supportingItems[0].domain).toBe('framer.com');
      expect(minimal.suggestedAdditions).toHaveLength(1);
      expect(minimal.suggestedAdditions[0].isSuggestedAddition).toBe(true);
      expect(minimal.totalItemCount).toBe(3);
    });
  });

  // =========================================================================
  // Requirement 12: Context Launcher Presentation Interface
  // =========================================================================
  describe('9. Context Launcher Presentation Adapter Interface', () => {
    it('creates a bounded presentation slice for the Context Launcher preview', () => {
      const items: MinimalWorksetItem[] = [
        // 4 primary items (launcher slice caps primary at 3)
        ...Array.from({ length: 4 }, (_, i) => ({
          id: `p-${i}`,
          title: `Primary ${i}`,
          itemType: 'file',
          relevanceTier: 'primary' as const,
          provenance: 'confirmed' as const,
          physicalLocation: 'workspace' as const,
          workspaceId: 'ctx-1',
          contextId: 'ctx-1',
          confidence: 0.9,
          isSuggestedAddition: false,
          isImportant: true,
          reasons: [],
          status: 'available' as const,
        })),
        // 4 reference items (launcher slice caps references at 3)
        ...Array.from({ length: 4 }, (_, i) => ({
          id: `r-${i}`,
          title: `Ref ${i}`,
          itemType: 'url',
          relevanceTier: 'reference' as const,
          provenance: 'confirmed' as const,
          physicalLocation: 'workspace' as const,
          workspaceId: 'ctx-1',
          contextId: 'ctx-1',
          confidence: 0.7,
          isSuggestedAddition: false,
          isImportant: false,
          reasons: [],
          status: 'available' as const,
        })),
        // 3 suggested additions (launcher slice caps at 2)
        ...Array.from({ length: 3 }, (_, i) => ({
          id: `s-${i}`,
          title: `Suggested ${i}`,
          itemType: 'text',
          relevanceTier: 'suggested' as const,
          provenance: 'inferred' as const,
          physicalLocation: 'inbox' as const,
          workspaceId: null,
          contextId: 'ctx-1',
          confidence: 0.65,
          isSuggestedAddition: true,
          isImportant: false,
          reasons: [],
          status: 'available' as const,
        })),
      ];

      const workset = deriveMinimalWorkset({
        contextId: 'ctx-1',
        contextName: 'Large Workspace',
        activeTaskTitle: 'Complete Audit',
        rawItems: items,
      });

      const slice = createLauncherWorksetSlice(workset);
      expect(slice.primaryFocus).toHaveLength(3); // Capped at 3
      expect(slice.references).toHaveLength(3); // Capped at 3
      expect(slice.suggestedAdditions).toHaveLength(2); // Capped at 2
      expect(slice.totalCount).toBe(11);
      expect(slice.hasItems).toBe(true);
    });

    it('identifies accept_suggestion action for suggested inbox items', () => {
      const suggestedItem: MinimalWorksetItem = {
        id: 'sug-1',
        title: 'New API Link',
        itemType: 'url',
        relevanceTier: 'suggested',
        provenance: 'inferred',
        physicalLocation: 'inbox',
        workspaceId: null,
        contextId: 'ctx-1',
        confidence: 0.75,
        isSuggestedAddition: true,
        isImportant: false,
        reasons: [],
        status: 'available',
      };

      const action = resolveWorksetItemAction(suggestedItem);
      expect(action.actionType).toBe('accept_suggestion');
      expect(action.label).toBe('Accept into Context');
      expect(action.isExecutable).toBe(true);
    });
  });
});
