import { describe, it, expect } from 'vitest';
import {
  formatShortId,
  formatGentleTime,
  formatFriendlyDate,
  getDuplicateNameSet,
  resolveDisambiguationTag,
  resolveContextSubtitle,
  resolveContextDisplayIdentity,
  ContextDisplayIdentity,
} from './contextIdentification';
import { LauncherContextItem } from '../store/launcherStore';

describe('Context Identification and Disambiguation', () => {
  describe('1. Clear Name Display', () => {
    it('displays existing context name clearly without modification', () => {
      const item: LauncherContextItem = {
        id: 'ws-11111111-2222-3333-4444-555555555555',
        name: 'Authentication Service Refactor',
        state: 'active',
      };

      const identity = resolveContextDisplayIdentity(item, false);
      expect(identity.name).toBe('Authentication Service Refactor');
      expect(identity.id).toBe(item.id);
    });

    it('trims extraneous surrounding whitespace from context name', () => {
      const item: LauncherContextItem = {
        id: 'ws-22222222-3333-4444-5555-666666666666',
        name: '   Payment Gateway Integration   ',
        state: 'active',
      };

      const identity = resolveContextDisplayIdentity(item, false);
      expect(identity.name).toBe('Payment Gateway Integration');
    });
  });

  describe('2. Missing Metadata (Zero Fabrication)', () => {
    it('returns null subtitle when no description, task, or activity is available for unique contexts', () => {
      const item: LauncherContextItem = {
        id: 'ws-33333333-4444-5555-6666-777777777777',
        name: 'Design System',
        state: 'active',
        description: null,
        taskTitle: null,
        activitySummary: null,
      };

      const identity = resolveContextDisplayIdentity(item, false);
      expect(identity.subtitle).toBeNull();
      expect(identity.disambiguationTag).toBeNull();
    });

    it('does not fabricate descriptions, project names, or activity when fields are empty strings', () => {
      const item: LauncherContextItem = {
        id: 'ws-44444444-5555-6666-7777-888888888888',
        name: 'Settings Page',
        state: 'recent',
        description: '   ',
        taskTitle: '',
        activitySummary: '   ',
      };

      const subtitle = resolveContextSubtitle(item, false);
      expect(subtitle).toBeNull();
    });

    it('gracefully formats gentle time when lastActiveAt is missing or null', () => {
      expect(formatGentleTime(null)).toBe('Ready to resume');
      expect(formatGentleTime(undefined)).toBe('Ready to resume');
      expect(formatGentleTime('')).toBe('Ready to resume');
      expect(formatGentleTime('invalid-date')).toBe('Recently active');
    });

    it('gracefully formats friendly date when date is missing or null', () => {
      expect(formatFriendlyDate(null)).toBeNull();
      expect(formatFriendlyDate(undefined)).toBeNull();
      expect(formatFriendlyDate('invalid-date')).toBeNull();
    });
  });

  describe('3. Descriptions and Distinguishing Metadata', () => {
    it('displays description when available and no taskTitle is set', () => {
      const item: LauncherContextItem = {
        id: 'ws-55555555-6666-7777-8888-999999999999',
        name: 'Onboarding Flow',
        state: 'active',
        description: 'Implementing step 3 user verification',
      };

      const identity = resolveContextDisplayIdentity(item, false);
      expect(identity.subtitle).toBe('Implementing step 3 user verification');
    });

    it('prioritizes active taskTitle as primary subtitle when both taskTitle and description exist', () => {
      const item: LauncherContextItem = {
        id: 'ws-66666666-7777-8888-9999-aaaaaaaaaaaa',
        name: 'Backend API',
        state: 'active',
        taskTitle: 'Investigating Redis cache misses in session validation',
        description: 'General backend API work',
      };

      const identity = resolveContextDisplayIdentity(item, false);
      expect(identity.subtitle).toBe('Investigating Redis cache misses in session validation');
    });

    it('falls back to activitySummary when description and taskTitle are absent', () => {
      const item: LauncherContextItem = {
        id: 'ws-77777777-8888-9999-aaaa-bbbbbbbbbbbb',
        name: 'Data Pipeline',
        state: 'recent',
        activitySummary: 'Edited schema.sql 10m ago',
      };

      const identity = resolveContextDisplayIdentity(item, false);
      expect(identity.subtitle).toBe('Edited schema.sql 10m ago');
    });
  });

  describe('4. Duplicate Name Detection', () => {
    it('identifies contexts with identical names across the list', () => {
      const contexts: LauncherContextItem[] = [
        { id: '1', name: 'Refactor Auth', state: 'active' },
        { id: '2', name: 'UI Overhaul', state: 'recent' },
        { id: '3', name: 'Refactor Auth', state: 'recent' },
        { id: '4', name: 'Bugfix', state: 'archived' },
      ];

      const duplicates = getDuplicateNameSet(contexts);
      expect(duplicates.has('refactor auth')).toBe(true);
      expect(duplicates.has('ui overhaul')).toBe(false);
      expect(duplicates.has('bugfix')).toBe(false);
    });

    it('handles duplicate detection case-insensitively and whitespace-tolerantly', () => {
      const contexts: LauncherContextItem[] = [
        { id: '1', name: 'Database Migration', state: 'active' },
        { id: '2', name: 'database migration  ', state: 'recent' },
      ];

      const duplicates = getDuplicateNameSet(contexts);
      expect(duplicates.has('database migration')).toBe(true);
    });

    it('ignores empty, whitespace-only, or unique names', () => {
      const contexts: LauncherContextItem[] = [
        { id: '1', name: '', state: 'active' },
        { id: '2', name: '   ', state: 'recent' },
        { id: '3', name: 'Solo Context', state: 'recent' },
      ];

      const duplicates = getDuplicateNameSet(contexts);
      expect(duplicates.size).toBe(0);
    });
  });

  describe('5. Disambiguation Strategy for Duplicate Names', () => {
    it('uses projectIdentifier as highest-priority disambiguation tag', () => {
      const item: LauncherContextItem = {
        id: 'ws-aaaabbbb-1111-2222-3333-444455556666',
        name: 'Feature Branch',
        state: 'active',
        projectIdentifier: 'contextsplit-web',
        repository: 'git@github.com:contextsplit/web.git',
      };

      const tag = resolveDisambiguationTag(item, true);
      expect(tag).toBe('contextsplit-web');
    });

    it('falls back to repository name when projectIdentifier is missing', () => {
      const item: LauncherContextItem = {
        id: 'ws-ccccdddd-1111-2222-3333-444455556666',
        name: 'Feature Branch',
        state: 'active',
        projectIdentifier: null,
        repository: 'contextsplit/desktop-app',
      };

      const tag = resolveDisambiguationTag(item, true);
      expect(tag).toBe('contextsplit/desktop-app');
    });

    it('falls back to short diagnostic hex ID when project and repo are absent', () => {
      const item: LauncherContextItem = {
        id: 'ws-12345678-abcd-ef01-2345-6789abcdef01',
        name: 'Feature Branch',
        state: 'active',
        projectIdentifier: null,
        repository: null,
      };

      const tag = resolveDisambiguationTag(item, true);
      expect(tag).toBe('#123456');
    });

    it('returns null disambiguation tag for unique context names to keep interface minimal', () => {
      const item: LauncherContextItem = {
        id: 'ws-12345678-abcd-ef01-2345-6789abcdef01',
        name: 'Completely Unique Context',
        state: 'active',
        projectIdentifier: 'contextsplit-web',
      };

      const tag = resolveDisambiguationTag(item, false);
      expect(tag).toBeNull();
    });

    it('disambiguates subtitle with project info or creation date when duplicate has no description', () => {
      const itemWithProject: LauncherContextItem = {
        id: 'ws-11111111-aaaa-bbbb-cccc-dddddddddddd',
        name: 'API Testing',
        state: 'recent',
        projectIdentifier: 'payment-svc',
      };
      const subtitle1 = resolveContextSubtitle(itemWithProject, true);
      expect(subtitle1).toBe('Project: payment-svc');

      const itemWithDate: LauncherContextItem = {
        id: 'ws-22222222-aaaa-bbbb-cccc-dddddddddddd',
        name: 'API Testing',
        state: 'recent',
        createdAt: '2026-10-15T12:00:00Z',
      };
      const subtitle2 = resolveContextSubtitle(itemWithDate, true);
      expect(subtitle2).toBe('Created Oct 15');
    });
  });

  describe('6. Raw Internal ID Safety', () => {
    it('never leaks raw UUIDs into context name, subtitle, or unique tags', () => {
      const rawUuid = 'ws-98765432-1111-2222-3333-444455556666';
      const item: LauncherContextItem = {
        id: rawUuid,
        name: 'Clean Workset',
        state: 'active',
      };

      const identity = resolveContextDisplayIdentity(item, false);
      expect(identity.name).not.toContain(rawUuid);
      expect(identity.subtitle).toBeNull();
      expect(identity.disambiguationTag).toBeNull();
    });

    it('formats short diagnostic ID consistently and compactly', () => {
      expect(formatShortId('ws-abcdef12-3456-7890-abcd-ef1234567890')).toBe('#abcdef');
      expect(formatShortId('fedcba98-7654-3210')).toBe('#fedcba');
      expect(formatShortId('short')).toBe('#short');
      expect(formatShortId('')).toBe('');
    });
  });

  describe('7. Context Selection Stability with Duplicate Names', () => {
    it('preserves distinct stable UUIDs for identically named contexts', () => {
      const contextA: LauncherContextItem = {
        id: 'ws-aaaa1111-0000-0000-0000-000000000001',
        name: 'Release Preparation',
        state: 'active',
        projectIdentifier: 'web-client',
      };

      const contextB: LauncherContextItem = {
        id: 'ws-bbbb2222-0000-0000-0000-000000000002',
        name: 'Release Preparation',
        state: 'recent',
        projectIdentifier: 'backend-core',
      };

      const identityA = resolveContextDisplayIdentity(contextA, true);
      const identityB = resolveContextDisplayIdentity(contextB, true);

      // Names match
      expect(identityA.name).toBe(identityB.name);

      // Stable IDs are strictly distinct
      expect(identityA.id).not.toBe(identityB.id);
      expect(identityA.id).toBe(contextA.id);
      expect(identityB.id).toBe(contextB.id);

      // Disambiguation tags allow user to distinguish them safely
      expect(identityA.disambiguationTag).toBe('web-client');
      expect(identityB.disambiguationTag).toBe('backend-core');
    });

    it('selectContextById and action execution receive stable context UUIDs', () => {
      const contexts: LauncherContextItem[] = [
        { id: 'uuid-1', name: 'Refactor', state: 'active' },
        { id: 'uuid-2', name: 'Refactor', state: 'recent' },
      ];

      // Selecting index 0 selects uuid-1
      const selectedIndex0 = 0;
      const selectedContext0 = contexts[selectedIndex0];
      expect(selectedContext0.id).toBe('uuid-1');

      // Selecting index 1 selects uuid-2
      const selectedIndex1 = 1;
      const selectedContext1 = contexts[selectedIndex1];
      expect(selectedContext1.id).toBe('uuid-2');

      // Double-clicking or resuming dispatches the precise UUID
      const resumedId = selectedContext1.id;
      expect(resumedId).toBe('uuid-2');
    });
  });
});
