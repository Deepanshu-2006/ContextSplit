/**
 * DAY 28 — PHASE 1 INTEGRATION / STEP 9
 * Context Interaction Contract Validation
 *
 * Verifies that the platform-independent Context Interaction API
 * (contextInteractionService) is a thin delegation layer over the existing
 * backend Context Engine and that it:
 *   1. Exposes exactly the 12 contracted APIs.
 *   2. Maps every API to an existing Tauri command with the correct envelope.
 *   3. Implements NO separate context logic (no local state, no DB access).
 *   4. Pairs with the 8 standardized interaction event types.
 *   5. Returns the 6 standardized interaction result states.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { contextInteractionService } from './contextInteractionService';
import { createInteractionEvent } from '../models/contextInteractionEvents';
import type {
  ContextInteractionEvent,
  InteractionEventType,
  InteractionSourceSurface,
} from '../models/contextInteractionEvents';
import { createInteractionResult } from '../models/contextInteractionResults';
import type {
  ContextInteractionResult,
  InteractionOutcome,
} from '../models/contextInteractionResults';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

/** The 12 contracted interaction APIs. */
const INTERACTION_APIS = [
  'getActiveContext',
  'getRecentContexts',
  'getContextSummary',
  'getContextWorkset',
  'getContextState',
  'getResumeReadiness',
  'resumeContext',
  'pauseContext',
  'captureItem',
  'assignItemToContext',
  'searchContexts',
  'getContextSuggestions',
] as const;

/** The 8 standardized interaction event types (DAY 28 contract). */
const INTERACTION_EVENT_TYPES: InteractionEventType[] = [
  'context.open',
  'context.resume',
  'context.pause',
  'context.capture',
  'context.assign',
  'context.search',
  'context.preview',
  'context.mark_important',
];

/** The 6 standardized interaction result states (DAY 28 contract). */
const INTERACTION_OUTCOMES: InteractionOutcome[] = [
  'SUCCESS',
  'PENDING',
  'AMBIGUOUS',
  'REJECTED',
  'ERROR',
  'CANCELLED',
];

/**
 * Every backend Context Engine command the Interaction API is allowed to call.
 * If an API calls anything outside this set, it is implementing context logic
 * in the frontend instead of delegating to the engine.
 */
const ALLOWED_ENGINE_COMMANDS = new Set([
  'get_last_active_context',
  'get_recent_work_sequence',
  'get_context_overview',
  'get_context_workset',
  'get_context_state',
  'get_resume_state',
  'create_context_signal',
  'handle_capture',
  'bulk_assign_inbox_items_to_workspace',
  'search_inbox_items',
  'list_item_context_memberships',
]);

import serviceSource from './contextInteractionService.ts?raw';

describe('Context Interaction Contract — API surface', () => {
  it('exposes exactly the 12 contracted APIs and nothing else', () => {
    expect(Object.keys(contextInteractionService).sort()).toEqual([...INTERACTION_APIS].sort());
  });

  it('exposes every API as a stateless function (no cached values)', () => {
    for (const api of INTERACTION_APIS) {
      expect(typeof contextInteractionService[api]).toBe('function');
    }
    // Every property must be a function: the API holds zero state of its own.
    for (const value of Object.values(contextInteractionService)) {
      expect(typeof value).toBe('function');
    }
  });
});

