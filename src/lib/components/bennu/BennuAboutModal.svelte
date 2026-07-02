<script lang="ts">
  /**
   * BennuAboutModal — Bennu's "about" card (opened from the hamburger / help menu).
   *
   * Bennu-branded sibling of the Tyto/shared About modals: same two-column layout
   * language, its own identity (the self-generating firebird · Java editor) and its
   * own quick shortcut reference. Reuses the shared Modal chrome + Kbd widget so it
   * never drifts from the app's look, and opens external links with `openUrl` from
   * `@tauri-apps/plugin-opener` (never `window.open` — see the memory rule).
   *
   * SCAFFOLD PHASE: this is presentation only — pure identity/version/links. The
   * one genuinely dynamic fact (version + platform) comes from the real `getAppInfo`
   * IPC that already backs the other About modals. The quick-shortcut list is a
   * local seam (see BENNU_QUICK_SHORTCUTS) that should point at a canonical
   * `BENNU_SHORTCUTS` source once one exists — today Bennu's bindings live only in
   * the docs HTML, so we mirror them here and mark it.
   */
  import { Sparkles, Layers, Cpu, Keyboard, Building2, Github, BookOpen } from 'lucide-svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { getAppInfo, type AppInfo } from '$lib/ipc/app';
  import { tooltip } from '$lib/actions/tooltip';

  let { onClose }: { onClose: () => void } = $props();

  const RELEASE_YEAR = new Date().getFullYear();

  // Real backend metadata (version is the single source of truth from
  // tauri.conf.json) — same IPC the Tyto/Arbor About modals use.
  let appInfo = $state<AppInfo | null>(null);
  $effect(() => {
    getAppInfo().then((info) => { appInfo = info; }).catch(() => {});
  });

  const buildRows = $derived([
    { label: 'Version',  value: appInfo ? `v${appInfo.version}` : '—' },
    { label: 'Runtime',  value: 'Tauri 2 + Rust' },
    { label: 'Frontend', value: 'Svelte 5 Runes' },
    { label: 'Engine',   value: 'tree-sitter · fst+rkyv index' },
    { label: 'Platform', value: appInfo ? `${appInfo.os} · ${appInfo.arch}` : '—' },
  ]);

  // On-brand tech chips. Descriptions surface on hover via the shared tooltip.
  const techStack = [
    { badge: 'Rust',          cls: 'rust',   desc: 'Backend · semantic engine · indexer' },
    { badge: 'Tauri 2',       cls: 'tauri',  desc: 'Native shell · IPC layer' },
    { badge: 'tree-sitter',   cls: 'ts',     desc: 'Incremental Java / JSP parsing' },
    { badge: 'CodeMirror 6',  cls: 'cm',     desc: 'Editor surface · decorations' },
    { badge: 'fst + rkyv',    cls: 'index',  desc: 'Cross-file symbol index · zero-copy load' },
  ];

  // External links. Repo/docs endpoints are placeholders until Bennu ships its
  // own pages — shaped as a table so a future [bennu] config or IPC can supply
  // the real URLs without touching the markup.
  // MOCK — canonical Bennu URLs TBD; wire to real GitHub/docs once published.
  const links = [
    { id: 'github', label: 'GitHub',        url: 'https://github.com/nightprint-studio', icon: Github,   desc: 'Source & issues' },
    { id: 'docs',   label: 'Documentation', url: 'https://github.com/nightprint-studio', icon: BookOpen, desc: 'Guides & reference' },
  ];

  // MOCK — Bennu has no typed shortcut source (like TYTO_SHORTCUTS) yet; its
  // bindings live only in the docs (bennu/docs/Shortcuts.svelte). Mirror the most
  // common ones here so the About card gives a taste. Replace with a slice of a
  // canonical `BENNU_SHORTCUTS` array once one is extracted, so this never drifts.
  const BENNU_QUICK_SHORTCUTS: { keys: string[]; description: string }[] = [
    { keys: ['Ctrl', 'K'],          description: 'Command palette' },
    { keys: ['Ctrl', 'O'],          description: 'Open project' },
    { keys: ['Ctrl', 'G'],          description: 'Go to line' },
    { keys: ['Ctrl', 'F'],          description: 'Find in file' },
    { keys: ['Ctrl', 'Shift', 'F'], description: 'Find in project' },
    { keys: ['Ctrl', 'Space'],      description: 'Completions' },
    { keys: ['Alt', '1'],           description: 'Toggle Project' },
    { keys: ['Alt', '2'],           description: 'Toggle Structure' },
  ];

  async function openExternal(url: string) {
    // Never window.open — WebView2 opens a stranded child window (memory rule).
    try { await openUrl(url); } catch { /* ignore — external opener best-effort */ }
  }
</script>

