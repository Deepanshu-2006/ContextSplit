/**
 * Context Identification & Disambiguation Contract
 *
 * Provides deterministic logic for clearly identifying, labeling, and
 * disambiguating contexts (workspaces) in ContextSplit.
 *
 * Invariants:
 * 1. Grounded strictly in existing data — no AI hallucinations or fabricated descriptions.
 * 2. Raw internal IDs are hidden for unique contexts and exposed only for disambiguation/diagnostics.
 * 3. Identical names are disambiguated using projects, repositories, timestamps, or compact short IDs.
 * 4. Stable UUID context IDs are strictly preserved for all selection and action dispatches.
 */

import { LauncherContextItem } from '../store/launcherStore';

/**
 * Resolved display identity for a context in UI surfaces.
 */
export interface ContextDisplayIdentity {
  /** Stable context UUID for actions and selection */
  id: string;
  /** The authentic, non-fabricated context name */
  name: string;
  /** Whether another context in the current scope shares this name */
  isDuplicate: boolean;
  /** Compact distinguishing tag (project, repo, or #shortId) when duplicate; null when unique */
  disambiguationTag: string | null;
  /** Real description, task, or distinguishing timestamp; null if no real data exists */
  subtitle: string | null;
  /** Gentle human-readable relative time */
  timeStr: string;
  /** Short 6-character hex identifier for diagnostic/disambiguation purposes */
  shortId: string;
}

/**
 * Builds a Set of normalized context names that appear more than once in the list.
 */
export function getDuplicateNameSet(
  contexts: Array<{ id: string; name: string }>
): Set<string> {
  const counts = new Map<string, number>();
  for (const ctx of contexts) {
    if (!ctx.name) continue;
    const key = ctx.name.trim().toLowerCase();
    counts.set(key, (counts.get(key) || 0) + 1);
  }

  const duplicates = new Set<string>();
  for (const [key, count] of counts.entries()) {
    if (count > 1) {
      duplicates.add(key);
    }
  }
  return duplicates;
}

/**
 * Formats a short, deterministic diagnostic identifier from a UUID (e.g. "#a1b2c3").
 */
export function formatShortId(id: string): string {
  if (!id || typeof id !== 'string') return '';
  const clean = id.trim().replace(/^ws-/, '');
  return `#${clean.slice(0, 6)}`;
}

/**
 * Gentle relative time formatter without noisy raw timestamps.
 * Handles ISO strings and fallback duration estimates safely.
 */
export function formatGentleTime(
  isoString?: string | null,
  durationEstimate?: string | null
): string {
  if (durationEstimate) return durationEstimate;
  if (!isoString) return 'Ready to resume';

  try {
    const timestamp = new Date(isoString).getTime();
    if (isNaN(timestamp)) return 'Recently active';
    const now = Date.now();
    const diffMs = Math.max(0, now - timestamp);
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHour = Math.floor(diffMin / 60);
    const diffDays = Math.floor(diffHour / 24);

    if (diffMin < 2) return 'Active just now';
    if (diffMin < 60) return `Active ${diffMin}m ago`;
    if (diffHour < 24) return `Active ${diffHour}h ago`;
    if (diffDays === 1) return 'Active yesterday';
    if (diffDays < 7) return `Active ${diffDays}d ago`;
    return 'Active recently';
  } catch {
    return 'Recently active';
  }
}

/**
 * Formats a calendar-friendly date stamp (e.g. "Oct 19" or "Oct 19, 2026")
 * for distinguishing contexts when relative times are too similar.
 */
export function formatFriendlyDate(isoString?: string | null): string | null {
  if (!isoString) return null;
  try {
    const date = new Date(isoString);
    if (isNaN(date.getTime())) return null;
    return date.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return null;
  }
}

/**
 * Selects the best genuine, non-fabricated subtitle for a context.
 * Never invents placeholder strings if data is genuinely absent.
 */
export function resolveContextSubtitle(
  context: LauncherContextItem,
  isDuplicate: boolean
): string | null {
  // 1. Highest priority: real active task title
  if (context.taskTitle && context.taskTitle.trim().length > 0) {
    return context.taskTitle.trim();
  }

  // 2. Second priority: explicit user description (if present)
  if (context.description && context.description.trim().length > 0) {
    return context.description.trim();
  }

  // 3. Third priority: real recent activity summary
  const activity = (context.recentActivitySummary || (context as any).activitySummary || '').trim();
  if (activity.length > 0) {
    return activity;
  }

  // 4. Disambiguation fallback for duplicate names:
  // If two contexts share a name and lack tasks/descriptions, show created or project info
  if (isDuplicate) {
    if (context.projectIdentifier && context.projectIdentifier.trim().length > 0) {
      return `Project: ${context.projectIdentifier.trim()}`;
    }
    if (context.repository && context.repository.trim().length > 0) {
      return `Repo: ${context.repository.trim()}`;
    }
    const createdStr = formatFriendlyDate(context.createdAt || context.lastActiveAt);
    if (createdStr) {
      return `Created ${createdStr}`;
    }
  }

  // If no genuine distinguishing metadata exists, return null (DO NOT fabricate)
  return null;
}

/**
 * Resolves a compact tag to display next to a context name when duplicate names exist.
 * Returns null if the context name is unique.
 */
export function resolveDisambiguationTag(
  context: LauncherContextItem,
  isDuplicate: boolean
): string | null {
  if (!isDuplicate) return null;

  // Prefer project identifier if available
  if (context.projectIdentifier && context.projectIdentifier.trim().length > 0) {
    return context.projectIdentifier.trim();
  }

  // Fallback to repository name if available
  if (context.repository && context.repository.trim().length > 0) {
    return context.repository.trim();
  }

  // Final deterministic fallback: short ID tag (e.g. "#a1b2c3")
  return formatShortId(context.id);
}

/**
 * Core identification resolver: produces complete display identity for a context item.
 */
export function resolveContextDisplayIdentity(
  context: LauncherContextItem,
  isDuplicate: boolean
): ContextDisplayIdentity {
  const name = (context.name || 'Unnamed Context').trim();
  const shortId = formatShortId(context.id);
  const disambiguationTag = resolveDisambiguationTag(context, isDuplicate);
  const subtitle = resolveContextSubtitle(context, isDuplicate);
  const timeStr = formatGentleTime(context.lastActiveAt, context.durationEstimate);

  return {
    id: context.id,
    name,
    isDuplicate,
    disambiguationTag,
    subtitle,
    timeStr,
    shortId,
  };
}
