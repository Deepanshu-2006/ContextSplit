import { describe, it, expect } from 'vitest';
// @ts-expect-error node:fs in vitest environment
import * as fs from 'node:fs';
// @ts-expect-error node:path in vitest environment
import * as path from 'node:path';
// @ts-expect-error node:url in vitest environment
import { fileURLToPath } from 'node:url';

const currentDir = path.dirname(fileURLToPath(import.meta.url));
const launcherSource = fs.readFileSync(path.resolve(currentDir, 'ContextLauncher.tsx'), 'utf-8');
const appCssSource = fs.readFileSync(path.resolve(currentDir, '../../App.css'), 'utf-8');

describe('Context Launcher Restrained Animations & Motion Accessibility', () => {
  describe('1. CSS Keyframe Timing and Restraint', () => {
    it('defines launcher dialog and popover keyframes in App.css', () => {
      expect(appCssSource).toContain('@keyframes launcherFadeIn');
      expect(appCssSource).toContain('@keyframes launcherPopoverFadeIn');
      expect(appCssSource).toContain('.launcher-dialog-enter');
      expect(appCssSource).toContain('.launcher-popover-enter');
    });

    it('uses restrained timings (<= 120ms) and subtle scale (<= 2%) for popover and dialog entrances', () => {
      // launcherFadeIn: 120ms, scale(0.99) -> scale(1)
      expect(appCssSource).toMatch(/launcherFadeIn\s+120ms\s+cubic-bezier/);
      expect(appCssSource).toContain('scale(0.99)');

      // launcherPopoverFadeIn: 100ms, scale(0.98) -> scale(1)
      expect(appCssSource).toMatch(/launcherPopoverFadeIn\s+100ms\s+cubic-bezier/);
      expect(appCssSource).toContain('scale(0.98)');
    });

    it('enforces prefers-reduced-motion media query overrides in App.css', () => {
      expect(appCssSource).toContain('@media (prefers-reduced-motion: reduce)');
      expect(appCssSource).toContain('animation: none !important');
      expect(appCssSource).toContain('transform: none !important');
    });
  });

  describe('2. Layout Stability (No Dimension Transitions)', () => {
    it('does not animate layout dimensions (transition-all) on list items or modal containers', () => {
      // List items must use transition-colors or transition-opacity, never transition-all
      expect(launcherSource).not.toMatch(/ContextLauncherItem[\s\S]*?className=[\s\S]*?transition-all/);
      // Check modal backdrop
      expect(launcherSource).not.toMatch(/role="dialog"[\s\S]*?transition-all/);
      // Ensure transition-all is absent throughout ContextLauncher
      expect(launcherSource).not.toContain('transition-all');
    });

    it('uses snappy, restrained color and opacity transitions (duration-75 or duration-150)', () => {
      expect(launcherSource).toContain('transition-colors duration-75');
      expect(launcherSource).toContain('transition-opacity duration-150');
    });
  });

  describe('3. Reduced-Motion Compliance for Spinners and Loaders', () => {
    it('ensures all animate-spin loading spinners include motion-reduce:animate-none', () => {
      const spinRegex = /className="[^"]*animate-spin[^"]*"/g;
      const matches = launcherSource.match(spinRegex) || [];

      expect(matches.length).toBeGreaterThan(0);
      for (const match of matches) {
        expect(match).toContain('motion-reduce:animate-none');
      }
    });

    it('ensures all animate-pulse skeleton loaders include motion-reduce:animate-none', () => {
      const pulseRegex = /className="[^"]*animate-pulse[^"]*"/g;
      const matches = launcherSource.match(pulseRegex) || [];

      expect(matches.length).toBeGreaterThan(0);
      for (const match of matches) {
        expect(match).toContain('motion-reduce:animate-none');
      }
    });
  });

  describe('4. Micro-Interaction Scale Accessibility', () => {
    it('ensures all active:scale transforms include motion-reduce:transform-none and motion-reduce:transition-none', () => {
      const scaleRegex = /className="[^"]*active:scale-[^"]*"/g;
      const matches = launcherSource.match(scaleRegex) || [];

      expect(matches.length).toBeGreaterThan(0);
      for (const match of matches) {
        expect(match).toContain('motion-reduce:transform-none');
        expect(match).toContain('motion-reduce:transition-none');
      }
    });
  });

  describe('5. Clean Aesthetic (No Neon Glows or Excessive Shadows)', () => {
    it('does not use colored glow shadows on status indicators or badges', () => {
      expect(launcherSource).not.toMatch(/shadow-\[0_0_\d+px/);
    });

    it('uses standard restrained utility classes launcher-dialog-enter and launcher-popover-enter', () => {
      expect(launcherSource).toContain('launcher-dialog-enter');
      expect(launcherSource).toContain('launcher-popover-enter');
      expect(launcherSource).not.toContain('animate-in fade-in zoom-in-95');
    });
  });
});
