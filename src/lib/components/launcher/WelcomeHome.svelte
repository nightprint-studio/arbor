<script lang="ts">
  /**
   * Arbor's welcome page — the home tab of the tabbed container.
   *
   * NOT the Canopy launcher window: that one is its own small, self-contained world (circuit-tree, starfield, its own
   * palette) and stays as it is. This lives INSIDE an Arbor window next to product tabs, so it wears the app's chrome
   * and theme tokens like any other panel.
   *
   * Top to bottom, from "where to?" to "what was I doing": a greeting that types itself and a prompt that waits, one
   * search box for projects, products and actions, a card to continue the last project, the two other ways in, the
   * products as a strip of panels, and the recent projects. `/` searches, 1–9 open a recent. The content is capped and
   * centred rather than stretched.
   */
  import TitleBarShell from '$lib/components/shared/ui/TitleBar.svelte';
  import WindowControls from '$lib/components/shared/WindowControls.svelte';
  import WorkspaceTabs from '$lib/components/shared/internal/WorkspaceTabs.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import McpSettingsModal from '$lib/components/launcher/mcp/McpSettingsModal.svelte';
  import { mcpMenuItems } from '$lib/components/launcher/mcp/menu';
  import { windowMenuItems } from '$lib/utils/window-menu';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { launcherState, type DecoratedTool, type RecentProject } from '$lib/stores/launcher/state.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { openProduct } from '$lib/utils/open-product';
  import { windowModeStore } from '$lib/stores/window-mode.svelte';
  import type { WindowMode } from '$lib/ipc/config';
  import WelcomeGreeting from './welcome/WelcomeGreeting.svelte';
  import WelcomeSearch from './welcome/WelcomeSearch.svelte';
  import WelcomeResume from './welcome/WelcomeResume.svelte';
  import WelcomeActions from './welcome/WelcomeActions.svelte';
  import WelcomeProducts from './welcome/WelcomeProducts.svelte';
  import WelcomeRecents from './welcome/WelcomeRecents.svelte';
  import { promptPhrases, recentKey, type SearchEntry } from './welcome/welcome-logic';

  $effect(() => launcherState.attach());

  const tools = $derived(launcherState.tools);
  const recents = $derived(launcherState.recents);
  const last = $derived(recents.length ? recents[0] : null);
  const productNames = $derived(Object.fromEntries(tools.map((t) => [t.id, t.name])));

  function launch(t: DecoratedTool) {
    launcherState.launch(t.id)
      .catch((e) => uiStore.showToast(`Could not open ${t.name}: ${e}`, 'error'));
  }
  function openRecent(r: RecentProject) {
    launcherState.openRecent(r)
      .catch((e) => uiStore.showToast(`Could not open ${r.name}: ${e}`, 'error'));
  }
  /** The two universal entry points; both are Git flows, so they go to Corvus. */
  function openProject() { void openProduct('corvus'); }
  function cloneRepo()   { void openProduct('corvus'); }

  // ── search ──────────────────────────────────────────────────────────────────────────────────────
  let search = $state<ReturnType<typeof WelcomeSearch>>();
  let recentsView = $state<ReturnType<typeof WelcomeRecents>>();

  const entries = $derived<SearchEntry[]>([
    ...recents.map((r) => ({ id: `p:${recentKey(r)}`, title: r.name, detail: r.path, kind: 'project' as const })),
    ...tools.map((t) => ({ id: `t:${t.id}`, title: t.name, detail: t.role, kind: 'product' as const })),
    { id: 'a:open', title: 'Open project…', detail: 'a repository or a folder', kind: 'action' },
    { id: 'a:clone', title: 'Clone repository…', detail: 'from a URL or a provider', kind: 'action' },
  ]);
  const productOf = (e: SearchEntry) =>
    e.id.startsWith('t:') ? e.id.slice(2)
    : e.id.startsWith('p:') ? (recents.find((r) => `p:${recentKey(r)}` === e.id)?.product ?? null)
    : null;

  function pick(e: SearchEntry) {
    if (e.id === 'a:open') openProject();
    else if (e.id === 'a:clone') cloneRepo();
    else if (e.id.startsWith('t:')) { const t = tools.find((x) => x.id === e.id.slice(2)); if (t) launch(t); }
    else { const r = recents.find((x) => `p:${recentKey(x)}` === e.id); if (r) openRecent(r); }
  }

  // `/` anywhere on the page searches; 1–9 open the n-th recent. Neither fires while you are typing somewhere.
  function onKeydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const el = e.target as HTMLElement | null;
    if (el?.closest('input, textarea, [contenteditable="true"]')) return;
    if (!surfaceStore.hasFocus('home')) return;
    if (e.key === '/') { e.preventDefault(); search?.focus(); }
    else if (/^[1-9]$/.test(e.key) && recentsView?.openNth(Number(e.key))) e.preventDefault();
  }

  /** Flip between one-window-per-product and the tabbed container. It MUST be here: in tabbed mode this page is the
   *  entry point and the standalone launcher never shows, so this is the only way back to separate windows. */
  async function pickWindowMode(mode: WindowMode) {
    if (windowModeStore.mode === mode) return;
    try {
      await windowModeStore.set(mode);
      uiStore.showToast(
        mode === 'tabbed' ? 'Products open as tabs' : 'Products open in their own window',
        'info',
      );
    } catch (e) {
      uiStore.showToast(`Setting not saved: ${e}`, 'error');
    }
  }

  /** The AI tool surface's settings. It MUST be reachable from here for the same reason the window-mode switch is: in
   *  tabbed mode this page is the entry point and the Canopy launcher — the only other place that offers it — never shows. */
  let mcpOpen = $state(false);

  const settingsMenu = $derived<DropdownItem[]>([
    ...mcpMenuItems(() => (mcpOpen = true)),
    { kind: 'separator', label: 'Products open in' },
    { kind: 'item', id: 'mode:windows', label: 'Their own window',
      active: !windowModeStore.tabbed, onclick: () => pickWindowMode('windows') },
    { kind: 'item', id: 'mode:tabbed', label: 'Tabs of one window',
      active: windowModeStore.tabbed, onclick: () => pickWindowMode('tabbed') },
    ...windowMenuItems(),
  ]);