<Modal {onClose} width="720px" height="520px" padBody={false} ariaLabel="About Bennu">
  {#snippet header()}
    <ModalHeader {onClose}>
      <div class="header-logo"><ArborLogo size={18} /></div>
      <span class="header-title">Bennu</span>
      {#if appInfo}<span class="header-version">v{appInfo.version}</span>{/if}
      <span class="header-sub">Java editor &amp; semantic engine</span>
    </ModalHeader>
  {/snippet}

  <div class="about-body">
    <div class="col">
      <div class="group-label"><Sparkles size={10} /> About the name</div>
      <p class="blurb">
        <b>Bennu</b> — the mythical self-generating firebird that is reborn from its
        own ashes: a program made to make other programs. A lean Java editor and
        cross-file semantic engine for legacy <b>Struts2</b>, <b>JSP</b>,
        <b>Entando</b> and <b>JDBC</b> stacks — the parts of a codebase IntelliJ
        makes you wade through, made fast and focused. Part of Nightprint Studio's
        bird-named toolkit alongside Corvus, Merula, Sitta and Tyto.
      </p>

      <div class="group-label"><Layers size={10} /> Build info</div>
      <div class="card">
        {#each buildRows as row (row.label)}
          <div class="card-row">
            <span class="row-key">{row.label}</span>
            <span class="row-val">{row.value}</span>
          </div>
        {/each}
      </div>

      <div class="group-label"><Cpu size={10} /> Technology</div>
      <div class="tech-strip">
        {#each techStack as t (t.badge)}
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
        {#each BENNU_QUICK_SHORTCUTS as s (s.description)}
          <div class="card-row">
            <span class="row-kbd"><Kbd keys={s.keys} size="sm" /></span>
            <span class="row-val">{s.description}</span>
          </div>
        {/each}
      </div>
      <p class="shortcuts-hint">
        Press <Kbd keys={['F1']} size="sm" /> any time for the full documentation.
      </p>

      <div class="group-label"><BookOpen size={10} /> Links</div>
      <div class="link-grid">
        {#each links as l (l.id)}
          <button
            type="button"
            class="link-tile"
            onclick={() => openExternal(l.url)}
            use:tooltip={l.desc}
          >
            <l.icon size={14} />
            <span class="link-label">{l.label}</span>
          </button>
        {/each}
      </div>
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
  .header-title { font-size: 13px; font-weight: 600; color: var(--text-primary); letter-spacing: 0.01em; }
  .header-version {
    font-size: 10px; font-family: var(--font-code); color: var(--accent);
    background: var(--accent-subtle); border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
    border-radius: var(--radius-sm); padding: 1px 5px; flex-shrink: 0;
  }
  .header-sub { flex: 1; font-size: 11px; color: var(--text-muted); }

  .about-body { display: flex; height: 100%; overflow-y: auto; font-family: var(--font-ui-sans); }
  .col { flex: 1; padding: 14px 18px 18px; display: flex; flex-direction: column; gap: 10px; }
  .col:first-child { border-right: 1px solid var(--border); }

  .group-label {
    display: flex; align-items: center; gap: 5px;
    font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.7px;
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
  .row-key { font-size: 11px; color: var(--text-muted); flex: 0 0 96px; }
  .row-val { font-size: 11px; color: var(--text-secondary); flex: 1; }
  .row-kbd { flex: 0 0 96px; display: flex; }

  .blurb { margin: 0; font-size: 11.5px; line-height: 1.6; color: var(--text-secondary); }
  .blurb b { color: var(--text-primary); }

  .tech-strip { display: flex; flex-wrap: wrap; gap: 5px; padding: 6px 2px 2px; }
  .tech-chip {
    display: inline-block; font-size: 10px; font-weight: 700;
    padding: 2px 9px; border-radius: 999px; border: 1px solid transparent;
    text-align: center; cursor: default;
  }
  .tech-chip.rust  { background: rgba(178,70,30,0.15);  color: #e07040; border-color: rgba(178,70,30,0.3); }
  .tech-chip.tauri { background: var(--accent-subtle);  color: var(--accent); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
  .tech-chip.ts    { background: rgba(80,160,110,0.14);  color: #57b98a; border-color: rgba(80,160,110,0.3); }
  .tech-chip.cm    { background: rgba(90,120,220,0.14);  color: #7d9bff; border-color: rgba(90,120,220,0.3); }
  .tech-chip.index { background: rgba(190,130,60,0.14);  color: #d6a058; border-color: rgba(190,130,60,0.3); }

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
    font-family: var(--font-code); font-size: 12px; font-weight: 700; letter-spacing: 0.5px;
    background: linear-gradient(135deg, #6b9eff 0%, #c8a8ff 100%);
    -webkit-background-clip: text; background-clip: text; color: transparent;
  }
  .publisher-meta { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .publisher-name { font-size: 12px; font-weight: 600; color: var(--text-primary); }
  .publisher-link {
    font-family: var(--font-code); font-size: 10px; color: var(--text-muted);
    background: none; border: none; padding: 0; text-align: left; cursor: pointer;
    transition: color 0.12s ease;
  }
  .publisher-link:hover { color: var(--accent); text-decoration: underline; text-underline-offset: 2px; }

  .shortcuts-hint {
    margin: 8px 2px 0; font-size: 10.5px; color: var(--text-muted); line-height: 1.7;
    display: flex; align-items: center; gap: 5px; flex-wrap: wrap;
  }

  .link-grid { display: flex; gap: 6px; padding: 2px 0; }
  .link-tile {
    flex: 1; display: flex; align-items: center; justify-content: center; gap: 6px;
    padding: 8px 10px; font-size: 11px; font-weight: 600; color: var(--text-secondary);
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); cursor: pointer;
    transition: color 0.12s ease, border-color 0.12s ease, background 0.12s ease;
  }
  .link-tile:hover {
    color: var(--accent); border-color: color-mix(in srgb, var(--accent) 40%, transparent);
    background: var(--accent-subtle);
  }
  .link-label { line-height: 1; }

  .about-footer {
    display: flex; align-items: center; justify-content: center; gap: 8px;
    width: 100%; font-size: 10.5px; color: var(--text-muted);
  }
  .about-footer .copyright { color: var(--text-secondary); font-weight: 500; }
  .about-footer .muted { color: var(--text-disabled); }
  .sep { opacity: 0.4; }
</style>
