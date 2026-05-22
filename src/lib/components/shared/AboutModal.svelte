<script lang="ts">
  import { Layers, Cpu, Keyboard, Building2 } from 'lucide-svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ArborLogo from './internal/ArborLogo.svelte';
  import { getAppInfo, type AppInfo } from '$lib/ipc/app';
  import { tooltip } from '$lib/actions/tooltip';

  let { onClose }: { onClose: () => void } = $props();

  async function openExternal(url: string) {
    try { await openUrl(url); } catch { /* ignore */ }
  }

  const RELEASE_YEAR = new Date().getFullYear();

  let appInfo = $state<AppInfo | null>(null);
  $effect(() => {
    getAppInfo().then(info => { appInfo = info; }).catch(() => {});
  });

  const buildRows = $derived([
    { label: 'Version',    value: appInfo ? `v${appInfo.version}` : '—' },
    { label: 'Runtime',    value: 'Tauri 2 + Rust' },
    { label: 'Frontend',   value: 'Svelte 5 Runes' },
    { label: 'Git engine', value: 'libgit2 (vendored)' },
    { label: 'Platform',   value: appInfo ? `${appInfo.os} · ${appInfo.arch}` : '—' },
  ]);

  const techStack = [
    { badge: 'Rust',      cls: 'rust',   desc: 'Backend · git2 · mlua · keyring' },
    { badge: 'Svelte 5',  cls: 'svelte', desc: 'Runes API · TypeScript' },
    { badge: 'Tauri 2',   cls: 'tauri',  desc: 'Native shell · IPC layer' },
    { badge: 'libgit2',   cls: 'lib',    desc: 'Vendored (no system dependency)' },
  ];

  const shortcuts = [
    { keys: 'Ctrl+O',         desc: 'Open repository' },
    { keys: 'Ctrl+W',         desc: 'Close active tab' },
    { keys: 'Ctrl+Tab',       desc: 'Next tab' },
    { keys: 'Ctrl+Shift+Tab', desc: 'Previous tab' },
    { keys: 'Ctrl+,',         desc: 'Settings' },
    { keys: 'Ctrl+K',         desc: 'Command palette' },
    { keys: 'Ctrl+F',         desc: 'Search commits' },
    { keys: 'Ctrl+Shift+S',   desc: 'Toggle stage area' },
    { keys: 'Ctrl+B',         desc: 'Toggle sidebar' },
    { keys: 'Escape',         desc: 'Close panel / search' },
  ];
</script>

