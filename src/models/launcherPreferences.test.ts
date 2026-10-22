import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  DEFAULT_LAUNCHER_PREFERENCES,
  LAUNCHER_PREFERENCES_STORAGE_KEY,
  isValidSortOrder,
  parseLauncherPreferences,
  loadLauncherPreferences,
  saveLauncherPreferences,
  resetLauncherPreferences,
} from './launcherPreferences';
import { useLauncherStore } from '../store/launcherStore';

/**
 * Mock storage implementation for isolated test environments
 */
class MockStorage implements Storage {
  private store = new Map<string, string>();

  get length(): number {
    return this.store.size;
  }

  clear(): void {
    this.store.clear();
  }

  getItem(key: string): string | null {
    return this.store.has(key) ? this.store.get(key)! : null;
  }

  key(index: number): string | null {
    return Array.from(this.store.keys())[index] ?? null;
  }

  removeItem(key: string): void {
    this.store.delete(key);
  }

  setItem(key: string, value: string): void {
    this.store.set(key, value);
  }
}

describe('Launcher Preferences Persistence & Resilience', () => {
  let mockStorage: MockStorage;

  beforeEach(() => {
    mockStorage = new MockStorage();
    resetLauncherPreferences(mockStorage);
  });

  // =========================================================================
  // 1. Defaults & Parsing
  // =========================================================================
  describe('Sensible Defaults & Parsing', () => {
    it('provides sensible default preferences for existing and new users', () => {
      expect(DEFAULT_LAUNCHER_PREFERENCES.sortOrder).toBe('recent');
      const loaded = loadLauncherPreferences(mockStorage);
      expect(loaded).toEqual({ sortOrder: 'recent' });
    });

    it('validates sort order correctly', () => {
      expect(isValidSortOrder('recent')).toBe(true);
      expect(isValidSortOrder('name')).toBe(true);
      expect(isValidSortOrder('invalid')).toBe(false);
      expect(isValidSortOrder('')).toBe(false);
      expect(isValidSortOrder(123)).toBe(false);
      expect(isValidSortOrder(null)).toBe(false);
      expect(isValidSortOrder(undefined)).toBe(false);
    });

    it('parses valid raw preference objects', () => {
      const parsed = parseLauncherPreferences({ sortOrder: 'name' });
      expect(parsed).toEqual({ sortOrder: 'name' });
    });

    it('falls back to default for null, primitive, or array raw inputs', () => {
      expect(parseLauncherPreferences(null)).toEqual(DEFAULT_LAUNCHER_PREFERENCES);
      expect(parseLauncherPreferences(undefined)).toEqual(DEFAULT_LAUNCHER_PREFERENCES);
      expect(parseLauncherPreferences('recent')).toEqual(DEFAULT_LAUNCHER_PREFERENCES);
      expect(parseLauncherPreferences(42)).toEqual(DEFAULT_LAUNCHER_PREFERENCES);
      expect(parseLauncherPreferences(['recent'])).toEqual(DEFAULT_LAUNCHER_PREFERENCES);
    });
  });

  // =========================================================================
  // 2. Persistence & Roundtrip
  // =========================================================================
  describe('Persistence Roundtrip & Reset', () => {
    it('persists selected sort order to storage', () => {
      const saved = saveLauncherPreferences({ sortOrder: 'name' }, mockStorage);
      expect(saved).toBe(true);

      const loaded = loadLauncherPreferences(mockStorage);
      expect(loaded.sortOrder).toBe('name');

      const raw = mockStorage.getItem(LAUNCHER_PREFERENCES_STORAGE_KEY);
      expect(raw).toBe(JSON.stringify({ sortOrder: 'name' }));
    });

    it('resets preferences to default in storage', () => {
      saveLauncherPreferences({ sortOrder: 'name' }, mockStorage);
      expect(loadLauncherPreferences(mockStorage).sortOrder).toBe('name');

      resetLauncherPreferences(mockStorage);
      expect(mockStorage.getItem(LAUNCHER_PREFERENCES_STORAGE_KEY)).toBeNull();
      expect(loadLauncherPreferences(mockStorage).sortOrder).toBe('recent');
    });
  });

  // =========================================================================
  // 3. Surviving Launcher Open / Close & Application Restart
  // =========================================================================
  describe('Surviving Launcher Open / Close & Restart', () => {
    it('preserves user sort preference across openLauncher() and closeLauncher() cycles', () => {
      // Simulate user opening launcher, changing sort order to 'name'
      useLauncherStore.getState().openLauncher();
      useLauncherStore.getState().setActiveSort('name');
      expect(useLauncherStore.getState().activeSort).toBe('name');

      // User closes launcher
      useLauncherStore.getState().closeLauncher();
      expect(useLauncherStore.getState().isOpen).toBe(false);

      // User reopens launcher
      useLauncherStore.getState().openLauncher();
      expect(useLauncherStore.getState().isOpen).toBe(true);
      // activeSort must NOT be wiped back to 'recent'
      expect(useLauncherStore.getState().activeSort).toBe('name');
    });

    it('survives application restart simulation by rehydrating persisted sort preference', () => {
      // 1. User sets preference to 'name' in storage
      saveLauncherPreferences({ sortOrder: 'name' }, mockStorage);

      // 2. Rehydrate state from storage (simulating app boot)
      const preferences = loadLauncherPreferences(mockStorage);
      expect(preferences.sortOrder).toBe('name');

      // Clean up global localStorage
      resetLauncherPreferences(mockStorage);
    });
  });

  // =========================================================================
  // 4. Resilience: Corrupt, Outdated, or Invalid Values
  // =========================================================================
  describe('Corrupt, Outdated & Invalid Value Resilience', () => {
    it('recovers gracefully from malformed JSON string without throwing', () => {
      mockStorage.setItem(LAUNCHER_PREFERENCES_STORAGE_KEY, '{invalid json syntax:;;');

      const loaded = loadLauncherPreferences(mockStorage);
      expect(loaded).toEqual(DEFAULT_LAUNCHER_PREFERENCES);
    });

    it('recovers gracefully from outdated or unrecognized sort order values', () => {
      mockStorage.setItem(
        LAUNCHER_PREFERENCES_STORAGE_KEY,
        JSON.stringify({ sortOrder: 'unrecognized_sorting_strategy' })
      );

      const loaded = loadLauncherPreferences(mockStorage);
      expect(loaded.sortOrder).toBe('recent');
    });

    it('ignores unknown legacy or unexpected keys without failing', () => {
      mockStorage.setItem(
        LAUNCHER_PREFERENCES_STORAGE_KEY,
        JSON.stringify({
          sortOrder: 'name',
          deprecatedUiTheme: 'dark-glass',
          legacyFilterFlag: true,
        })
      );

      const loaded = loadLauncherPreferences(mockStorage);
      expect(loaded.sortOrder).toBe('name');
    });
  });

  // =========================================================================
  // 5. Fault Tolerance: Storage Errors & Data Protection
  // =========================================================================
  describe('Storage Fault Tolerance & Non-Corruption', () => {
    it('handles storage throwing on getItem without crashing', () => {
      const faultStorage = new MockStorage();
      faultStorage.getItem = vi.fn().mockImplementation(() => {
        throw new DOMException('SecurityError: Access Denied');
      });

      const loaded = loadLauncherPreferences(faultStorage);
      expect(loaded).toEqual(DEFAULT_LAUNCHER_PREFERENCES);
    });

    it('handles storage throwing on setItem (e.g. QuotaExceededError) safely', () => {
      const faultStorage = new MockStorage();
      faultStorage.setItem = vi.fn().mockImplementation(() => {
        throw new DOMException('QuotaExceededError');
      });

      const saved = saveLauncherPreferences({ sortOrder: 'name' }, faultStorage);
      expect(saved).toBe(false);
    });

    it('ensures preferences failure never corrupts context evidence or lifecycle state', () => {
      // Faulty storage fails to save
      const faultStorage = new MockStorage();
      faultStorage.setItem = vi.fn().mockImplementation(() => {
        throw new Error('Disk full');
      });

      saveLauncherPreferences({ sortOrder: 'name' }, faultStorage);

      // Launcher store state is still safe in-memory
      const storeState = useLauncherStore.getState();
      expect(storeState.activeFilter).toBe('all');
      expect(storeState.error).toBeNull();
    });
  });

  // =========================================================================
  // 6. Separation of Concerns & Avoiding Storing Derived Values
  // =========================================================================
  describe('Separation of Concerns & Derived Value Rule', () => {
    it('does not persist derived grouping structures or item counts', () => {
      saveLauncherPreferences({ sortOrder: 'name' }, mockStorage);
      const raw = mockStorage.getItem(LAUNCHER_PREFERENCES_STORAGE_KEY);
      const parsed = JSON.parse(raw!);

      // Only enduring user preferences exist in storage
      expect(Object.keys(parsed)).toEqual(['sortOrder']);
      // Derived grouping layouts, counts, search terms, selected indices are NOT persisted
      expect(parsed.groups).toBeUndefined();
      expect(parsed.totalCount).toBeUndefined();
      expect(parsed.searchQuery).toBeUndefined();
      expect(parsed.selectedIndex).toBeUndefined();
    });
  });
});
