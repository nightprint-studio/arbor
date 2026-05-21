<script lang="ts">
  import Modal from '../shared/Modal.svelte';
  import ModalHeader from '../shared/ModalHeader.svelte';
  import Button from '../shared/ui/Button.svelte';
  import type { CommitNode } from '$lib/types/git';
  import { ChevronDown, Check, Upload } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    node,
    onClose,
    onCreate,
  }: {
    node: CommitNode;
    onClose: () => void;
    /** push=true creates the tag AND pushes it to origin in one go. */
    onCreate: (name: string, push: boolean) => void;
  } = $props();

  let name        = $state('');
  let nameInputEl: HTMLInputElement | undefined = $state();
  $effect(() => { nameInputEl?.focus(); });
  let menuOpen    = $state(false);
  let menuAnchor  = $state<HTMLElement | null>(null);
  let menuPos     = $state<{ x: number; y: number } | null>(null);

  const valid = $derived(name.trim().length > 0);

  function submit(push: boolean) {
    if (!valid) return;
    onCreate(name.trim(), push);
  }

  function toggleMenu() {
    if (menuOpen) { menuOpen = false; return; }
    if (!menuAnchor) return;
    const r = menuAnchor.getBoundingClientRect();
    menuPos  = { x: r.right - 220, y: r.bottom + 4 };
    menuOpen = true;
  }

  function closeMenu() { menuOpen = false; }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') { menuOpen = false; return; }
    if (e.key === 'Enter' && valid) submit(false);
  }
</script>

<Modal {onClose} ariaLabel="Create Tag">
  {#snippet header()}
    <ModalHeader title="Create Tag" {onClose} />
  {/snippet}
  <div style="display:flex; flex-direction:column; gap:12px">
    <p style="color:var(--text-secondary); font-size:var(--font-size-sm)">
      Tag commit <code style="font-family:var(--font-code)">{node.short_oid}</code>
    </p>
    <input
      class="input"
      placeholder="Tag name (e.g. v1.0.0)…"
      bind:value={name}
      onkeydown={onKey}
      bind:this={nameInputEl}
    />

    <div class="actions">
      <Button variant="secondary" onclick={onClose}>Cancel</Button>

      <!-- Split button: primary action + dropdown chevron -->
      <div class="split-btn" class:open={menuOpen}>
        <button
          class="split-main"
          onclick={() => submit(false)}
          disabled={!valid}
          use:tooltip={'Crea il tag in locale'}
        >
          <Check size={13} /> Create
        </button>
        <button
          class="split-chevron"
          bind:this={menuAnchor}
          onclick={toggleMenu}
          disabled={!valid}
          aria-haspopup="menu"
          aria-expanded={menuOpen}
          use:tooltip={'Altre azioni'}
        >
          <ChevronDown size={12} />
        </button>
      </div>
    </div>
  </div>
</Modal>

{#if menuOpen && menuPos}
  <button type="button" aria-label="Close menu" class="menu-backdrop" onclick={closeMenu}></button>
  <div class="split-menu" style="left:{menuPos.x}px; top:{menuPos.y}px" role="menu">
    <button
      class="split-menu-item"
      onclick={() => { closeMenu(); submit(true); }}
      role="menuitem"
    >
      <Upload size={13} />
      <div class="split-menu-text">
        <div class="split-menu-title">Create &amp; Push</div>
        <div class="split-menu-sub">Crea il tag e lo pusha su <code>origin</code></div>
      </div>
    </button>
  </div>
{/if}

<style>
  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    align-items: center;
  }

  /* ── Split button ────────────────────────────────────────────────────── */
  .split-btn {
    display: inline-flex;
    align-items: stretch;
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
    transition: filter var(--transition-fast);
  }
  .split-btn.open { filter: brightness(0.95); }

  .split-main, .split-chevron {
    background: transparent;
    border: none;
    color: var(--text-on-accent);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .split-main {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 12px;
    font-weight: 500;
  }
  .split-main:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
  }

  .split-chevron {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0 7px;
    border-left: 1px solid rgba(255, 255, 255, 0.25);
  }
  .split-chevron:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
  }

  .split-btn:has(.split-main:disabled),
  .split-btn:has(.split-chevron:disabled) {
    opacity: 0.55;
    box-shadow: 0 0 0 1px var(--border);
    background: var(--bg-overlay);
  }
  .split-main:disabled, .split-chevron:disabled { cursor: not-allowed; }

  /* ── Dropdown menu ───────────────────────────────────────────────────── */
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1100;
    background: transparent;
    border: none;
    padding: 0;
    cursor: default;
  }
  .split-menu {
    position: fixed;
    z-index: 1101;
    min-width: 220px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-popup);
    padding: 4px;
    font-family: var(--font-ui-sans);
  }
  .split-menu-item {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-primary);
    text-align: left;
    transition: background var(--transition-fast);
  }
  .split-menu-item:hover { background: var(--bg-hover); }
  .split-menu-text { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .split-menu-title {
    font-size: var(--font-size-sm);
    font-weight: 500;
  }
  .split-menu-sub {
    font-size: 10px;
    color: var(--text-muted);
  }
  .split-menu-sub :global(code) {
    font-family: var(--font-code);
    color: var(--text-secondary);
  }
</style>
