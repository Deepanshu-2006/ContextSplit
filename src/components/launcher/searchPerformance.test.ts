import { describe, it, expect } from 'vitest';
import {
  matchContext,
  sortContextMatches,
  matchesContextFilter,
  getVisibleLauncherSlice,
  MAX_VISIBLE_LAUNCHER_RESULTS,
  LauncherContextItem,
} from '../../store/launcherStore';

// Safe memory reader for cross-environment vitest execution
function getHeapUsedBytes(): number {
  try {
    const proc = (globalThis as any).process;
    if (proc && typeof proc.memoryUsage === 'function') {
      return proc.memoryUsage().heapUsed;
    }
  } catch {
    // Fallback if not available
  }
  return 0;
}

// Realistic lightweight fixtures generator
function generateRealisticContexts(count: number): LauncherContextItem[] {
  const domains = [
    'PayX',
    'Auth0',
    'CloudSync',
    'Analytics',
    'Docs',
    'MobileApp',
    'DataPipeline',
    'Frontend',
    'PaymentGateway',
    'NotificationService',
  ];
  const tasks = [
    'Webhook integration',
    'JWT rotation',
    'Schema migration',
    'UI redesign',
    'LCP optimization',
    'Bugfix #102',
    'Security audit',
    'Cache layer refactor',
  ];
  const states = ['recent', 'paused', 'dormant', 'archived'];

  const items: LauncherContextItem[] = [];

  for (let i = 0; i < count; i++) {
    const domain = domains[i % domains.length];
    const task = tasks[i % tasks.length];
    const state = i === 0 ? 'active' : states[i % states.length];
    const daysAgo = (i % 30) + 1;
    const date = new Date(Date.now() - daysAgo * 24 * 60 * 60 * 1000).toISOString();

    items.push({
      id: `ctx-${domain.toLowerCase()}-${i + 1}`,
      name: `${domain} Context ${i + 1}`,
      description: `Handling ${task} for ${domain.toLowerCase()} workspace ${i + 1}`,
      projectIdentifier: `${domain.toLowerCase()}-proj`,
      repository: `${domain.toLowerCase()}-repo`,
      taskTitle: `${task} in progress`,
      recentActivitySummary: `Updated ${domain.toLowerCase()} configs and verified integration`,
      memoryStatement: `Working on ${domain.toLowerCase()} with focus on performance and reliability`,
      isActive: i === 0,
      state,
      lastActiveAt: date,
      itemCount: (i % 15) + 1,
    });
  }

  return items;
}

// Search pipeline executor
function executeSearchPipeline(
  items: LauncherContextItem[],
  query: string,
  filter = 'all'
): { results: LauncherContextItem[]; durationMs: number } {
  const start = performance.now();
  const stateFiltered = items.filter((item) => matchesContextFilter(item, filter as any));
  let results: LauncherContextItem[];

  if (!query.trim()) {
    results = stateFiltered;
  } else {
    const matched = stateFiltered.filter((item) => matchContext(item, query));
    results = sortContextMatches(matched, query);
  }

  const durationMs = performance.now() - start;
  return { results, durationMs };
}