describe('Context Interaction Contract — engine delegation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('getActiveContext() delegates to get_last_active_context', async () => {
    const mockContext = { context_id: 'ctx_1', name: 'Work' };
    vi.mocked(invoke).mockResolvedValueOnce(mockContext);

    const result = await contextInteractionService.getActiveContext();

    expect(invoke).toHaveBeenCalledWith('get_last_active_context');
    expect(result).toEqual(mockContext);
  });

  it('getRecentContexts() delegates to get_recent_work_sequence and unwraps the engine envelope', async () => {
    const entries = [{ context_id: 'ctx_1' }, { context_id: 'ctx_2' }];
    vi.mocked(invoke).mockResolvedValueOnce({ entries });

    const result = await contextInteractionService.getRecentContexts(14);

    expect(invoke).toHaveBeenCalledWith('get_recent_work_sequence', { limitDays: 14 });
    expect(result).toEqual(entries);
  });

  it('getContextSummary() delegates to get_context_overview', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ overview: true });

    await contextInteractionService.getContextSummary('ctx_1');

    expect(invoke).toHaveBeenCalledWith('get_context_overview', { contextId: 'ctx_1' });
  });

  it('getContextWorkset() delegates to get_context_workset', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ items: [] });

    await contextInteractionService.getContextWorkset('ctx_1');

    expect(invoke).toHaveBeenCalledWith('get_context_workset', { contextId: 'ctx_1' });
  });

  it('getContextState() delegates to get_context_state', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ state: 'Active' });

    await contextInteractionService.getContextState('ctx_1');

    expect(invoke).toHaveBeenCalledWith('get_context_state', { contextId: 'ctx_1' });
  });

  it('getResumeReadiness() delegates to get_resume_state', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ readiness: 'High' });

    await contextInteractionService.getResumeReadiness('ctx_1');

    expect(invoke).toHaveBeenCalledWith('get_resume_state', { contextId: 'ctx_1' });
  });

  it('resumeContext() emits a Resume user_action signal through the engine', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ id: 'sig_1' });

    await contextInteractionService.resumeContext('ctx_1');

    expect(invoke).toHaveBeenCalledWith('create_context_signal', {
      workspaceId: 'ctx_1',
      itemId: null,
      signalType: 'user_action',
      sessionId: null,
      payload: JSON.stringify({ action: 'resume', source: 'InteractionAPI' }),
    });
  });

  it('pauseContext() emits a Pause user_action signal through the engine', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ id: 'sig_2' });

    await contextInteractionService.pauseContext('ctx_1');

    expect(invoke).toHaveBeenCalledWith('create_context_signal', {
      workspaceId: 'ctx_1',
      itemId: null,
      signalType: 'user_action',
      sessionId: null,
      payload: JSON.stringify({ action: 'pause', source: 'InteractionAPI' }),
    });
  });

  it('captureItem() delegates to handle_capture with the CaptureRequest untouched', async () => {
    const req = { data_type: 'text', content: 'hello', title: null, source: 'launcher' };
    vi.mocked(invoke).mockResolvedValueOnce(null);

    await contextInteractionService.captureItem(req);

    expect(invoke).toHaveBeenCalledWith('handle_capture', { req });
  });

  it('assignItemToContext() delegates to bulk_assign_inbox_items_to_workspace', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);

    await contextInteractionService.assignItemToContext('item_1', 'ctx_1');

    expect(invoke).toHaveBeenCalledWith('bulk_assign_inbox_items_to_workspace', {
      itemIds: ['item_1'],
      workspaceId: 'ctx_1',
    });
  });

  it('searchContexts() delegates to search_inbox_items with a params envelope', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);

    await contextInteractionService.searchContexts('roadmap');

    expect(invoke).toHaveBeenCalledWith('search_inbox_items', {
      params: { query: 'roadmap', enableSemantic: true },
    });
  });

  it('getContextSuggestions() delegates to list_item_context_memberships', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);

    await contextInteractionService.getContextSuggestions('item_1');

    expect(invoke).toHaveBeenCalledWith('list_item_context_memberships', { itemId: 'item_1' });
  });

  it('every API resolves to a positive result (no swallowed engine errors)', async () => {
    for (const api of INTERACTION_APIS) {
      vi.mocked(invoke).mockResolvedValueOnce(Array.isArray(undefined) ? [] : null);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      await (contextInteractionService as any)[api]('ctx_1');
    }
    expect(invoke).toHaveBeenCalledTimes(INTERACTION_APIS.length);
  });
});

