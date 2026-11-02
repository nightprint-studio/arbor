<script lang="ts">
  /**
   * The products as a strip of tall panels. One is open at a time — the one the pointer settled on, or the arrow keys
   * reached — and it widens and grows in height while the others close up, so what you need (what it is, Open, Stop)
   * is inside the panel rather than in a second block beside it. The strip's full height is reserved from the start:
   * opening a panel never moves what is under it.
   */
  import { RotateCcw } from 'lucide-svelte';
  import ProductIcon from '$lib/components/shared/internal/ProductIcon.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import type { DecoratedTool } from '$lib/stores/launcher/state.svelte';

  let { tools, initial, onlaunch, onstop, onrestart }: {
    tools: DecoratedTool[];
    /** The product whose panel starts open. */
    initial: string | null;
    onlaunch: (t: DecoratedTool) => void;
    onstop: (t: DecoratedTool) => void | Promise<void>;
    onrestart: (t: DecoratedTool) => void | Promise<void>;
  } = $props();

  // The chosen one; until someone chooses, the starting product (or the first).
  let chosen = $state<string | null>(null);
  const openId = $derived(
    tools.some((t) => t.id === chosen) ? chosen
    : tools.some((t) => t.id === initial) ? initial
    : (tools[0]?.id ?? null),
  );

  let busy = $state(false);
  async function run(action: () => void | Promise<void>) {
    if (busy) return;
    busy = true;
    try { await action(); } finally { busy = false; }
  }

  // Crossing the strip must not make it dance under the pointer: a panel opens once the pointer has settled on it.
  let intent: ReturnType<typeof setTimeout> | undefined;
  function settle(id: string) { clearTimeout(intent); intent = setTimeout(() => (chosen = id), 110); }

  const hits: Record<string, HTMLButtonElement | undefined> = {};
  function go(delta: number | 'first' | 'last', from: string) {
    const i = tools.findIndex((t) => t.id === from);
    const next = delta === 'first' ? 0 : delta === 'last' ? tools.length - 1 : (i + delta + tools.length) % tools.length;
    chosen = tools[next].id;
    queueMicrotask(() => hits[chosen!]?.focus());
  }
  function onKey(e: KeyboardEvent, id: string) {
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') go(1, id);
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') go(-1, id);
    else if (e.key === 'Home') go('first', id);
    else if (e.key === 'End') go('last', id);
    else return;
    e.preventDefault();
  }
</script>