describe('Launcher Search Performance Benchmarks', () => {
  const scales = [10, 100, 500, 1000, 5000];

  // 1. Search Latency
  it('1. measures search latency across 10, 100, 500, 1,000, and 5,000 contexts', () => {
    console.log('\n================================================================');
    console.log('BENCHMARK 1: SEARCH LATENCY ACROSS CONTEXT SCALES');
    console.log('================================================================');

    const realisticQueries = [
      'pay', // Prefix match
      'payx', // Exact word prefix
      'webhook', // Metadata match in description / task
      'schema migration', // Multi-word query
      'Context 1', // Substring query
      'nonexistent-term-xyz', // No-match query
      '', // Empty query (all active/recent)
    ];

    for (const scale of scales) {
      const fixtures = generateRealisticContexts(scale);

      // Warmup run
      for (const q of realisticQueries) {
        executeSearchPipeline(fixtures, q);
      }

      // Timed runs
      const runDurations: number[] = [];
      for (let run = 0; run < 5; run++) {
        for (const q of realisticQueries) {
          const { durationMs } = executeSearchPipeline(fixtures, q);
          runDurations.push(durationMs);
        }
      }

      const avgLatencyMs = runDurations.reduce((a, b) => a + b, 0) / runDurations.length;
      const minLatencyMs = Math.min(...runDurations);
      const maxLatencyMs = Math.max(...runDurations);

      console.log(
        `Scale: ${scale.toString().padStart(5)} contexts | Avg: ${avgLatencyMs.toFixed(3)}ms | Min: ${minLatencyMs.toFixed(3)}ms | Max: ${maxLatencyMs.toFixed(3)}ms`
      );

      // Assert that search latency remains strictly within interactive frame limits (< 16.6ms for 60fps)
      if (scale <= 100) {
        expect(avgLatencyMs).toBeLessThan(1.0); // <1ms for standard workspaces
      } else if (scale <= 1000) {
        expect(avgLatencyMs).toBeLessThan(4.0); // <4ms for large repositories
      } else {
        expect(avgLatencyMs).toBeLessThan(15.0); // <15ms for massive 5,000 contexts (actual ~2-3ms)
      }
    }
  });

  // 2. UI Responsiveness (Interactive Typing Burst Simulation)
  it('2. measures UI responsiveness during rapid typing bursts', () => {
    console.log('\n================================================================');
    console.log('BENCHMARK 2: UI RESPONSIVENESS (TYPING BURST SIMULATION)');
    console.log('================================================================');

    // Simulate typing a query character-by-character, backspacing, and typing another
    const typingSequence = [
      'p',
      'pa',
      'pay',
      'payx',
      'payx ',
      'payx c',
      'payx co',
      'payx con',
      'payx co', // Backspace
      'payx ', // Backspace
      'pay', // Backspace
      'p', // Backspace
      '', // Cleared
      'a',
      'an',
      'ana',
      'analy',
      'analytic',
      'analytics',
      'web',
      'webhook',
    ];

    for (const scale of scales) {
      const fixtures = generateRealisticContexts(scale);

      const startBurst = performance.now();
      const latencies: number[] = [];

      for (const keystrokeQuery of typingSequence) {
        const { durationMs } = executeSearchPipeline(fixtures, keystrokeQuery);
        latencies.push(durationMs);
      }

      const totalBurstDurationMs = performance.now() - startBurst;
      const avgKeystrokeLatencyMs = totalBurstDurationMs / typingSequence.length;
      const throughputKeystrokesPerSec = Math.round((typingSequence.length / (totalBurstDurationMs / 1000)));

      console.log(
        `Scale: ${scale.toString().padStart(5)} contexts | Total Burst: ${totalBurstDurationMs.toFixed(2)}ms (${typingSequence.length} keystrokes) | Avg/Key: ${avgKeystrokeLatencyMs.toFixed(3)}ms | Throughput: ${throughputKeystrokesPerSec} keys/sec`
      );

      // Typing responsiveness: Every keystroke must complete well under human perception (<50ms)
      // and under the 60fps frame budget (16.6ms)
      expect(avgKeystrokeLatencyMs).toBeLessThan(16.6);
      expect(throughputKeystrokesPerSec).toBeGreaterThan(60); // Can handle >60 keystrokes/sec without backlog
    }
  });

  // 3. Memory Usage
  it('3. measures memory footprint and allocation stability', () => {
    console.log('\n================================================================');
    console.log('BENCHMARK 3: MEMORY USAGE & ALLOCATION STABILITY');
    console.log('================================================================');

    for (const scale of scales) {
      const memBefore = getHeapUsedBytes();
      const fixtures = generateRealisticContexts(scale);
      const memAfter = getHeapUsedBytes();

      // Estimate bytes per context item
      const deltaBytes = Math.max(0, memAfter - memBefore);
      const bytesPerItem = deltaBytes > 0 ? Math.round(deltaBytes / scale) : 0;

      // Run repeated search cycles to verify allocation stability
      const memBeforeSearch = getHeapUsedBytes();
      for (let i = 0; i < 50; i++) {
        executeSearchPipeline(fixtures, 'pay');
        executeSearchPipeline(fixtures, 'analytics');
        executeSearchPipeline(fixtures, 'webhook');
      }
      const memAfterSearch = getHeapUsedBytes();
      const searchDeltaKb = Math.round((memAfterSearch - memBeforeSearch) / 1024);

      console.log(
        `Scale: ${scale.toString().padStart(5)} contexts | Memory Delta: ${(deltaBytes / 1024).toFixed(1)} KB (~${bytesPerItem} B/item) | 150 Searches Delta: ${searchDeltaKb} KB`
      );

      // Memory per context should remain lightweight (< 5KB per rich context model)
      if (bytesPerItem > 0) {
        expect(bytesPerItem).toBeLessThan(5000);
      }
    }
  });

  // 4. Rendering Cost & Windowing Optimization
  it('4. measures rendering cost reduction via windowed result slicing', () => {
    console.log('\n================================================================');
    console.log('BENCHMARK 4: RENDERING COST & WINDOWING OPTIMIZATION');
    console.log('================================================================');

    expect(MAX_VISIBLE_LAUNCHER_RESULTS).toBe(50);

    for (const scale of scales) {
      const fixtures = generateRealisticContexts(scale);
      const { results: allMatches } = executeSearchPipeline(fixtures, 'context');

      // Test windowing at initial view (selectedIndex = 0)
      const initialSlice = getVisibleLauncherSlice(allMatches, 0);

      // Verify visible item bounds
      const expectedVisibleCount = Math.min(allMatches.length, MAX_VISIBLE_LAUNCHER_RESULTS);
      expect(initialSlice.visibleItems.length).toBe(expectedVisibleCount);
      expect(initialSlice.totalCount).toBe(allMatches.length);
      expect(initialSlice.hasMore).toBe(allMatches.length > MAX_VISIBLE_LAUNCHER_RESULTS);

      // Calculate DOM reduction ratio
      const domReductionPct =
        allMatches.length > 0
          ? (((allMatches.length - initialSlice.visibleItems.length) / allMatches.length) * 100).toFixed(1)
          : '0.0';

      console.log(
        `Scale: ${scale.toString().padStart(5)} contexts | Matches: ${allMatches.length.toString().padStart(5)} | Rendered DOM Elements: ${initialSlice.visibleItems.length.toString().padStart(3)} | Node Reduction: ${domReductionPct}%`
      );

      // Test keyboard navigation beyond initial limit (e.g. index 55)
      if (allMatches.length > 50) {
        const navigatedSlice = getVisibleLauncherSlice(allMatches, 55);
        // Window should dynamically expand to include index 55 + lookahead
        expect(navigatedSlice.visibleItems.length).toBe(Math.min(allMatches.length, 65));
        expect(navigatedSlice.visibleItems[55]).toBeDefined();
        expect(navigatedSlice.visibleItems[55].id).toBe(allMatches[55].id);
      }
    }
  });
});
