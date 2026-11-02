<script lang="ts">
  /** The way back into the last project: one card, whichever product opened it. */
  import { ArrowRight } from 'lucide-svelte';
  import ProductIcon from '$lib/components/shared/internal/ProductIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { RecentProject } from '$lib/ipc/recents';
  import { whenLabel } from './welcome-logic';

  let { project, productName, onopen }: {
    project: RecentProject;
    productName: string;
    onopen: () => void;
  } = $props();
</script>

<button type="button" class="resume" onclick={onopen} use:tooltip={project.path}>
  <ProductIcon id={project.product} size={44} />
  <span class="t">
    <span class="kicker">Continue where you left off · {whenLabel(project.opened_at, Date.now() / 1000)}</span>
    <span class="name">{project.name}</span>
    <span class="path">{project.path}</span>
  </span>
  <span class="go">in {productName} <ArrowRight size={18} /></span>
</button>

<style>
  .resume {
    display: flex; align-items: center; gap: 16px; width: 100%; margin-top: 18px; padding: 16px 20px; text-align: left; cursor: pointer;
    color: inherit; font: inherit;
    background: linear-gradient(100deg, color-mix(in srgb, var(--accent) 16%, transparent), color-mix(in srgb, var(--accent) 6%, transparent) 60%, transparent), var(--bg-elevated);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent); border-radius: var(--radius-lg);
    transition: transform var(--anim-dur-base), border-color var(--anim-dur-base);
  }
  .resume:hover { transform: translateY(-1px); border-color: var(--accent); }
  .resume:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .kicker { font-size: 11px; font-weight: 600; letter-spacing: 0.07em; text-transform: uppercase; color: var(--accent); }
  .name { font-size: 18px; font-weight: 600; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .path { font-family: var(--font-code); font-size: 12px; color: var(--text-disabled); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; direction: rtl; text-align: left; }
  .go { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); transition: color var(--anim-dur-base); }
  .resume:hover .go { color: var(--accent); }
</style>
