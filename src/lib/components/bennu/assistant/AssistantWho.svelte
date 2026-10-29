<script lang="ts">
  /**
   * AssistantWho — the line above a message that says who wrote it: an avatar, a name, an optional badge
   * (`main`, `sub-agent`) and the time. One place for it, so the assistant's own messages and the two sides of an
   * agent's thread name their author the same way.
   */
  import { Bot, Sparkles } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';

  type Tone = 'info' | 'success' | 'warning' | 'error' | 'accent' | 'neutral';

  let {
    name,
    when = '',
    badge = null,
    live = false,
    kind = 'imber',
  }: {
    name: string;
    when?: string;
    badge?: { label: string; tone: Tone } | null;
    /** The avatar glows while this author is writing. */
    live?: boolean;
    /** `imber` (the assistant) or `agent` (a sub-agent). */
    kind?: 'imber' | 'agent';
  } = $props();
</script>

<div class="who">
  <span class="avatar" class:agent={kind === 'agent'} class:asst-live={live}>
    {#if kind === 'agent'}<Bot size={10} />{:else}<Sparkles size={10} />{/if}
  </span>
  <span class="name">{name}</span>
  {#if badge}<Badge variant="tone" size="sm" tone={badge.tone} label={badge.label} />{/if}
  {#if when}<span class="when">{when}</span>{/if}
</div>

<style>
  .who { display: flex; align-items: center; gap: 7px; min-width: 0; }
  .avatar {
    flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    width: 18px; height: 18px; border-radius: 50%;
    color: var(--text-on-accent);
    background: linear-gradient(140deg, var(--accent), color-mix(in srgb, var(--accent) 55%, var(--info)));
    box-shadow: 0 2px 8px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  /* A sub-agent is cooler than the assistant that started it: the same mark, tipped toward the info colour. */
  .avatar.agent {
    background: linear-gradient(140deg, var(--info), color-mix(in srgb, var(--info) 55%, var(--accent)));
    box-shadow: 0 2px 8px color-mix(in srgb, var(--info) 35%, transparent);
  }
  .name {
    min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary);
  }
  .when { font-size: var(--font-size-2xs); color: var(--text-muted); }
</style>
