import { describe, it, expect } from 'vitest';
import {
  CONTEXT_ORGANIZATION_REGISTRY,
  ContextOrganizationActionId,
  getConceptDomain,
  verifyConceptualBoundary,
  CONTEXT_IDENTITY_INVARIANTS,
  assertContextIdentityPreserved,
  validateOrganizationActionTransition,
  validateContextName,
  validateOrganizationAction,
  getBackendOperationMapping,
} from './contextOrganization';

describe('Context Organization Contract', () => {
  describe('1. Genuine Backend Support & Gap Identification', () => {
    it('accurately identifies supported backend operations', () => {
      const supportedActions: ContextOrganizationActionId[] = [
        'context.rename',
        'context.archive',
        'context.unarchive',
        'context.delete',
      ];

      for (const actionId of supportedActions) {
        const descriptor = CONTEXT_ORGANIZATION_REGISTRY[actionId];
        expect(descriptor).toBeDefined();
        expect(descriptor.isSupported).toBe(true);
        expect(descriptor.supportStatus).toBe('SUPPORTED');
        expect(descriptor.underlyingOperation).toBeDefined();
      }
    });

    it('explicitly identifies unsupported gap operations with documented reasons', () => {
      const unsupportedGapActions: ContextOrganizationActionId[] = [
        'context.favorite',
        'context.unfavorite',
        'context.set_metadata',
      ];

      for (const actionId of unsupportedGapActions) {
        const descriptor = CONTEXT_ORGANIZATION_REGISTRY[actionId];
        expect(descriptor).toBeDefined();
        expect(descriptor.isSupported).toBe(false);
        expect(descriptor.supportStatus).toMatch(/^UNSUPPORTED/);
        expect(descriptor.unsupportedReason).toBeDefined();
        expect(descriptor.unsupportedReason!.length).toBeGreaterThan(10);
      }
    });
  });

  describe('2. Conceptual Domain Separation (Organization vs Lifecycle vs Membership)', () => {
    it('classifies concepts into their respective domain scopes', () => {
      expect(getConceptDomain('rename')).toBe('organization');
      expect(getConceptDomain('pin')).toBe('organization');
      expect(getConceptDomain('favorite')).toBe('organization');
      expect(getConceptDomain('project_identifier')).toBe('organization');

      expect(getConceptDomain('active')).toBe('lifecycle');
      expect(getConceptDomain('paused')).toBe('lifecycle');
      expect(getConceptDomain('dormant')).toBe('lifecycle');
      expect(getConceptDomain('archived')).toBe('lifecycle');
      expect(getConceptDomain('decay')).toBe('lifecycle');

      expect(getConceptDomain('item')).toBe('membership');
      expect(getConceptDomain('membership')).toBe('membership');
      expect(getConceptDomain('important')).toBe('membership');
      expect(getConceptDomain('workset')).toBe('membership');
    });

    it('enforces non-interchangeability between pinning and item memory importance', () => {
      const boundary = verifyConceptualBoundary('pinning', 'importance');
      expect(boundary.areDistinct).toBe(true);
      expect(boundary.explanation).toContain('Contexts');
      expect(boundary.explanation).toContain('Items');
    });

    it('enforces non-interchangeability between pinning and lifecycle state', () => {
      const boundary = verifyConceptualBoundary('pinning', 'lifecycle');
      expect(boundary.areDistinct).toBe(true);
      expect(boundary.explanation).toContain('runtime state');
    });

    it('enforces non-interchangeability between item importance and context lifecycle', () => {
      const boundary = verifyConceptualBoundary('importance', 'lifecycle');
      expect(boundary.areDistinct).toBe(true);
    });

    it('enforces non-interchangeability between workspace membership and organization', () => {
      const boundary = verifyConceptualBoundary('membership', 'pinning');
      expect(boundary.areDistinct).toBe(true);
    });
  });

  describe('3. Identity Preservation Invariants', () => {
    it('confirms identity is preserved when ID remains unchanged', () => {
      const before = { id: 'ws-auth-123', name: 'Auth Flow' };
      const after = { id: 'ws-auth-123', name: 'Authentication Refactor' };

      const result = assertContextIdentityPreserved(before, after);
      expect(result.isPreserved).toBe(true);
      expect(result.violation).toBeUndefined();
    });

    it('rejects identity mutation if context ID changes', () => {
      const before = { id: 'ws-auth-123', name: 'Auth Flow' };
      const after = { id: 'ws-auth-999', name: 'Auth Flow' };

      const result = assertContextIdentityPreserved(before, after);
      expect(result.isPreserved).toBe(false);
      expect(result.violation).toContain('Identity mutation detected');
    });

    it('rejects invalid or empty context IDs', () => {
      const before = { id: '   ', name: 'Auth Flow' };
      const after = { id: 'ws-auth-123', name: 'Auth Flow' };

      const result = assertContextIdentityPreserved(before, after);
      expect(result.isPreserved).toBe(false);
      expect(result.violation).toContain('Initial context ID is invalid');
    });

    it('declares formal invariants constants', () => {
      expect(CONTEXT_IDENTITY_INVARIANTS.IMMUTABLE_PRIMARY_KEY).toBeDefined();
      expect(CONTEXT_IDENTITY_INVARIANTS.MEMBERSHIP_PRESERVATION).toBeDefined();
      expect(CONTEXT_IDENTITY_INVARIANTS.ACTIVE_SESSION_CONTINUITY).toBeDefined();
      expect(CONTEXT_IDENTITY_INVARIANTS.RESTRICTED_ACTIVE_ARCHIVAL).toBeDefined();
    });
  });

  describe('4. Context Name Validation & Sanitization', () => {
    it('accepts and trims valid names', () => {
      const result = validateContextName('  Payments V2  ');
      expect(result.isValid).toBe(true);
      expect(result.sanitizedName).toBe('Payments V2');
      expect(result.error).toBeUndefined();
    });

    it('rejects empty or whitespace-only names', () => {
      expect(validateContextName('').isValid).toBe(false);
      expect(validateContextName('    ').isValid).toBe(false);
      expect(validateContextName('   \t\n  ').isValid).toBe(false);
    });

    it('rejects non-string names', () => {
      expect(validateContextName(null).isValid).toBe(false);
      expect(validateContextName(undefined).isValid).toBe(false);
      expect(validateContextName(123).isValid).toBe(false);
      expect(validateContextName({}).isValid).toBe(false);
    });

    it('enforces maximum length constraint (100 characters)', () => {
      const longName = 'A'.repeat(101);
      const result = validateContextName(longName);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('cannot exceed 100 characters');

      const maxValidName = 'A'.repeat(100);
      expect(validateContextName(maxValidName).isValid).toBe(true);
    });

    it('rejects newlines and unprintable ASCII control characters', () => {
      expect(validateContextName('Context\nName').isValid).toBe(false);
      expect(validateContextName('Context\rName').isValid).toBe(false);
      expect(validateContextName('Context\x00Name').isValid).toBe(false);
      expect(validateContextName('Context\x1FName').isValid).toBe(false);
    });
  });

  describe('5. State-Aware Action Availability Matrix', () => {
    describe('Active Context State', () => {
      it('allows renaming active context', () => {
        const val = validateOrganizationActionTransition('context.rename', 'active', true);
        expect(val.isValid).toBe(true);
        expect(val.outcome).toBe('ALLOWED');
      });

      it('blocks archiving active context without pausing/switching first', () => {
        const val = validateOrganizationActionTransition('context.archive', 'active', true);
        expect(val.isValid).toBe(false);
        expect(val.outcome).toBe('BLOCKED');
        expect(val.reason).toContain('Cannot archive the currently active workspace');
      });

      it('blocks unarchiving active context', () => {
        const val = validateOrganizationActionTransition('context.unarchive', 'active', true);
        expect(val.isValid).toBe(false);
        expect(val.outcome).toBe('BLOCKED');
      });

      it('blocks deleting active context', () => {
        const val = validateOrganizationActionTransition('context.delete', 'active', true);
        expect(val.isValid).toBe(false);
        expect(val.outcome).toBe('BLOCKED');
        expect(val.reason).toContain('Cannot delete the currently active workspace');
      });
    });

    describe('Paused & Dormant Context States', () => {
      it('allows archiving paused context', () => {
        const val = validateOrganizationActionTransition('context.archive', 'paused', false);
        expect(val.isValid).toBe(true);
        expect(val.outcome).toBe('ALLOWED');
      });

      it('allows archiving dormant context', () => {
        const val = validateOrganizationActionTransition('context.archive', 'dormant', false);
        expect(val.isValid).toBe(true);
        expect(val.outcome).toBe('ALLOWED');
      });

      it('blocks unarchiving dormant or paused context', () => {
        const val = validateOrganizationActionTransition('context.unarchive', 'dormant', false);
        expect(val.isValid).toBe(false);
        expect(val.outcome).toBe('BLOCKED');
      });

      it('allows renaming and deleting dormant context', () => {
        expect(validateOrganizationActionTransition('context.rename', 'dormant', false).isValid).toBe(true);
        expect(validateOrganizationActionTransition('context.delete', 'dormant', false).isValid).toBe(true);
      });
    });

    describe('Archived Context State', () => {
      it('blocks archiving an already-archived context', () => {
        const val = validateOrganizationActionTransition('context.archive', 'archived', false);
        expect(val.isValid).toBe(false);
        expect(val.outcome).toBe('BLOCKED');
        expect(val.reason).toContain('already archived');
      });

      it('allows unarchiving an archived context', () => {
        const val = validateOrganizationActionTransition('context.unarchive', 'archived', false);
        expect(val.isValid).toBe(true);
        expect(val.outcome).toBe('ALLOWED');
      });

      it('allows renaming and deleting an archived context', () => {
        expect(validateOrganizationActionTransition('context.rename', 'archived', false).isValid).toBe(true);
        expect(validateOrganizationActionTransition('context.delete', 'archived', false).isValid).toBe(true);
      });
    });

    describe('Unsupported Operations Guard', () => {
      it('consistently returns UNSUPPORTED for unpersisted favoriting/metadata operations', () => {
        const gapActions: ContextOrganizationActionId[] = [
          'context.favorite',
          'context.unfavorite',
          'context.set_metadata',
        ];

        for (const actionId of gapActions) {
          const val = validateOrganizationActionTransition(actionId, 'dormant', false);
          expect(val.isValid).toBe(false);
          expect(val.outcome).toBe('UNSUPPORTED');
          expect(val.reason).toBeDefined();
        }
      });
    });
  });

  describe('6. Comprehensive Action Validation (validateOrganizationAction)', () => {
    it('validates a valid rename operation with input sanitization', () => {
      const res = validateOrganizationAction({
        actionId: 'context.rename',
        contextId: 'ws-target-123',
        currentState: 'dormant',
        targetName: '  New Clean Name  ',
      });

      expect(res.isValid).toBe(true);
      expect(res.outcome).toBe('ALLOWED');
      expect(res.sanitizedValue).toBe('New Clean Name');
    });

    it('rejects rename with empty or invalid context name', () => {
      const res = validateOrganizationAction({
        actionId: 'context.rename',
        contextId: 'ws-target-123',
        currentState: 'dormant',
        targetName: '   ',
      });

      expect(res.isValid).toBe(false);
      expect(res.outcome).toBe('INVALID_INPUT');
      expect(res.errorCode).toBe('INVALID_NAME');
    });

    it('rejects action with invalid context ID', () => {
      const res = validateOrganizationAction({
        actionId: 'context.archive',
        contextId: '',
        currentState: 'dormant',
      });

      expect(res.isValid).toBe(false);
      expect(res.outcome).toBe('INVALID_INPUT');
      expect(res.errorCode).toBe('EMPTY_CONTEXT_ID');
    });

    it('rejects unsupported actions with OPERATION_UNSUPPORTED error code', () => {
      const res = validateOrganizationAction({
        actionId: 'context.favorite',
        contextId: 'ws-target-123',
        currentState: 'dormant',
      });

      expect(res.isValid).toBe(false);
      expect(res.outcome).toBe('UNSUPPORTED');
      expect(res.errorCode).toBe('OPERATION_UNSUPPORTED');
    });
  });

  describe('7. Backend Operation Mapping & Safety', () => {
    it('maps rename to db.updateWorkspace', () => {
      const mapping = getBackendOperationMapping('context.rename');
      expect(mapping.supported).toBe(true);
      expect(mapping.serviceMethod).toContain('updateWorkspace');
      expect(mapping.isSafe).toBe(true);
      expect(mapping.requiresConfirmation).toBe(false);
    });

    it('maps archive to db.archiveWorkspace', () => {
      const mapping = getBackendOperationMapping('context.archive');
      expect(mapping.supported).toBe(true);
      expect(mapping.serviceMethod).toContain('archiveWorkspace');
      expect(mapping.isSafe).toBe(true);
      expect(mapping.requiresConfirmation).toBe(false);
    });

    it('maps unarchive to db.unarchiveWorkspace', () => {
      const mapping = getBackendOperationMapping('context.unarchive');
      expect(mapping.supported).toBe(true);
      expect(mapping.serviceMethod).toContain('unarchiveWorkspace');
      expect(mapping.isSafe).toBe(true);
    });

    it('maps delete to db.deleteWorkspace with safety confirmation flag', () => {
      const mapping = getBackendOperationMapping('context.delete');
      expect(mapping.supported).toBe(true);
      expect(mapping.serviceMethod).toContain('deleteWorkspace');
      expect(mapping.isSafe).toBe(false);
      expect(mapping.requiresConfirmation).toBe(true);
    });

    it('marks unmigrated favoriting and metadata as unsupported', () => {
      expect(getBackendOperationMapping('context.pin').supported).toBe(true);
      expect(getBackendOperationMapping('context.favorite').supported).toBe(false);
      expect(getBackendOperationMapping('context.set_metadata').supported).toBe(false);
    });
  });
});
