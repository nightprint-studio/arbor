/**
 * Scene registry for the dev gallery (`/dev-gallery?mock`).
 *
 * A scene is one component rendered in isolation with fixed props, so the same screen can be
 * screenshotted and its DOM dumped on any revision and the two compared. Every `*Modal.svelte`
 * gets a scene automatically; `registry.ts` (and the per-product files it merges) override
 * props, seed stores, or mark a component as not renderable on its own.
 */
import type { Component } from 'svelte';
import { dev } from '$app/environment';
import { overrides, extras } from './registry';

export interface SceneOverride {
  /** Props to mount with. A function is evaluated at mount, so it can read freshly seeded stores. */
  props?: Record<string, unknown> | (() => Record<string, unknown>);
  /** Seed stores / fixtures before mount (and undo nothing: scenes run in a fresh page or are idempotent). */
  setup?: () => void | Promise<void>;
  /** Milliseconds to let async loads settle before the DOM is read. Default 400. */
  settle?: number;
  /** Why this component cannot be shown on its own (needs a parent, a native window…). */
  skip?: string;
}

/** A scene for a component that is not a `*Modal.svelte` (panels, popups, whole windows). */
export interface ExtraScene extends SceneOverride {
  id: string;
  load: () => Promise<{ default: Component<any> }>;
}

export interface Scene extends SceneOverride {
  /** `corvus/CloneRepoModal` — path under `src/lib/components` without extension. */
  id: string;
  load: () => Promise<{ default: Component<any> }>;
}

const PREFIX = '/src/lib/components/';

// `dev` is a build-time constant: in production the glob below is dead code, so the gallery
// adds nothing to the bundle.
const modules: Record<string, () => Promise<unknown>> = dev
  ? import.meta.glob('/src/lib/components/**/*Modal.svelte')
  : {};

export const scenes: Scene[] = [
  ...Object.entries(modules).map(([path, load]) => {
    const id = path.slice(PREFIX.length).replace(/\.svelte$/, '');
    return { id, load: load as Scene['load'], ...overrides[id] } satisfies Scene;
  }),
  ...(dev ? extras.map((e) => ({ ...e, ...overrides[e.id] }) satisfies Scene) : []),
].sort((a, b) => a.id.localeCompare(b.id));

export const sceneById = (id: string): Scene | undefined => scenes.find((s) => s.id === id);
