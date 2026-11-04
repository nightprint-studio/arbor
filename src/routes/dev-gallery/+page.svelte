<script lang="ts">
  /**
   * Dev gallery: `/dev-gallery?mock` lists every scene; `/dev-gallery?mock&scene=<id>` shows one.
   *
   * It also installs `window.__gallery`, the surface the automation drives: `run(id)` mounts a
   * scene, waits for it to settle and returns its status and DOM, `runAll()` does that for every
   * scene in turn. That is how a revision gets snapshotted and two revisions get compared.
   */
  import { tick } from 'svelte';
  import type { Component } from 'svelte';
  import { scenes, sceneById } from '$lib/dev/gallery/scenes';
  import SceneHarness from '$lib/dev/gallery/SceneHarness.svelte';

  interface RunResult {
    id: string;
    state: 'ok' | 'error' | 'skipped' | 'unknown';
    error?: string;
    skipReason?: string;
    html?: string;
  }

  let current = $state<{ id: string; Comp: Component<any>; props: Record<string, unknown> } | null>(null);
  let status = $state<{ state: 'ok' | 'error'; error?: string } | null>(null);

  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const readStatus = () => status;

  /** The DOM a scene produced: the stage plus anything portalled to <body>, minus this page's own chrome. */
  function captureDom(): string {
    const clone = document.body.cloneNode(true) as HTMLElement;
    clone.querySelectorAll('[data-gallery-chrome]').forEach((n) => n.remove());
    clone.querySelectorAll('script').forEach((n) => n.remove());
    return clone.innerHTML.replace(/\s+/g, ' ').trim();
  }

  async function run(id: string, withHtml = true): Promise<RunResult> {
    const scene = sceneById(id);
    if (!scene) return { id, state: 'unknown' };
    if (scene.skip) return { id, state: 'skipped', skipReason: scene.skip };
    status = null;
    current = null;
    await tick();
    try {
      await scene.setup?.();
      const mod = await scene.load();
      const props = typeof scene.props === 'function' ? scene.props() : (scene.props ?? {});
      current = { id, Comp: mod.default, props };
    } catch (e) {
      return { id, state: 'error', error: e instanceof Error ? e.message : String(e) };
    }
    await sleep(scene.settle ?? 400);
    await tick();
    // `status` is written by the harness callback; read through a function so TypeScript does not
    // narrow it to the `null` assigned at the top of this run.
    const reported = readStatus();
    const result: RunResult = { id, state: reported?.state ?? 'ok', error: reported?.error };
    if (withHtml) result.html = captureDom();
    return result;
  }

  async function runAll(ids?: string[], withHtml = true): Promise<RunResult[]> {
    const out: RunResult[] = [];
    for (const id of ids ?? scenes.map((s) => s.id)) out.push(await run(id, withHtml));
    current = null;
    return out;
  }

  /** Run every scene and POST each result to the dump endpoint (`vite.gallery.config.js`). */
  async function dumpAll(ids?: string[]): Promise<{ total: number; by: Record<string, number> }> {
    const by: Record<string, number> = {};
    const list = ids ?? scenes.map((s) => s.id);
    for (const id of list) {
      const result = await run(id, true);
      by[result.state] = (by[result.state] ?? 0) + 1;
      await fetch('/__dump', { method: 'POST', body: JSON.stringify({ id, result }) });
    }
    current = null;
    return { total: list.length, by };
  }

  if (typeof window !== 'undefined') {
    (window as unknown as { __gallery: unknown }).__gallery = {
      ids: () => scenes.map((s) => s.id),
      run,
      runAll,
      dumpAll,
    };
    const q = new URLSearchParams(location.search).get('scene');
    if (q) void run(q, false);
  }
</script>

{#if current}
  {#key current.id}
    <SceneHarness Comp={current.Comp} props={current.props} onstatus={(s) => (status = s)} />
  {/key}
{/if}

<div data-gallery-chrome class="chrome">
  {#if current}
    <span class="tag" class:bad={status?.state === 'error'}>{current.id}{status?.state === 'error' ? ' — ERROR' : ''}</span>
  {:else}
    <h1>Dev gallery — {scenes.length} scenes</h1>
    <ul>
      {#each scenes as s (s.id)}
        <li><a href="?mock&scene={encodeURIComponent(s.id)}" class:skip={!!s.skip}>{s.id}</a>{#if s.skip} <em>({s.skip})</em>{/if}</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .chrome { position: fixed; z-index: 99999; font: 12px var(--font-ui, sans-serif); color: var(--text-secondary, #aaa); }
  .chrome:has(h1) { position: static; padding: 16px; background: var(--bg-base); min-height: 100vh; }
  h1 { font-size: 16px; color: var(--text-primary, #eee); }
  ul { columns: 3; list-style: none; padding: 0; }
  a { color: var(--accent, #6af); text-decoration: none; }
  a.skip { opacity: 0.5; }
  /* Dev-only label of the scene on screen: out of the way (top right, no pointer events), in theme colours. */
  .tag { position: fixed; right: 8px; bottom: 34px; padding: 1px 8px; border-radius: var(--radius-sm, 4px); background: var(--bg-overlay, #1c2128); border: 1px solid var(--border-subtle, #30363d); color: var(--text-muted, #8b949e); font-size: 11px; opacity: 0.6; pointer-events: none; }
  .tag.bad { background: var(--error, #a00); color: #fff; opacity: 1; }
</style>
