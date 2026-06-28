<script lang="ts">
  /**
   * Arbor Canopy — the launcher shell (entry-point home, JetBrains-Toolbox-like).
   *
   * The Nightprint suite is drawn as a circuit-tree: one branch per product
   * (Corvus, Merula, Sitta), each a status-lit node. Selecting a node fills the
   * bottom detail footer; the primary action opens the product's real Arbor
   * window. Running state is driven by real window lifecycle events from the
   * backend (`onProductState` + `listRunningProducts`), and versions come from
   * the `versions.ts` seam (today the shared Arbor version, all up-to-date).
   * Self-contained dark aesthetic, starry backdrop, titlebar filter, toasts.
   */
  import {
    PRODUCT_WINDOW_OPENERS, closeProductWindow, listRunningProducts, onProductState,
  } from '$lib/ipc/app';
  import {
    BASE, GREEN, RUN, genStars, starShadow, geometry, decorate,
    type FilterKey, type DecoratedTool,
  } from './canopy';
  import { fetchInstalledVersions, fetchLatestVersions } from './versions';
  import { getLauncherConfig, setLauncherCloseToTray } from '$lib/ipc/config';
  import { Settings as SettingsIcon } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import CanopyBackground from './CanopyBackground.svelte';
  import CanopyBrand from './CanopyBrand.svelte';
  import CanopyTree from './CanopyTree.svelte';
  import CanopyFilterMenu from './CanopyFilterMenu.svelte';
  import CanopyToast from './CanopyToast.svelte';
  import DetailCard from './DetailCard.svelte';

  // ── State ──────────────────────────────────────────────────────────────────
  let filter = $state<FilterKey>('all');
  let sel = $state('corvus');
  let running = $state<Set<string>>(new Set());
  let installed = $state<Record<string, string>>({});
  let latest = $state<Record<string, string>>({});
  let toast = $state<{ msg: string; color: string } | null>(null);
  let hoverId = $state<string | null>(null);
  // Per-product "close reduces to tray" flags, keyed by product id.
  let closeToTray = $state<Record<string, boolean>>({});

  const ids = BASE.map(t => t.id);
  const starsShadow = starShadow(genStars(140));

  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  // ── Derived ────────────────────────────────────────────────────────────────
  const tools = $derived(BASE.map(t => decorate(t, {
    running: running.has(t.id),
    installed: installed[t.id] ?? '—',
    latest: latest[t.id] ?? installed[t.id] ?? '—',
  })));
  const geo = $derived(geometry('few', BASE.length));
  const card = $derived(tools.find(t => t.id === sel) ?? tools[0]);

  const chips = $derived.by(() => [
    { key: 'all' as FilterKey, label: 'Tutti', count: tools.length, color: GREEN, active: filter === 'all' },
    { key: 'running' as FilterKey, label: 'In esecuzione', count: tools.filter(t => t.isRun).length, color: RUN, active: filter === 'running' },
    { key: 'update' as FilterKey, label: 'Da aggiornare', count: tools.filter(t => t.isUpd).length, color: '#e8a857', active: filter === 'update' },
  ]);

  // ── Version + running-state wiring ───────────────────────────────────────────
  $effect(() => {
    let alive = true;
    void fetchInstalledVersions(ids).then(v => { if (alive) installed = v; });
    void fetchLatestVersions(ids).then(v => { if (alive) latest = v; });
    void listRunningProducts().then(list => { if (alive) running = new Set(list); });
    void getLauncherConfig().then(c => {
      if (!alive) return;
      const map: Record<string, boolean> = {};
      for (const [k, v] of Object.entries(c.products ?? {})) map[k] = v.close_to_tray;
      closeToTray = map;
    });

    const unlisten = onProductState(({ id, running: r }) => {
      const next = new Set(running);
      if (r) next.add(id); else next.delete(id);
      running = next;
    });
    return () => { alive = false; void unlisten.then(fn => fn()); };
  });

  // ── Actions ────────────────────────────────────────────────────────────────
  function fire(msg: string, color: string) {
    toast = { msg, color };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { toast = null; }, 2000);
  }

  function doAction(t: DecoratedTool) {
    if (t.kind === 'update') { void doUpdate(t); return; }
    const opener = PRODUCT_WINDOW_OPENERS[t.id];
    if (opener) {
      void opener();
      fire((t.isRun ? 'Apertura di ' : 'Avvio di ') + t.name + '…', t.accent);
    }
  }
  async function doUpdate(t: DecoratedTool) {
    // No update channel yet — re-check latest and report. Wired so that when a
    // release feed exists, a newer version here flips the node to "update".
    latest = await fetchLatestVersions(ids);
    fire('Nessun aggiornamento per ' + t.name, '#9aa3b2');
  }
  function doStop(t: { id: string; name: string }) {
    void closeProductWindow(t.id);
    fire(t.name + ' arrestato', '#9aa3b2');
  }
  // Single version per product today; selecting it is a no-op until a real
  // version-switch lands.
  function pickVer(_id: string, _v: string) {}

  // ── Launcher settings (gear menu) — per-product tray-close toggles ───────────
  async function toggleCloseToTray(id: string) {
    const next = !(closeToTray[id] ?? false);
    closeToTray = { ...closeToTray, [id]: next };
    try { await setLauncherCloseToTray(id, next); }
    catch { closeToTray = { ...closeToTray, [id]: !next }; } // revert on failure
  }
  const settingsMenu = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'Chiusura riduce a icona' },
    ...tools.map(t => ({
      kind: 'item' as const, id: `tray:${t.id}`, label: t.name,
      active: closeToTray[t.id] ?? false,
      onclick: () => toggleCloseToTray(t.id),
    })),
  ]);

  // ── Region focus navigation ─────────────────────────────────────────────────
  // F6 / Shift+F6 cycle focus across the three regions (titlebar → tree →
  // footer) so you don't have to Tab through everything; arrow keys then move
  // within a region (nodes / dropdown items).
  let topbarEl = $state<HTMLElement>();
  let treeEl = $state<HTMLElement>();
  let footerEl = $state<HTMLElement>();

  function regionOf(el: Element | null): number {
    if (!el) return -1;
    if (topbarEl?.contains(el)) return 0;
    if (treeEl?.contains(el)) return 1;
    if (footerEl?.contains(el)) return 2;
    return -1;
  }
  function focusRegion(idx: number) {
    if (idx === 0) (topbarEl?.querySelector('button, [tabindex="0"]') as HTMLElement | null)?.focus();
    else if (idx === 1) ((treeEl?.querySelector(`[data-node-id="${sel}"]`) ?? treeEl?.querySelector('[role="button"]')) as HTMLElement | null)?.focus();
    else if (idx === 2) (footerEl?.querySelector('button, [tabindex="0"]') as HTMLElement | null)?.focus();
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'F6') {
      e.preventDefault();
      const cur = regionOf(document.activeElement);
      focusRegion(((cur + (e.shiftKey ? -1 : 1)) + 3) % 3);
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="launcher">
  <CanopyBackground starShadow={starsShadow} />
  <div class="overlay"></div>

  <div class="content">
    <!-- top bar: brand · filter dropdown · settings -->
    <header class="topbar" bind:this={topbarEl}>
      <CanopyBrand />
      <div class="spacer" data-tauri-drag-region></div>
      <div class="tb-right">
        <CanopyFilterMenu {chips} onpick={(k) => { filter = k; }} />
        <div class="gear-dd">
          <Dropdown items={settingsMenu} selectionMode="multiple" position="fixed" direction="down" width="270px">
            {#snippet trigger({ toggle })}
              <Button variant="icon" title="Impostazioni" ariaLabel="Impostazioni" onclick={toggle}>
                <SettingsIcon size={16} />
              </Button>
            {/snippet}
          </Dropdown>
        </div>
      </div>
    </header>

    <!-- tree canvas -->
    <div class="tree" bind:this={treeEl}>
      <CanopyTree {geo} {tools} {sel} {filter} {hoverId}
                  onselect={(id) => { sel = id; }}
                  onactivate={(id) => { const t = tools.find(x => x.id === id); if (t) doAction(t); }}
                  onhover={(id) => { hoverId = id; }} />
    </div>

    <!-- detail footer (selected product) -->
    <div class="footer-region" bind:this={footerEl}>
      {#if card}
        <DetailCard tool={card}
                    onaction={() => doAction(card)} onstop={() => doStop(card)}
                    onpickVer={(v) => pickVer(card.id, v)} />
      {/if}
    </div>
  </div>

  {#if toast}
    <CanopyToast msg={toast.msg} color={toast.color} />
  {/if}
</div>

<style>
  /* Self-contained dark aesthetic (its own palette, not the app theme). Fonts
     fall back gracefully when Space Grotesk isn't installed. */
  .launcher {
    --canopy-display: 'Space Grotesk', var(--font-ui-sans);
    --canopy-sans: var(--font-ui-sans);
    --canopy-mono: var(--font-code);
    position: relative;
    width: 100%;
    height: 100vh;
    overflow: hidden;
    background: #06080d;
    -webkit-font-smoothing: antialiased;
  }
  .overlay {
    position: absolute; inset: 0; pointer-events: none;
    background: linear-gradient(180deg, rgba(6,8,13,0) 0%, rgba(6,8,13,0) 40%, rgba(6,8,13,.4) 74%, rgba(6,8,13,.85) 100%);
  }
  .content { position: relative; height: 100%; display: flex; flex-direction: column; z-index: 1; }

  .topbar { display: flex; align-items: center; gap: 8px; padding: 8px 8px 8px 12px; flex: none; }
  .spacer { flex: 1; align-self: stretch; min-width: 12px; }
  .tb-right { display: flex; align-items: center; gap: 6px; flex: none; }

  /* Theme-independent "sky" palette for the gear menu (see Dropdown's `--dd-*`
     hooks) so it matches the filter dropdown on the dark titlebar. */
  .gear-dd {
    display: inline-flex;
    --dd-bg: rgba(12, 16, 24, 0.96);
    --dd-border: rgba(255, 255, 255, 0.12);
    --dd-shadow: 0 18px 46px -16px rgba(0, 0, 0, 0.85);
    --dd-text: #c2cad6;
    --dd-text-muted: #9aa3b2;
    --dd-hover-bg: rgba(255, 255, 255, 0.06);
    --dd-active-bg: rgba(255, 255, 255, 0.09);
    --dd-check: #8fce6a;
  }

  .tree { flex: 1; min-height: 0; position: relative; }

  /* Global animation keyframes — `-global-` so Svelte doesn't scope-rename them
     (referenced by inline `animation:` in CanopyTree / CanopyBackground). */
  @keyframes -global-arbPulse { 0% { transform: scale(1); opacity: .6; } 75% { transform: scale(2); opacity: 0; } 100% { opacity: 0; } }
  @keyframes -global-arbSpin { to { transform: rotate(360deg); } }
  @keyframes -global-arbTwinkle { 0%,100% { opacity: .85; } 50% { opacity: .4; } }
</style>