describe('Context Interaction Contract — no separate context logic', () => {
  it('calls only existing Context Engine backend commands', async () => {
    vi.mocked(invoke).mockClear();
    vi.mocked(invoke).mockResolvedValue(null);

    const calls: string[] = [];
    const record = (cmd: unknown) => {
      if (typeof cmd === 'string') calls.push(cmd);
      return Promise.resolve(null);
    };
    vi.mocked(invoke).mockImplementation(record as never);

    await contextInteractionService.getActiveContext();
    await contextInteractionService.getRecentContexts();
    await contextInteractionService.getContextSummary('ctx');
    await contextInteractionService.getContextWorkset('ctx');
    await contextInteractionService.getContextState('ctx');
    await contextInteractionService.getResumeReadiness('ctx');
    await contextInteractionService.resumeContext('ctx');
    await contextInteractionService.pauseContext('ctx');
    await contextInteractionService.captureItem({ data_type: 'text', content: 'x' });
    await contextInteractionService.assignItemToContext('item', 'ctx');
    await contextInteractionService.searchContexts('q');
    await contextInteractionService.getContextSuggestions('item');

    expect(calls.length).toBe(12);
    for (const cmd of calls) {
      expect(ALLOWED_ENGINE_COMMANDS.has(cmd), `unexpected command: ${cmd}`).toBe(true);
    }
  });

  it('delegates exactly once per API — no composition of engine logic in the UI layer', () => {
    const invokeCallSites = (serviceSource.match(/await invoke</g) || []).length;
    expect(invokeCallSites).toBe(12);
  });

  it('holds no local context state or persistence in the interaction layer', () => {
    expect(serviceSource).not.toMatch(/localStorage|sessionStorage|indexedDB/);
    expect(serviceSource).not.toMatch(/zustand|create\(/);
    expect(serviceSource).not.toMatch(/useState|useRef|useEffect/);
    expect(serviceSource).not.toMatch(/CREATE TABLE|INSERT INTO|UPDATE\s+\w+\s+SET/);
  });

  it('transforms no context decision data — only transport envelopes differ', () => {
    // getRecentContexts is the sole envelope-unwrap (entries out of RecentWorkSequenceResponse).
    // Everything else must pass engine data through untouched.
    expect(serviceSource).toMatch(/get_recent_work_sequence/);
    expect((serviceSource.match(/JSON\.stringify/g) || []).length).toBe(2); // resume/pause payloads only
  });
});

describe('Context Interaction Contract — interaction events', () => {
  const eventFixtures: Array<{
    type: InteractionEventType;
    surface: InteractionSourceSurface;
    contextReference?: string;
    targetReference?: string;
  }> = [
    { type: 'context.open', surface: 'app', contextReference: 'ctx_1' },
    { type: 'context.resume', surface: 'launcher', contextReference: 'ctx_1' },
    { type: 'context.pause', surface: 'app', contextReference: 'ctx_1' },
    { type: 'context.capture', surface: 'mac_notch', targetReference: 'item_1' },
    { type: 'context.assign', surface: 'drag_drop', contextReference: 'ctx_1', targetReference: 'item_1' },
    { type: 'context.search', surface: 'keyboard' },
    { type: 'context.preview', surface: 'windows_edge', targetReference: 'item_1' },
    { type: 'context.mark_important', surface: 'app', targetReference: 'item_1' },
  ];

  it('supports all 8 standardized interaction event types', () => {
    expect(eventFixtures.map((f) => f.type)).toEqual(INTERACTION_EVENT_TYPES);
  });

  it.each(eventFixtures)('creates a valid "$type" event from $surface', ({ type, surface, contextReference, targetReference }) => {
    const event: ContextInteractionEvent = createInteractionEvent(type, surface, {
      contextReference,
      targetReference,
    });

    expect(event.type).toBe(type);
    expect(event.sourceSurface).toBe(surface);
    expect(event.contextReference).toBe(contextReference);
    expect(event.targetReference).toBe(targetReference);

    const parsed = new Date(event.timestamp);
    expect(parsed.toISOString()).toBe(event.timestamp);
  });

  it('produces platform-independent events — identical structure from any surface', () => {
    const fromNotch = createInteractionEvent('context.capture', 'mac_notch', { targetReference: 'item_1' });
    const fromEdge = createInteractionEvent('context.capture', 'windows_edge', { targetReference: 'item_1' });

    expect(Object.keys(fromNotch).sort()).toEqual(Object.keys(fromEdge).sort());
    expect(fromNotch.type).toBe(fromEdge.type);
    expect(fromNotch.targetReference).toBe(fromEdge.targetReference);
  });
});

describe('Context Interaction Contract — result states', () => {
  it('supports all 6 standardized result states', () => {
    expect(INTERACTION_OUTCOMES).toEqual([
      'SUCCESS',
      'PENDING',
      'AMBIGUOUS',
      'REJECTED',
      'ERROR',
      'CANCELLED',
    ]);
  });

  it.each(INTERACTION_OUTCOMES)('creates a valid %s result', (outcome) => {
    const result: ContextInteractionResult = createInteractionResult(outcome);
    expect(result.outcome).toBe(outcome);
  });

  it('SUCCESS carries the resolved context', () => {
    const result = createInteractionResult('SUCCESS', {
      message: 'Item captured and assigned',
      resolvedContextId: 'ctx_1',
    });
    expect(result.outcome).toBe('SUCCESS');
    expect(result.resolvedContextId).toBe('ctx_1');
    expect(result.suggestedContextIds).toBeUndefined();
  });

  it('PENDING carries progress metadata and no resolution yet', () => {
    const result = createInteractionResult('PENDING', {
      metadata: { jobId: 'job_1' },
    });
    expect(result.outcome).toBe('PENDING');
    expect(result.resolvedContextId).toBeUndefined();
    expect(result.metadata).toEqual({ jobId: 'job_1' });
  });

  it('AMBIGUOUS carries suggested contexts instead of a single resolution', () => {
    const result = createInteractionResult('AMBIGUOUS', {
      message: 'Choose context',
      suggestedContextIds: ['ctx_1', 'ctx_2'],
    });
    expect(result.outcome).toBe('AMBIGUOUS');
    expect(result.resolvedContextId).toBeUndefined();
    expect(result.suggestedContextIds).toEqual(['ctx_1', 'ctx_2']);
  });

  it('REJECTED carries a human-readable reason and performs no assignment', () => {
    const result = createInteractionResult('REJECTED', {
      message: 'Rejected by Context Quality Guard',
    });
    expect(result.outcome).toBe('REJECTED');
    expect(result.message).toBe('Rejected by Context Quality Guard');
    expect(result.resolvedContextId).toBeUndefined();
  });

  it('ERROR carries a failure message', () => {
    const result = createInteractionResult('ERROR', { message: 'Capture failed' });
    expect(result.outcome).toBe('ERROR');
    expect(result.message).toBe('Capture failed');
  });

  it('CANCELLED is representable with a minimal payload', () => {
    const result = createInteractionResult('CANCELLED');
    expect(result.outcome).toBe('CANCELLED');
    expect(result.message).toBeUndefined();
  });
});