</script>

<svelte:window onkeydown={onKeydown} />

<div class="welcome">
  <TitleBarShell
    logoTooltip="Arbor"
    settings={{ menu: settingsMenu, menuWidth: '250px' }}
    nativeMenuEnabled={surfaceStore.hasFocus('home')}
  >
    {#snippet logo()}<ArborLogo size={22} />{/snippet}
    {#snippet center()}<WorkspaceTabs />{/snippet}
    {#snippet windowControls()}<WindowControls />{/snippet}
  </TitleBarShell>

  <div class="panels">
    <div class="panel">
      <div class="dots" aria-hidden="true"></div>
      <div class="page">

        <div class="rise" style="--i:0"><WelcomeGreeting phrases={promptPhrases(last?.name ?? null)} /></div>

        <div class="rise search" style="--i:1">
          <WelcomeSearch bind:this={search} {entries} {productOf} onpick={pick} />
        </div>

        {#if last}
          <div class="rise" style="--i:2">
            <WelcomeResume project={last} productName={productNames[last.product] ?? last.product} onopen={() => openRecent(last)} />
          </div>
        {/if}

        <div class="rise" style="--i:3"><WelcomeActions onopen={openProject} onclone={cloneRepo} /></div>

        <section class="sec rise" style="--i:4">
          <div class="head"><h2>Products</h2><span class="hint"><Kbd keys={['←']} size="sm" /> <Kbd keys={['→']} size="sm" /> browse</span></div>
          <WelcomeProducts
            {tools}
            initial={last?.product ?? null}
            onlaunch={launch}
            onstop={(t) => launcherState.stop(t.id)}
            onrestart={(t) => launcherState.restart(t.id)}
          />
        </section>

        <section class="sec rise" style="--i:5">
          <WelcomeRecents
            bind:this={recentsView}
            {recents}
            {productNames}
            onopen={openRecent}
            onpin={(r, pinned) => void launcherState.pinRecent(r, pinned)}
            onforget={(r) => void launcherState.forgetRecent(r)}
          />
        </section>

        <footer class="foot">
          <span><Kbd keys={['/']} size="sm" /> search</span>
          <span><Kbd action="open_repo" size="sm" /> open project</span>
          <span><Kbd action="new_surface_tab" size="sm" /> new tab</span>
          <span><Kbd action="switch_window" size="sm" /> switch window</span>
          <span><Kbd action="toggle_docs" size="sm" /> documentation</span>
        </footer>

      </div>
    </div>
  </div>
</div>

{#if mcpOpen}
  <McpSettingsModal onClose={() => (mcpOpen = false)} />
{/if}

<style>
  /* Arbor's window shape: chrome on --bg-elevated, one floating panel on --bg-base with the usual 6px gutter. */
  .welcome { height: 100vh; display: flex; flex-direction: column; background: var(--bg-elevated); overflow: hidden; }
  .panels { flex: 1; min-height: 0; padding: 0 6px 6px; display: flex; }
  .panel {
    position: relative; flex: 1; min-height: 0; border-radius: var(--radius-lg); overflow-y: auto;
    /* the pools of colour are the products' own palette, not one blue; no blur anywhere */
    background:
      radial-gradient(900px 420px at 8% -6%, color-mix(in srgb, var(--info) 15%, transparent), transparent 62%),
      radial-gradient(760px 380px at 94% 4%, color-mix(in srgb, var(--warning) 12%, transparent), transparent 60%),
      radial-gradient(900px 480px at 50% 112%, color-mix(in srgb, var(--success) 10%, transparent), transparent 62%),
      var(--bg-base);
    font-size: var(--font-size-md);
  }
  /* a dot grid that fades toward the edges and drifts very slowly */
  .dots {
    position: absolute; inset: 0; pointer-events: none;
    background-image: radial-gradient(color-mix(in srgb, var(--text-primary) 10%, transparent) 1px, transparent 1.2px);
    background-size: 24px 24px;
    mask-image: radial-gradient(ellipse 70% 60% at 50% 22%, #000, transparent 75%);
    animation: drift 40s linear infinite;
  }
  @keyframes drift { to { background-position: 24px 24px; } }

  /* Capped and centred — the page must not grow with the window. `min-height` pushes the shortcut footer to the bottom of
     the panel when the content is short. */
  .page { position: relative; width: 100%; max-width: 1040px; min-height: 100%; margin: 0 auto; padding: 44px 28px 0; display: flex; flex-direction: column; }

  /* each block rises in turn; the duration is the app's animation token, so switching animations off leaves the page simply there */
  @keyframes rise { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: none; } }
  .rise { animation: rise calc(var(--anim-dur-slow) * 2.4) cubic-bezier(0.2, 0.7, 0.2, 1) both; animation-delay: calc(var(--i, 0) * var(--anim-dur-fast) * 0.75); }
  @media (prefers-reduced-motion: reduce) { .rise, .dots { animation: none; } }

  /* each `.rise` block is a stacking context of its own while it animates (and after: the animation fills forwards), and later blocks
     paint over earlier ones: the search results, which drop down over the blocks below, need their block above them */
  .search { position: relative; z-index: 30; }
  .sec { margin-top: 34px; }
  .head { display: flex; align-items: center; gap: 12px; margin-bottom: 12px; }
  h2 { margin: 0; font-size: var(--font-size-sm); font-weight: 600; letter-spacing: 0.07em; text-transform: uppercase; color: var(--text-muted); }
  .hint { margin-left: auto; display: inline-flex; align-items: center; gap: 4px; font-size: var(--font-size-xs); color: var(--text-disabled); }

  /* Sticky at the bottom of the panel: the shortcuts are a permanent reference, not the end of the document. */
  .foot {
    position: sticky; bottom: 0; margin-top: auto; padding: 13px 0 14px; display: flex; flex-wrap: wrap; gap: 18px;
    font-size: var(--font-size-sm); color: var(--text-disabled); background: linear-gradient(transparent, var(--bg-base) 40%);
  }
  .foot span { display: inline-flex; align-items: center; gap: 5px; }
  .sec:last-of-type { margin-bottom: 40px; }
  @media (max-width: 640px) { .page { padding: 28px 16px 0; } }
</style>
