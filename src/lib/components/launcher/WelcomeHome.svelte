<script lang="ts">
  /**
   * Arbor's welcome page — the home tab of the tabbed container.
   *
   * NOT the Canopy launcher window: that one is its own small, self-contained
   * world (circuit-tree, starfield, its own palette) and stays as it is. This
   * lives INSIDE an Arbor window next to product tabs, so it wears the app's
   * chrome and theme tokens like any other panel.
   *
   * Shape follows the Corvus welcome — mark, name, primary actions — then two
   * sections in priority order: **Products** first, because "what do I start"
   * is the question this screen exists to answer, and **Recent projects**
   * under them, quieter. The content is capped and centred rather than
   * stretched: on a 1920px monitor the leftover is margin, not more columns.
   */
  import { FolderOpen, Download } from 'lucide-svelte';
  import TitleBarShell from '$lib/components/shared/ui/TitleBar.svelte';
  import WindowControls from '$lib/components/shared/WindowControls.svelte';
  import WorkspaceTabs from '$lib/components/shared/internal/WorkspaceTabs.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import WelcomeProductCard from './WelcomeProductCard.svelte';
  import { windowMenuItems } from '$lib/utils/window-menu';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { launcherState, type DecoratedTool, type RecentProject } from '$lib/stores/launcher/state.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { openProduct } from '$lib/utils/open-product';
  import { windowModeStore } from '$lib/stores/window-mode.svelte';
  import type { WindowMode } from '$lib/ipc/config';

  type ProductFilter = 'all' | 'running' | 'update';

  let filter = $state<ProductFilter>('all');
  let query  = $state('');

  $effect(() => launcherState.attach());

  const tools = $derived(launcherState.tools);
  const runningCount = $derived(tools.filter(t => t.isRunning).length);
  const updateCount  = $derived(tools.filter(t => t.isUpd).length);

  const shownTools = $derived(
    filter === 'running' ? tools.filter(t => t.isRunning)
    : filter === 'update' ? tools.filter(t => t.isUpd)
    : tools,
  );

  const shownRecents = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return launcherState.recents;
    return launcherState.recents.filter(
      r => r.name.toLowerCase().includes(q) || r.path.toLowerCase().includes(q),
    );
  });

  function launch(t: DecoratedTool) {
    launcherState.launch(t.id)
      .catch(e => uiStore.showToast(`Could not open ${t.name}: ${e}`, 'error'));
  }
  function openRecent(r: RecentProject) {
    launcherState.openRecent(r)
      .catch(e => uiStore.showToast(`Could not open ${r.name}: ${e}`, 'error'));
  }
  /** The two universal entry points; both are Git flows, so they go to Corvus. */
  function openProject() { void openProduct('corvus'); }
  function cloneRepo()   { void openProduct('corvus'); }

  const accentOf = (product: string) =>
    tools.find(t => t.id === product)?.accent ?? 'var(--accent)';

  /** Compact "when", the way a welcome screen wants it. */
  function whenLabel(openedAt: number): string {
    const days = Math.floor((Date.now() / 1000 - openedAt) / 86_400);
    if (days <= 0) return 'today';
    if (days === 1) return 'yesterday';
    if (days < 7)  return `${days}d ago`;
    if (days < 30) return `${Math.floor(days / 7)}w ago`;
    return new Date(openedAt * 1000).toLocaleDateString();
  }

  /** Flip between one-window-per-product and the tabbed container. It MUST be
   *  here: in tabbed mode this page is the entry point and the standalone
   *  launcher never shows, so this is the only way back to separate windows. */
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

  const settingsMenu = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'Products open in' },
    { kind: 'item', id: 'mode:windows', label: 'Their own window',
      active: !windowModeStore.tabbed, onclick: () => pickWindowMode('windows') },
    { kind: 'item', id: 'mode:tabbed', label: 'Tabs of one window',
      active: windowModeStore.tabbed, onclick: () => pickWindowMode('tabbed') },
    ...windowMenuItems(),
  ]);
