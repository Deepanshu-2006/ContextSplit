/**
 * User Preferences for Context Organization & Launcher Presentation
 *
 * Requirements:
 * 1. Persist only preferences that should survive closing and reopening the launcher (e.g. sortOrder).
 * 2. Reuse standard local-first storage infrastructure.
 * 3. Keep user preferences separate from Context Engine evidence and lifecycle state.
 * 4. Avoid storing derived values that can be calculated reliably (e.g. grouping layout).
 * 5. Handle missing, outdated, or invalid preference values safely with sensible defaults.
 * 6. Ensure persistence failures never corrupt context data.
 */

export type ContextSortOrder = 'recent' | 'name';

export interface LauncherPreferences {
  sortOrder: ContextSortOrder;
}

export const DEFAULT_LAUNCHER_PREFERENCES: Readonly<LauncherPreferences> = Object.freeze({
  sortOrder: 'recent',
});

export const LAUNCHER_PREFERENCES_STORAGE_KEY = 'contextsplit:launcher:preferences';

/**
 * Validates whether a given value is a supported ContextSortOrder.
 */
export function isValidSortOrder(val: unknown): val is ContextSortOrder {
  return val === 'recent' || val === 'name';
}

/**
 * Safely parses raw stored preferences, applying validation and falling back
 * to default values for missing, outdated, or malformed properties.
 */
export function parseLauncherPreferences(raw: unknown): LauncherPreferences {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    return { ...DEFAULT_LAUNCHER_PREFERENCES };
  }

  const candidate = raw as Record<string, unknown>;

  const sortOrder: ContextSortOrder = isValidSortOrder(candidate.sortOrder)
    ? candidate.sortOrder
    : DEFAULT_LAUNCHER_PREFERENCES.sortOrder;

  return {
    sortOrder,
  };
}

/**
 * Helper to obtain the active browser/runtime storage safely.
 * Returns null if window, localStorage, or storage access is unavailable.
 */
function getSafeStorage(customStorage?: Storage): Storage | null {
  if (customStorage) return customStorage;

  try {
    if (typeof window !== 'undefined' && window.localStorage) {
      return window.localStorage;
    }
  } catch {
    // Storage access may be blocked by environment security or private browsing
  }

  return null;
}

/**
 * Loads persisted launcher preferences from storage.
 * Handles missing keys, corrupt JSON, and invalid values gracefully by returning
 * sensible defaults without throwing.
 */
export function loadLauncherPreferences(storage?: Storage): LauncherPreferences {
  const store = getSafeStorage(storage);
  if (!store) {
    return { ...DEFAULT_LAUNCHER_PREFERENCES };
  }

  try {
    const serialized = store.getItem(LAUNCHER_PREFERENCES_STORAGE_KEY);
    if (!serialized) {
      return { ...DEFAULT_LAUNCHER_PREFERENCES };
    }

    const parsed = JSON.parse(serialized);
    return parseLauncherPreferences(parsed);
  } catch {
    // If parsing fails or storage throws, safely fall back to sensible defaults
    return { ...DEFAULT_LAUNCHER_PREFERENCES };
  }
}

/**
 * Persists updated launcher preferences to storage.
 * Merges updates into current preferences and validates values.
 * Catches any storage errors (quota exceeded, permissions) and returns false
 * without throwing or corrupting context data.
 */
export function saveLauncherPreferences(
  updates: Partial<LauncherPreferences>,
  storage?: Storage
): boolean {
  const store = getSafeStorage(storage);
  if (!store) {
    return false;
  }

  try {
    const current = loadLauncherPreferences(store);
    const merged: LauncherPreferences = {
      sortOrder: isValidSortOrder(updates.sortOrder) ? updates.sortOrder : current.sortOrder,
    };

    store.setItem(LAUNCHER_PREFERENCES_STORAGE_KEY, JSON.stringify(merged));
    return true;
  } catch (err) {
    console.warn('[LauncherPreferences] Failed to persist preferences:', err);
    return false;
  }
}

/**
 * Resets launcher preferences to default in storage.
 */
export function resetLauncherPreferences(storage?: Storage): boolean {
  const store = getSafeStorage(storage);
  if (!store) {
    return false;
  }

  try {
    store.removeItem(LAUNCHER_PREFERENCES_STORAGE_KEY);
    return true;
  } catch {
    return false;
  }
}