<Modal {onClose} width="720px" height="540px" padBody={false} ariaLabel="About Arbor">
  {#snippet header()}
    <ModalHeader {onClose}>
      <div class="header-logo">
        <ArborLogo size={18} />
      </div>
      <span class="header-title">Arbor</span>
      {#if appInfo}
        <span class="header-version">v{appInfo.version}</span>
      {/if}
      <span class="header-sub">A modern, extensible Git client</span>
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

      <div class="group-label"><Cpu size={10} /> Technology</div>
      <div class="tech-strip">
        {#each techStack as t}
          <span class="tech-chip {t.cls}" use:tooltip={t.desc}>{t.badge}</span>
        {/each}
      </div>

      <div class="group-label"><Building2 size={10} /> Made by</div>
      <div class="publisher-card">
        <div class="publisher-mark">
          <span class="publisher-monogram">NS</span>
        </div>
        <div class="publisher-meta">
          <span class="publisher-name">Nightprint Studio</span>
          <button
            type="button"
            class="publisher-tag publisher-link"
            onclick={() => openExternal('https://github.com/nightprint-studio')}
            use:tooltip={'Open Nightprint Studio on GitHub'}
          >github.com/nightprint-studio</button>
          <button
            type="button"
            class="publisher-tag publisher-link"
            onclick={() => openExternal('mailto:nightprint.studio@gmail.com')}
            use:tooltip={'Send an email to Nightprint Studio'}
          >nightprint.studio@gmail.com</button>
        </div>
      </div>
    </div>

    <div class="col">
      <div class="group-label"><Keyboard size={10} /> Keyboard shortcuts</div>
      <div class="card">
        {#each shortcuts as s}
          <div class="card-row">
            <kbd>{s.keys}</kbd>
            <span class="row-val">{s.desc}</span>
          </div>
        {/each}
      </div>
      <p class="shortcuts-hint">
        Tip: press <kbd class="kbd-inline">Ctrl+K</kbd> to browse every action in the command palette.
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
  /* Logo is self-illustrated (full-color SVG); the surrounding chip
     styling that suited a single Lucide glyph would just box it in. */
  .header-logo {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .header-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: 0.01em;
  }

  .header-version {
    font-size: 10px;
    font-family: var(--font-code);
    color: var(--accent);
    background: var(--accent-subtle);
    border: 1px solid rgba(77,120,204,0.25);
    border-radius: var(--radius-sm);
    padding: 1px 5px;
    flex-shrink: 0;
  }

  .header-sub {
    flex: 1;
    font-size: 11px;
    color: var(--text-muted);
  }

  .about-body {
    display: flex;
    height: 100%;
    overflow-y: auto;
    font-family: var(--font-ui-sans);
  }

  .col {
    flex: 1;
    padding: 14px 18px 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .col:first-child {
    border-right: 1px solid var(--border);
  }

  .group-label {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.7px;
    color: var(--text-disabled);
    padding: 4px 0 0;
  }
  .group-label:first-child { padding-top: 0; }

  .card {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
    box-shadow: 0 1px 3px rgba(0,0,0,0.25);
  }

  .card-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--border);
  }
  .card-row:last-child { border-bottom: none; }
  .card-row:hover { background: rgba(255,255,255,0.03); }

  .row-key {
    font-size: 11px;
    color: var(--text-muted);
    flex: 0 0 88px;
  }

  .row-val {
    font-size: 11px;
    color: var(--text-secondary);
    flex: 1;
  }

  .tech-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    padding: 6px 2px 2px;
  }
  .tech-chip {
    display: inline-block;
    font-size: 10px;
    font-weight: 700;
    padding: 2px 9px;
    border-radius: 999px;
    border: 1px solid transparent;
    text-align: center;
    cursor: default;
  }
  .tech-chip.rust   { background: rgba(178,70,30,0.15);  color: #e07040; border-color: rgba(178,70,30,0.3); }
  .tech-chip.svelte { background: rgba(255,100,40,0.12); color: #ff6428; border-color: rgba(255,100,40,0.28); }
  .tech-chip.tauri  { background: var(--accent-subtle);  color: var(--accent); border-color: rgba(77,120,204,0.3); }
  .tech-chip.lib    { background: var(--bg-overlay);     color: var(--text-secondary); border-color: var(--border); }

  .publisher-card {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 10px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: 0 1px 3px rgba(0,0,0,0.25);
  }
  .publisher-mark {
    flex-shrink: 0;
    width: 34px;
    height: 34px;
    border-radius: var(--radius-sm);
    background: linear-gradient(135deg, #1a1f2e 0%, #0d1117 100%);
    border: 1px solid rgba(77,120,204,0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: inset 0 0 0 1px rgba(255,255,255,0.04);
  }
  .publisher-monogram {
    font-family: var(--font-code);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.5px;
    background: linear-gradient(135deg, #6b9eff 0%, #c8a8ff 100%);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .publisher-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .publisher-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: 0.01em;
  }
  .publisher-tag {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
  }

  /* Buttons styled identically to the publisher-tag spans, but clickable. */
  .publisher-link {
    background: none;
    border: none;
    padding: 0;
    margin: 0;
    text-align: left;
    cursor: pointer;
    transition: color 0.12s ease;
  }
  .publisher-link:hover {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .publisher-link:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
    border-radius: 2px;
  }

  kbd {
    display: inline-block;
    font-family: var(--font-code);
    font-size: 10px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    white-space: nowrap;
    flex-shrink: 0;
    min-width: 92px;
    text-align: center;
  }
  .kbd-inline {
    min-width: 0;
    padding: 0 5px;
    font-size: 9.5px;
    vertical-align: 1px;
  }

  .shortcuts-hint {
    margin: 8px 2px 0;
    font-size: 10.5px;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .about-footer {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    width: 100%;
    font-size: 10.5px;
    color: var(--text-muted);
    letter-spacing: 0.01em;
  }
  .about-footer .copyright {
    color: var(--text-secondary);
    font-weight: 500;
  }
  .about-footer .muted { color: var(--text-disabled); }

  .sep { opacity: 0.4; }
</style>