</script>

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
      <div class="page">

        <header class="hero">
          <div class="hero-mark"><ArborLogo size={42} /></div>
          <div class="hero-ident">
            <h1>Arbor</h1>
            <p>Git · Java · Music · Files · Capture</p>
          </div>
          <div class="hero-actions">
            <Button variant="primary" size="sm" onclick={openProject}>
              {#snippet iconStart()}<FolderOpen size={14} />{/snippet}
              Open project…
            </Button>
            <Button variant="outline" size="sm" onclick={cloneRepo}>
              {#snippet iconStart()}<Download size={14} />{/snippet}
              Clone…
            </Button>
          </div>
        </header>

        <section class="sec">
          <div class="sec-head">
            <h2 class="sec-title">Products</h2>
            <div class="chips" role="group" aria-label="Filter products">
              <Button variant={filter === 'all' ? 'tonal' : 'outline'} size="xs"
                      onclick={() => filter = 'all'}>All {tools.length}</Button>
              <Button variant={filter === 'running' ? 'tonal' : 'outline'} size="xs"
                      onclick={() => filter = 'running'}>Running {runningCount}</Button>
              <Button variant={filter === 'update' ? 'tonal' : 'outline'} size="xs"
                      onclick={() => filter = 'update'}>Updates {updateCount}</Button>
            </div>
          </div>

          <div class="pgrid">
            {#each shownTools as t (t.id)}
              <WelcomeProductCard tool={t}
                                  onlaunch={() => launch(t)}
                                  onstop={() => void launcherState.stop(t.id)} />
            {:else}
              <p class="sec-empty">No product matches this filter.</p>
            {/each}
          </div>
        </section>

        <section class="sec">
          <div class="sec-head">
            <h2 class="sec-title">Recent projects</h2>
            <div class="sec-search">
              <Input bind:value={query} size="sm" clearable placeholder="Filter…" ariaLabel="Filter recent projects" />
            </div>
          </div>

          <div class="rgrid">
            {#each shownRecents as r (r.product + r.path)}
              <button type="button" class="rcard" onclick={() => openRecent(r)} title={r.path}>
                <Monogram name={r.name} color={accentOf(r.product)} size={26} />
                <span class="r-body">
                  <span class="r-name">{r.name}</span>
                  <span class="r-path">{r.path}</span>
                </span>
                <span class="r-when">{whenLabel(r.opened_at)}</span>
              </button>
            {:else}
              <div class="r-empty">
                <EmptyState
                  message={query ? 'No match' : 'Nothing opened yet'}
                  description={query
                    ? 'No recent project matches this filter.'
                    : 'Projects you open — repositories, Java projects, pieces — show up here, whichever product opened them.'} />
              </div>
            {/each}
          </div>
        </section>

        <footer class="foot">
          <span><Kbd action="open_repo" size="sm" /> open project</span>
          <span><Kbd action="new_surface_tab" size="sm" /> new tab</span>
          <span><Kbd action="switch_window" size="sm" /> switch window</span>
          <span><Kbd action="toggle_docs" size="sm" /> documentation</span>
        </footer>

      </div>
    </div>
  </div>
</div>

<style>
  /* Arbor's window shape: chrome on --bg-elevated, one floating panel on
     --bg-base with the usual 6px gutter, like every product window. */
  .welcome {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-elevated);
    overflow: hidden;
  }
  .panels { flex: 1; min-height: 0; padding: 0 6px 6px; display: flex; }
  .panel {
    flex: 1;
    min-height: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow-y: auto;
    /* The page reads at a comfortable size rather than at panel density: this
       is a screen you look at between tasks, not one you work in all day. */
    font-size: 13.5px;
  }

  /* Capped and centred — the page must not grow with the window. `min-height`
     pushes the shortcut footer to the bottom of the panel when the content is
     short, instead of leaving it floating mid-page. */
  .page {
    width: 100%;
    max-width: 1060px;
    min-height: 100%;
    margin: 0 auto;
    padding: 32px 28px 0;
    display: flex;
    flex-direction: column;
  }

  /* ── hero ── */
  .hero { display: flex; align-items: center; gap: 15px; }
  .hero-mark { display: flex; flex: none; }
  .hero-ident { min-width: 0; }
  .hero h1 {
    margin: 0;
    font-size: 23px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--text-primary);
  }
  .hero p { margin: 3px 0 0; font-size: 13.5px; color: var(--text-muted); }
  .hero-actions { margin-left: auto; display: flex; gap: 7px; flex: none; }

  /* ── sections ── */
  .sec { margin-top: 26px; }
  .sec-head { display: flex; align-items: center; gap: 10px; margin-bottom: 12px; }
  .sec-title {
    margin: 0;
    font-size: 12.5px;
    font-weight: 600;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .chips { display: flex; align-items: center; gap: 6px; }
  .sec-search { margin-left: auto; width: 220px; }
  .sec-empty { margin: 2px; font-size: 13px; color: var(--text-muted); }

  /* Fixed columns, NOT auto-fill: the cards keep the size they're designed at
     and a wide window gets margin instead of a sixth column. */
  .pgrid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .rgrid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }
  .r-empty { grid-column: 1 / -1; }

  @media (max-width: 880px) {
    .pgrid { grid-template-columns: repeat(2, 1fr); }
    .rgrid { grid-template-columns: 1fr; }
  }

  /* ── recents: deliberately quieter than the products above ── */
  .rcard {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 9px 11px;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: none;
    cursor: pointer;
    text-align: left;
    color: inherit;
    font: inherit;
    min-width: 0;
  }
  .rcard:hover {
    background: var(--bg-elevated);
    border-color: var(--border-subtle, var(--border));
  }
  .r-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .r-name {
    font-size: 13.5px;
    font-weight: 500;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Tail-first: the deepest folders identify a project, the drive letter never does. */
  .r-path {
    font-family: var(--font-code);
    font-size: 11.5px;
    color: var(--text-disabled);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: rtl;
    text-align: left;
  }
  .r-when { flex: none; font-size: 12px; color: var(--text-muted); }

  /* Sticky at the bottom of the panel: the shortcuts are a permanent reference,
     not the end of the document, so they shouldn't scroll away with the list.
     `margin-top: auto` keeps them pinned down when the page is short. */
  .foot {
    position: sticky;
    bottom: 0;
    margin-top: auto;
    padding: 13px 0 14px;
    border-top: 1px solid var(--border-subtle, var(--border));
    background: var(--bg-base);
    display: flex;
    flex-wrap: wrap;
    gap: 18px;
    font-size: 12px;
    color: var(--text-disabled);
  }
  .foot span { display: inline-flex; align-items: center; gap: 5px; }
</style>