<div class="strip" role="group" aria-label="Products" onpointerleave={() => clearTimeout(intent)}>
  {#each tools as t (t.id)}
    {@const on = t.id === openId}
    <div class="pn" role="group" aria-label={t.name} data-on={on} style="--c:{t.accent}" onpointerenter={() => settle(t.id)}>
      <button
        type="button" class="hit" bind:this={hits[t.id]}
        aria-label="{t.name}, {t.role}" onclick={() => onlaunch(t)} onkeydown={(e) => onKey(e, t.id)}
      ></button>
      <span class="icon"><ProductIcon id={t.id} size={46} /></span>
      <span class="vn">{t.name}</span>
      {#if t.isRunning}<i class="live" title="Running"></i>{/if}
      <div class="pb">
        <div class="head">
          <b>{t.name}{#if t.isRunning}<span class="st"><i></i>Running</span>{:else if t.isUpd}<span class="st upd">Update available</span>{/if}</b>
          <span class="role">{t.role}</span>
        </div>
        {#if t.blurb}<p title={t.blurb}>{t.blurb}</p>{/if}
        <div class="crow">
          <Button variant="tonal" color={t.accent} size="md" onclick={() => onlaunch(t)}>{t.actionLabel}</Button>
          {#if t.isRunning}
            <Button variant="outline" size="md" disabled={busy}
                    tooltip={{ content: 'Close it and start it again — picks up a rebuilt backend' }}
                    onclick={() => void run(() => onrestart(t))}>
              {#snippet iconStart()}<RotateCcw size={14} />{/snippet}
              Restart
            </Button>
            <Button variant="danger" size="md" disabled={busy} onclick={() => void run(() => onstop(t))}>Stop</Button>
          {/if}
        </div>
      </div>
    </div>
  {/each}
</div>

<style>
  .strip { --open: 224px; --shut: 112px; display: flex; align-items: flex-start; gap: 8px; height: var(--open); }
  .pn {
    position: relative; flex: 1 1 0; min-width: 0; height: var(--shut); overflow: hidden; border-radius: 18px;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle, var(--border));
    transition: flex 650ms cubic-bezier(0.22, 0.8, 0.22, 1), height 650ms cubic-bezier(0.22, 0.8, 0.22, 1),
                border-color 500ms, box-shadow 650ms;
  }
  .pn[data-on='true'] {
    flex: 4.4; height: var(--open);
    border-color: color-mix(in srgb, var(--c) 55%, var(--border));
    box-shadow: 0 18px 44px color-mix(in srgb, var(--c) 18%, transparent);
  }
  /* the product's colour rises from the bottom corner as it opens */
  .pn::before {
    content: ''; position: absolute; inset: 0; pointer-events: none; opacity: 0; transform: translateY(40%);
    background: radial-gradient(130% 100% at 0% 100%, color-mix(in srgb, var(--c) 42%, transparent),
                color-mix(in srgb, var(--c) 8%, transparent) 55%, transparent 78%);
    transition: opacity 700ms ease, transform 800ms cubic-bezier(0.22, 0.8, 0.22, 1);
  }
  .pn[data-on='true']::before { opacity: 1; transform: none; }

  .hit { position: absolute; inset: 0; border: 0; background: none; cursor: pointer; border-radius: inherit; }
  .hit:focus-visible { outline: 2px solid var(--accent); outline-offset: -3px; }

  .icon {
    position: absolute; top: 16px; left: calc(50% - 23px); display: flex; pointer-events: none;
    transition: left 650ms cubic-bezier(0.22, 0.8, 0.22, 1), top 650ms cubic-bezier(0.22, 0.8, 0.22, 1),
                transform 650ms cubic-bezier(0.3, 1.5, 0.5, 1), filter 650ms;
  }
  .pn:hover .icon { transform: scale(1.12) rotate(-5deg); }
  .pn[data-on='true'] .icon { top: 22px; left: 22px; transform: scale(1.18) rotate(-4deg); filter: drop-shadow(0 12px 18px color-mix(in srgb, var(--c) 45%, transparent)); }

  .vn {
    position: absolute; left: 4px; right: 4px; bottom: 14px; text-align: center; font-size: 13px; font-weight: 600;
    color: var(--text-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; pointer-events: none;
    transition: opacity 350ms, color 350ms;
  }
  .pn:hover .vn { color: var(--text-primary); }
  .pn[data-on='true'] .vn { opacity: 0; }

  .live { position: absolute; top: 12px; right: 12px; width: 11px; height: 11px; border-radius: 50%; background: var(--success); animation: pulse 2s ease-out infinite; }
  .pn[data-on='true'] .live { display: none; }
  @keyframes pulse { 0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--success) 60%, transparent); } 100% { box-shadow: 0 0 0 8px transparent; } }

  /* open: the text and the buttons arrive after the width has mostly settled; closed ones leave the tab order. The block is
     laid out top to bottom beside the icon — head, a blurb cut at three lines, buttons — so a long description can never run
     up over the icon: what does not fit is cut, and the full text is on the panel's tooltip. */
  .pb {
    position: absolute; left: 22px; top: 22px; bottom: 20px; width: 330px; display: flex; flex-direction: column;
    opacity: 0; transform: translateY(14px); visibility: hidden;
    transition: opacity 350ms ease, transform 350ms ease, visibility 0s 350ms;
  }
  .pn[data-on='true'] .pb {
    opacity: 1; transform: none; visibility: visible;
    transition: opacity 500ms ease 280ms, transform 500ms cubic-bezier(0.22, 0.8, 0.22, 1) 280ms, visibility 0s;
  }
  .head { min-height: 56px; padding-left: 70px; display: flex; flex-direction: column; justify-content: center; }
  .head b { font-size: 20px; letter-spacing: -0.01em; color: var(--text-primary); line-height: 1.2; }
  .role { display: block; margin-top: 1px; font-size: var(--font-size-sm); color: color-mix(in srgb, var(--c) 55%, var(--text-secondary)); }
  .pb p {
    margin: 10px 0 0; font-size: var(--font-size-sm); line-height: 1.45; color: var(--text-secondary);
    display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden;
  }
  .st { display: inline-flex; align-items: center; gap: 6px; margin-left: 8px; font-size: 11px; font-weight: 400; color: var(--success); vertical-align: 2px; }
  .st i { width: 7px; height: 7px; border-radius: 50%; background: var(--success); animation: pulse 2s ease-out infinite; }
  .st.upd { color: var(--warning); }
  .crow { display: flex; gap: 8px; margin-top: auto; }

  @media (max-width: 900px) { .pb { width: 270px; } }
  @media (max-width: 760px) {
    .strip { --open: 200px; --shut: 64px; flex-direction: column; height: auto; }
    .pn { flex: none; height: var(--shut); transition: height 600ms cubic-bezier(0.22, 0.8, 0.22, 1), border-color 500ms; }
    .pn[data-on='true'] { height: var(--open); }
    .icon { top: 9px; left: 14px !important; }
    .vn { display: none; }
    .pb { left: 78px; top: 10px; width: calc(100% - 92px); }
    .head { padding-left: 0; min-height: 0; }
  }
</style>
