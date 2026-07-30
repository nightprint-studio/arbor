<script lang="ts">
  /**
   * TytoAboutModal — Tyto's "about" card (opened from the hamburger menu).
   * Tyto-branded sibling of the shared AboutModal: same layout language, its own
   * identity (barn owl · screen recorder) and its own quick shortcut reference
   * sourced from TYTO_SHORTCUTS so it never drifts from the real bindings.
   */
  import { Layers, Cpu, Keyboard, Building2, Feather } from 'lucide-svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { getAppInfo, type AppInfo } from '$lib/ipc/app';
  import { tooltip } from '$lib/actions/tooltip';
  import { TYTO_SHORTCUTS } from './tyto-shortcuts';

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
    { label: 'Capture', value: 'Windows (backend in progress)' },
    { label: 'Platform', value: appInfo ? `${appInfo.os} · ${appInfo.arch}` : '—' },
  ]);

  const techStack = [
    { badge: 'Rust', cls: 'rust', desc: 'Backend · capture · encode' },
    { badge: 'Svelte 5', cls: 'svelte', desc: 'Runes API · TypeScript' },
    { badge: 'Tauri 2', cls: 'tauri', desc: 'Native shell · IPC layer' },
  ];

  // First entry of each group — a compact taste of the shortcuts, always in
  // sync with the canonical list.
  const quickShortcuts = TYTO_SHORTCUTS.flatMap((g) => g.shortcuts).slice(0, 8);

  async function openExternal(url: string) {
    try { await openUrl(url); } catch { /* ignore */ }
  }
</script>

