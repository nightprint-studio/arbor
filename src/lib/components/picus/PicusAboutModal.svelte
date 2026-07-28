<script lang="ts">
  /**
   * PicusAboutModal — Picus's "about" card (opened from the hamburger menu).
   * Same layout language as the other product about cards, with its own identity
   * (woodpecker · SQL studio) and a quick shortcut list sourced from
   * `PICUS_SHORTCUTS`, so it can never drift from the real bindings.
   */
  import { Database, Cpu, Keyboard, Building2, Layers } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { getAppInfo, type AppInfo } from '$lib/ipc/app';
  import { PICUS_SHORTCUTS } from './picus-shortcuts';

  let { onClose }: { onClose: () => void } = $props();

  const RELEASE_YEAR = new Date().getFullYear();

  let appInfo = $state<AppInfo | null>(null);
  $effect(() => {
    getAppInfo().then((info) => { appInfo = info; }).catch(() => {});
  });

  const buildRows = $derived([
    { label: 'Version', value: appInfo ? `v${appInfo.version}` : '—' },
    { label: 'Runtime', value: 'Tauri 2 + Rust' },
    { label: 'Frontend', value: 'Svelte 5 Runes' },
    { label: 'Engines', value: 'Oracle · PostgreSQL (backend in progress)' },
    { label: 'Platform', value: appInfo ? `${appInfo.os} · ${appInfo.arch}` : '—' },
  ]);

  /** One taste per group, always in step with the canonical list. */
  const quickShortcuts = PICUS_SHORTCUTS.flatMap((g) => g.shortcuts.slice(0, 2)).slice(0, 8);
</script>

<Modal {onClose} width="720px" height="540px" padBody={false} ariaLabel="About Picus">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Database size={14} />
      <span class="modal-title">About Picus</span>
    </ModalHeader>
  {/snippet}

  <div class="ab">
    <section class="ab-hero">
      <ArborLogo size={44} />
      <div class="ab-hero-text">
        <h1>Picus</h1>
        <p class="ab-tagline">SQL studio — databases and the scripts that build them</p>
        <p class="ab-blurb">
          A client for Oracle and PostgreSQL, and a maintainer for the script repository
          they are installed from: it reads the folder tree as it is, works out which
          engine each part is written for, checks that they stay in step, and generates the
          changes that keep them there.
        </p>
      </div>
    </section>

    <div class="ab-cols">
      <section class="ab-card">
        <h2><Cpu size={13} /> Build</h2>
        <dl class="ab-rows">
          {#each buildRows as row (row.label)}
            <div class="ab-row">
              <dt>{row.label}</dt>
              <dd>{row.value}</dd>
            </div>
          {/each}
        </dl>
      </section>

      <section class="ab-card">
        <h2><Keyboard size={13} /> Shortcuts</h2>
        <ul class="ab-keys">
          {#each quickShortcuts as s (s.description)}
            <li>
              <span>{s.description}</span>
              <Kbd keys={s.keys} size="sm" />
            </li>
          {/each}
        </ul>
      </section>
    </div>

    <section class="ab-card ab-principles">
      <h2><Layers size={13} /> Two things it will not do</h2>
      <ul>
        <li>
          <b>No language model, anywhere in the flow.</b> SQL generation is deterministic:
          structured input becomes a model, the model is emitted per dialect. It is a
          product requirement, not a preference.
        </li>
        <li>
          <b>No password of yours.</b> Credentials belong to Arbor's keychain; Picus asks
          for a handle and receives the secret at the moment of use.
        </li>
      </ul>
    </section>

    <footer class="ab-foot">
      <Building2 size={12} />
      <span>Nightprint Studio · © {RELEASE_YEAR}</span>
    </footer>
  </div>
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }

  .ab {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    overflow-y: auto;
    padding: 16px;
  }

  .ab-hero { display: flex; gap: 16px; align-items: flex-start; }
  .ab-hero-text h1 { font-size: 20px; font-weight: 600; margin-bottom: 2px; }
  .ab-tagline { font-size: 12px; color: var(--accent); margin-bottom: 6px; }
  .ab-blurb { font-size: 12px; line-height: 1.55; color: var(--text-muted); max-width: 62ch; }

  .ab-cols { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 12px; }

  .ab-card {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 12px;
  }
  .ab-card h2 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .ab-card h2 :global(svg) { color: var(--text-disabled); }

  .ab-rows { margin: 0; display: flex; flex-direction: column; gap: 4px; }
  .ab-row { display: flex; gap: 10px; font-size: 11.5px; }
  .ab-row dt { width: 88px; flex-shrink: 0; color: var(--text-muted); }
  .ab-row dd { margin: 0; color: var(--text-secondary); }

  .ab-keys { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .ab-keys li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    font-size: 11.5px;
    color: var(--text-secondary);
  }

  .ab-principles ul { margin: 0; padding-left: 18px; display: flex; flex-direction: column; gap: 7px; }
  .ab-principles li { font-size: 11.5px; line-height: 1.55; color: var(--text-muted); }
  .ab-principles b { color: var(--text-secondary); }

  .ab-foot {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-top: 4px;
    font-size: 11px;
    color: var(--text-disabled);
  }
</style>