<Modal {onClose} width="720px" height="520px" padBody={false} ariaLabel="About Tyto">
  {#snippet header()}
    <ModalHeader {onClose}>
      <div class="header-logo"><ArborLogo size={18} /></div>
      <span class="header-title">Tyto</span>
      {#if appInfo}<span class="header-version">v{appInfo.version}</span>{/if}
      <span class="header-sub">Screen recorder &amp; screenshots</span>
    </ModalHeader>
  {/snippet}

  <div class="about-body">
    <div class="col">
      <div class="group-label"><Layers size={10} /> Build info</div>
      <div class="card">
        {#each buildRows as row}
          <div class="card-row">
            <span class="row-key">{row.label}</span>
            <span class="row-val">{row.value}</span>
          </div>
        {/each}
      </div>

      <div class="group-label"><Feather size={10} /> About the name</div>
      <p class="blurb">
        <b>Tyto</b> — the barn owl (<i>Tyto alba</i>): the suite's silent watcher,
        recording the screen without a sound. Part of Nightprint Studio's
        bird-named toolkit alongside Corvus, Merula and Sitta.
      </p>

      <div class="group-label"><Cpu size={10} /> Technology</div>
      <div class="tech-strip">
        {#each techStack as t}
          <span class="tech-chip {t.cls}" use:tooltip={t.desc}>{t.badge}</span>
        {/each}
      </div>

      <div class="group-label"><Building2 size={10} /> Made by</div>
      <div class="publisher-card">
        <div class="publisher-mark"><span class="publisher-monogram">NS</span></div>
        <div class="publisher-meta">
          <span class="publisher-name">Nightprint Studio</span>
          <button
            type="button"
            class="publisher-link"
            onclick={() => openExternal('https://github.com/nightprint-studio')}
            use:tooltip={'Open Nightprint Studio on GitHub'}
          >github.com/nightprint-studio</button>
        </div>
      </div>
    </div>

    <div class="col">
      <div class="group-label"><Keyboard size={10} /> Keyboard shortcuts</div>
      <div class="card">
        {#each quickShortcuts as s}
          <div class="card-row">
            <span class="row-kbd"><Kbd keys={s.keys} size="sm" /></span>
            <span class="row-val">{s.description}</span>
          </div>
        {/each}
      </div>
      <p class="shortcuts-hint">
        Press <Kbd keys={['Shift', 'F1']} size="sm" /> any time for the full reference.
      </p>
    </div>
  </div>

  {#snippet footer()}
    <div class="about-footer">
      <span class="copyright">© {RELEASE_YEAR} Nightprint Studio</span>
      <span class="sep">·</span>
      <span class="muted">All rights reserved</span>
    </div>
  {/snippet}
</Modal>

<style>
  .header-logo { display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
  .header-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); letter-spacing: 0.01em; }
  .header-version {
    font-size: var(--font-size-2xs); font-family: var(--font-code); color: var(--accent);
    background: var(--accent-subtle); border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
    border-radius: var(--radius-sm); padding: 1px 5px; flex-shrink: 0;
  }
  .header-sub { flex: 1; font-size: var(--font-size-xs); color: var(--text-muted); }

  .about-body { display: flex; height: 100%; overflow-y: auto; font-family: var(--font-ui-sans); }
  .col { flex: 1; padding: 14px 18px 18px; display: flex; flex-direction: column; gap: 10px; }
  .col:first-child { border-right: 1px solid var(--border); }

  .group-label {
    display: flex; align-items: center; gap: 5px;
    font-size: var(--font-size-2xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.7px;
    color: var(--text-disabled); padding: 4px 0 0;
  }
  .group-label:first-child { padding-top: 0; }

  .card {
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); overflow: hidden; box-shadow: 0 1px 3px rgba(0,0,0,0.25);
  }
  .card-row {
    display: flex; align-items: center; gap: 10px;
    padding: 7px 12px; border-bottom: 1px solid var(--border);
  }
  .card-row:last-child { border-bottom: none; }
  .card-row:hover { background: rgba(255,255,255,0.03); }
  .row-key { font-size: var(--font-size-xs); color: var(--text-muted); flex: 0 0 96px; }
  .row-val { font-size: var(--font-size-xs); color: var(--text-secondary); flex: 1; }
  .row-kbd { flex: 0 0 96px; display: flex; }

  .blurb { margin: 0; font-size: var(--font-size-xs); line-height: 1.6; color: var(--text-secondary); }
  .blurb b { color: var(--text-primary); }

  .tech-strip { display: flex; flex-wrap: wrap; gap: 5px; padding: 6px 2px 2px; }
  .tech-chip {
    display: inline-block; font-size: var(--font-size-2xs); font-weight: 700;
    padding: 2px 9px; border-radius: 999px; border: 1px solid transparent;
    text-align: center; cursor: default;
  }
  .tech-chip.rust { background: rgba(178,70,30,0.15); color: #e07040; border-color: rgba(178,70,30,0.3); }
  .tech-chip.svelte { background: rgba(255,100,40,0.12); color: #ff6428; border-color: rgba(255,100,40,0.28); }
  .tech-chip.tauri { background: var(--accent-subtle); color: var(--accent); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }

  .publisher-card {
    display: flex; align-items: center; gap: 11px;
    padding: 10px 12px; background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); box-shadow: 0 1px 3px rgba(0,0,0,0.25);
  }
  .publisher-mark {
    flex-shrink: 0; width: 34px; height: 34px; border-radius: var(--radius-sm);
    background: linear-gradient(135deg, #1a1f2e 0%, #0d1117 100%);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    display: flex; align-items: center; justify-content: center;
  }
  .publisher-monogram {
    font-family: var(--font-code); font-size: var(--font-size-sm); font-weight: 700; letter-spacing: 0.5px;
    background: linear-gradient(135deg, #6b9eff 0%, #c8a8ff 100%);
    -webkit-background-clip: text; background-clip: text; color: transparent;
  }
  .publisher-meta { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .publisher-name { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .publisher-link {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    background: none; border: none; padding: 0; text-align: left; cursor: pointer;
    transition: color 0.12s ease;
  }
  .publisher-link:hover { color: var(--accent); text-decoration: underline; text-underline-offset: 2px; }

  .shortcuts-hint {
    margin: 8px 2px 0; font-size: var(--font-size-2xs); color: var(--text-muted); line-height: 1.7;
    display: flex; align-items: center; gap: 5px; flex-wrap: wrap;
  }

  .about-footer {
    display: flex; align-items: center; justify-content: center; gap: 8px;
    width: 100%; font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .about-footer .copyright { color: var(--text-secondary); font-weight: 500; }
  .about-footer .muted { color: var(--text-disabled); }
  .sep { opacity: 0.4; }
</style>
