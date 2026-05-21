<!--
  StudioModal — Generic shell for the per-format Studio modals
  (RON, JSON, TOML, YAML, .properties — ones to come).

  Owns the Modal wrapper invocation, the IDE-style view-mode tabs
  (Tree / Text / Diff / Errors — swappable via prop), the right
  activity rail with persisted rightPane state, the body layout
  (main card + optional bindings / query / schema sidecars), and the
  loading / error / empty StateBlock fallbacks.

  Everything format-specific is delegated via snippet props:
    · `headerLeft` — file icon, workspace tabs strip, "Open …" launcher.
    · `rightRailButtons` — the rail buttons themselves (the wrapper owns
      the routing because each pane decides its own dot / count).
    · `footerStatusLeft` — extras after the standard parse/dirty/saved
      pill (schema chip, encoding pill, refs index pill, selected path …).
    · `footerCenter` — undo / redo / indent / Format / Convert.
    · `footerRight` — Save split button.
    · `bodyMain({ viewMode })` — content for the active view mode.
    · `bindingsSidecar` / `schemaSidecar` / `querySidecar` — when the
      matching `rightPane` is open, the shell drops the snippet inside a
      slide-in `<aside>` chrome. Snippets render in the wrapper's scope
      so wrapper CSS still applies.
    · `auxiliary` — extras rendered after the modal (file pickers,
      view-source modals, cross-ref popovers …).

  Persistence: `rightPane` is mirrored to localStorage under
  `rightPaneStorageKey`, so reopening the modal restores the user's
  pick. `viewMode` is bindable but NOT persisted by the shell — the
  wrapper decides whether tab switches should reset it (RON does so
  because each tab has its own valid view state).
-->
<script lang="ts" generics="TKind extends string">
  import type { Snippet } from 'svelte';
  import { untrack } from 'svelte';
  import { slide, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { AlertCircle } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Modal from '../Modal.svelte';
  import StateBlock from '../ui/StateBlock.svelte';
  import Tabs, { type TabItem } from '../ui/Tabs.svelte';
  import ExperimentalBadge from '../ui/ExperimentalBadge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { StudioFormat, StudioBackend } from '$lib/ipc/studio-format';

  type ViewMode = 'tree' | 'text' | 'diff' | 'errors';
  type RightPane = 'inspector' | 'schema' | 'bindings' | 'query' | 'tools' | null;

  interface Props {
    formatId:                 StudioFormat;
    backend:                  StudioBackend<TKind>;
    open:                     boolean;
    loading?:                 boolean;
    loadingLabel?:            string;
    errorState?:              string | null;
    parseError?:              string | null;
    hasDoc?:                  boolean;
    /** View-mode tab items rendered in the header. */
    viewItems:                TabItem[];
    /** Bindable: caller usually mirrors this to drive `bodyMain`. */
    viewMode?:                ViewMode;
    /** Bindable: caller renders the matching sidecar snippet.
     *  The wrapper is responsible for the initial value (loaded from
     *  its own localStorage key) — the shell only persists changes
     *  back when `rightPaneStorageKey` is provided. */
    rightPane?:               RightPane;
    /** Optional localStorage key for the rightPane choice. When set,
     *  the shell mirrors every change back. The wrapper still owns
     *  the init read because $bindable defaults can't see props. */
    rightPaneStorageKey?:     string;
    width?:                   string;
    height?:                  string;
    ariaLabel?:               string;
    onClose:                  () => void | Promise<void>;
    /** Width in px of every right-side sidecar (bindings/schema/query).
     *  Default 320px matches the existing RON/JSON modals. */
    sidecarWidth?:            number;

    // ── Snippets ───────────────────────────────────────────────────
    /** Left part of the header strip — icon + title or workspace tabs
     *  + "Open …" launcher, plus (by convention) the per-doc undo /
     *  redo cluster that wrappers drop inline so it sits to the LEFT
     *  of their own tab strip. The view-mode tabs and the close
     *  button on the right edge are shell-owned. */
    headerLeft?:              Snippet<[]>;
    /** Right activity rail buttons. Wrapper owns the routing because
     *  each pane decides its own dot / count / tooltip. */
    rightRailButtons?:        Snippet<[]>;
    /** Footer status row — extras after the standard parse/dirty/saved
     *  pill rendered by the shell. */
    footerStatusLeft?:        Snippet<[]>;
    /** Footer center — undo / redo / indent / Format / Convert. */
    footerCenter?:            Snippet<[]>;
    /** Footer right — Save split button. */
    footerRight?:             Snippet<[]>;
    /** Body banner area (above the query bar — error alerts etc.). */
    bodyBanners?:             Snippet<[]>;
    /** Query bar — wrapper renders <StudioQueryBar> with format-specific
     *  snippets (kindChip / toolbarRight). Snippet renders inside the
     *  main card, above the view body. Pass the snippet only when the
     *  query bar should appear (typically: tree view + no parse error,
     *  but the wrapper decides). Named `queryBarSlot` (not `queryBar`)
     *  to avoid shadowing the wrapper's `bind:this` controller var of
     *  the same name. */
    queryBarSlot?:            Snippet<[]>;
    /** Main view content for the active view mode. The wrapper reads
     *  `viewMode` from its own bindable scope, so the snippet takes
     *  no arguments. */
    bodyMain?:                Snippet<[]>;
    /** Bindings sidecar content. Rendered when `rightPane === 'bindings'`. */
    bindingsSidecar?:         Snippet<[]>;
    /** Inspector sidecar content. Rendered when `rightPane === 'inspector'`.
     *  Lives next to the main card the same way bindings/schema/query
     *  do — full body height, never clipped by the query bar that sits
     *  inside the main card. The wrapper decides what to show when no
     *  node is selected (Inspector typically renders an empty state). */
    inspectorSidecar?:        Snippet<[]>;
    /** Schema sidecar content. Rendered when `rightPane === 'schema'`. */
    schemaSidecar?:           Snippet<[]>;
    /** Query results sidecar content. Rendered when `rightPane === 'query'`. */
    querySidecar?:            Snippet<[]>;
    /** Tools sidecar content (Format / Indent / Convert / …). Rendered
     *  when `rightPane === 'tools'`. Wrapper renders <StudioToolsSidebar>
     *  (the panel-shaped version) inside this snippet so the chrome
     *  matches the other sidecars. */
    toolsSidecar?:            Snippet<[]>;
    /** Auxiliary modals / popovers — rendered after the main Modal so
     *  they sit above it in DOM order (FilePicker / ViewSource / cross-
     *  ref picker portal …). */
    auxiliary?:               Snippet<[]>;
  }

  let {
    formatId,
    backend,
    open,
    loading = false,
    loadingLabel = 'Loading…',
    errorState = null,
    parseError = null,
    hasDoc = false,
    viewItems,
    viewMode = $bindable('tree'),
    rightPane = $bindable('inspector'),
    rightPaneStorageKey,
    width = 'min(1480px, 97vw)',
    height = 'min(960px, 94vh)',
    ariaLabel,
    onClose,
    sidecarWidth = 320,

    headerLeft,
    rightRailButtons,
    footerStatusLeft,
    footerCenter,
    footerRight,
    bodyBanners,
    queryBarSlot,
    bodyMain,
    bindingsSidecar,
    inspectorSidecar,
    schemaSidecar,
    querySidecar,
    toolsSidecar,
    auxiliary,
  }: Props = $props();

  // Touch the trait-bound props so the unused-prop warning stays quiet
  // for wrappers that don't read them inline. They're part of the API
  // surface for future capability hooks (e.g. rendering an icon for
  // the format from `backend.descriptor()` once the FE descriptor
  // type lands).
  void untrack(() => formatId); void untrack(() => backend);

  /** True once the user has explicitly switched the view at least
   *  once. The shell auto-flips to 'errors' on first parse failure
   *  only when this is false, so a deliberate selection isn't
   *  overruled by the next failed re-parse. */
  let userPickedView = $state(false);

  // The wrapper drives `viewMode` directly via `bind:viewMode`; we
  // expose `setView` only so the wrapper can opt in to the
  // user-picked tracking by routing user clicks through here.
  function onViewSelect(id: string) {
    viewMode = id as ViewMode;
    userPickedView = true;
  }

  /** Auto-jump to Errors view on first parse failure of the session.
   *  Mirrors RonStudioModal's existing behaviour, generalised. */
  $effect(() => {
    if (parseError && !userPickedView) viewMode = 'errors';
  });

  /** Reset the user-picked-view flag when the document closes so the
   *  next opened doc gets the auto-flip behaviour again. */
  $effect(() => {
    if (!hasDoc) userPickedView = false;
  });

  /** Persist rightPane on every change. localStorage write is cheap
   *  (sync, single value) so we don't bother debouncing. The shell
   *  only persists when the wrapper opted in by passing a storage
   *  key — the wrapper still owns the init read (see prop docs). */
  $effect(() => {
    if (!rightPaneStorageKey || typeof localStorage === 'undefined') return;
    try {
      if (rightPane === null) localStorage.removeItem(rightPaneStorageKey);
      else                    localStorage.setItem(rightPaneStorageKey, rightPane);
    } catch { /* ignore quota / private mode */ }
  });

  /** Used by the right-rail buttons (wrapper) to flip a pane open /
   *  closed. Exported so wrappers can call it via {bind:this}. */
  export function toggleRightPane(p: 'inspector' | 'schema' | 'bindings' | 'query' | 'tools') {
    rightPane = rightPane === p ? null : p;
  }

  /** Same with explicit set — used by query-bar onActiveChange to
   *  force-open the query sidecar without toggling. */
  export function setRightPane(p: RightPane) {
    rightPane = p;
  }
</script>

{#if open}
  <Modal
    {onClose}
    {width}
    {height}
    padBody={false}
    {ariaLabel}
  >
    {#snippet rightRail()}
      {@render rightRailButtons?.()}
    {/snippet}

    {#snippet header()}
      {@render headerLeft?.()}
      <ExperimentalBadge
        description="The Studio modals (RON · JSON · TOML · YAML · .properties) are still in active iteration — parse, schema and conversion behaviour may change between releases."
      />

      <!-- View switcher — shared <Tabs variant="solid"> with the
           wrapper-supplied items. Item content is rendered via the
           inline snippet so the Errors '!' chip can flip to red via
           the `errorBadge` data marker that wrappers can set. -->
      <Tabs
        variant="solid"
        size="sm"
        value={viewMode}
        items={viewItems}
        onSelect={onViewSelect}
        ariaLabel="View mode"
      >
        {#snippet itemContent({ item, active }: { item: TabItem; active: boolean })}
          {#if item.icon}{@const Ic = item.icon}<Ic size={14} />{/if}
          {#if item.label}<span class="tab-label">{item.label}</span>{/if}
          {#if item.badge !== undefined && item.badge !== null && item.badge !== ''}
            <span
              class="sm-view-badge"
              class:sm-view-badge-active={active}
              class:sm-view-badge-err={(item.data as { errorBadge?: boolean } | undefined)?.errorBadge}
            >{item.badge}</span>
          {/if}
        {/snippet}
      </Tabs>

      <!-- Close button on the right edge (Windows-style placement).
           The mac-close-btn visual stays — the red dot reads as
           "close" in either corner and matches the rest of arbor's
           modals. -->
      <button class="mac-close-btn sm-close-right" onclick={onClose} aria-label="Close" use:tooltip={'Close'}></button>
    {/snippet}

    {#snippet footer()}
      <!-- Left status row — wrapper-supplied. Convention: parse/dirty/
           saved pill is wrapper-side (it depends on the wrapper's
           dirty/saving signal). The shell keeps the layout shell. -->
      <div class="sm-footer-status">
        {@render footerStatusLeft?.()}
      </div>

      <div class="sm-spacer"></div>

      {@render footerCenter?.()}
      {@render footerRight?.()}
    {/snippet}

    {#if loading}
      <StateBlock tone="loading">
        {#snippet spinner()}<Spinner size="lg" label={loadingLabel} />{/snippet}
      </StateBlock>
    {:else if errorState}
      <StateBlock tone="error" label={errorState}>
        {#snippet icon()}<AlertCircle size={18} />{/snippet}
      </StateBlock>
    {:else if hasDoc}
      <div class="sm-body">
        <!-- Main card: view body. Banners + Query bar + the active
             view (Tree / Text / Diff / Errors) all live here, so the
             wrapper renders a single `bodyMain` snippet that knows
             which one to draw based on `viewMode`. -->
        <div class="sm-card sm-card-grow">
          {@render bodyBanners?.()}
          {@render queryBarSlot?.()}
          {@render bodyMain?.()}
        </div>

        <!-- Optional sidecars. Each renders only when the matching
             rightPane is selected. Slide-in transition mirrors the
             RonStudioModal flow. The shell defines the chrome
             (`<aside class="sm-card sm-side">`); content is the
             wrapper-supplied snippet. -->
        <!-- One aside for the WHOLE sidecar lifecycle.

             Layered transitions:
               · The outer `<aside>` uses `transition:slide` only at the
                 open ↔ closed boundary (null ↔ any pane). Single mount
                 / single unmount → no concurrent transitions on the
                 flex row → no layout race.
               · The inner content is wrapped in `{#key rightPane}` and
                 uses `transition:fly` so swapping between panes
                 (inspector → schema, etc.) animates the content alone.
                 The aside stays mounted at full width during the swap,
                 the body row layout never reflows mid-animation, and
                 the old / new content cross-fly inside the fixed
                 viewport.

             Result: identical "from the right" feel for open, close,
             AND swap. -->
        {#if rightPane !== null && (
          (rightPane === 'bindings'  && bindingsSidecar) ||
          (rightPane === 'inspector' && inspectorSidecar) ||
          (rightPane === 'query'     && querySidecar) ||
          (rightPane === 'schema'    && schemaSidecar)  ||
          (rightPane === 'tools'     && toolsSidecar)
        )}
          <aside class="sm-card sm-side"
                 style="width:{sidecarWidth}px"
                 transition:slide={{ axis: 'x', duration: animStore.dPanel, easing: cubicOut }}>
            <div class="sm-side-inner" style="width:{sidecarWidth}px">
              {#key rightPane}
                <div class="sm-side-content"
                     in:fly={{ x: sidecarWidth, duration: animStore.dPanel, easing: cubicOut }}
                     out:fly={{ x: sidecarWidth, duration: animStore.dPanel, easing: cubicOut }}>
                  {#if rightPane === 'bindings'}
                    {@render bindingsSidecar?.()}
                  {:else if rightPane === 'inspector'}
                    {@render inspectorSidecar?.()}
                  {:else if rightPane === 'query'}
                    {@render querySidecar?.()}
                  {:else if rightPane === 'schema'}
                    {@render schemaSidecar?.()}
                  {:else if rightPane === 'tools'}
                    {@render toolsSidecar?.()}
                  {/if}
                </div>
              {/key}
            </div>
          </aside>
        {/if}
      </div>
    {/if}
  </Modal>

  {@render auxiliary?.()}
{/if}

<style>
  /* Right-anchored close button — small left margin so it doesn't
     crowd the adjacent control (view-toggle / copy / etc.). */
  .sm-close-right { margin-left: 10px; }

  .sm-spacer { flex: 1; }

  /* Tone-aware badge inside the view-mode <Tabs variant="solid">.
     The `err` modifier flips to a red chip with white glyph so the
     "!" Errors indicator reads as a hard warning rather than an
     accent count. */
  .sm-view-badge {
    background: var(--accent);
    /* Theme-aware foreground so warm-accent themes (Ayu Dark yellow,
       Solarized, …) don't get unreadable white-on-yellow. */
    color: var(--text-on-accent);
    font-size: 9px;
    padding: 1px 5px;
    border-radius: 8px;
    margin-left: 3px;
    font-weight: 700;
    line-height: 1.3;
  }
  /* Active tab pill is filled with --accent, so flip the badge:
     background = the accent's foreground colour, text = accent.
     This gives a contrasting chip on every theme (white-on-blue,
     dark-on-yellow, …) without hard-coding #fff. */
  .sm-view-badge-active {
    background: var(--text-on-accent);
    color: var(--accent);
  }
  .sm-view-badge-err {
    background: var(--error, #e06c75);
    color: #fff;
    font-weight: 700;
  }
  .sm-view-badge-active.sm-view-badge-err {
    background: var(--text-on-accent);
    color: var(--error, #e06c75);
  }

  /* Footer status row — wrapper renders pills inside this flex line. */
  .sm-footer-status {
    display: inline-flex; align-items: center; gap: 8px;
    min-width: 0;
    font-size: 11px;
  }

  /* Body — horizontal flex row carrying floating cards (main + optional
     sidecars). The right activity rail sits flush against the modal's
     right edge via Modal's own `rightRail` snippet — this body row
     sits to the rail's left. */
  .sm-body {
    display: flex;
    flex-direction: row;
    height: 100%;
    overflow: hidden;
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
  }
  .sm-card {
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .sm-card-grow { flex: 1; min-height: 0; }
  .sm-side { flex-shrink: 0; }
  /* Pinned-width inner wrapper — keeps the sidecar's content at the
     full target width during the parent's slide animation. Without
     this, fluid sub-component internals (text, flex columns) shrink
     in lockstep with the aside's width and the visual reads as
     "content squishes left" instead of the desired "panel reveals
     from the right". The inner div doesn't shrink (`flex-shrink: 0`),
     so as the aside's width animates 320 ↔ 0 the overflow:hidden
     parent reveals/hides the content uniformly from its right edge.

     `position: relative` + child `position: absolute` lets the
     pane-swap fly-in / fly-out cross-animate over the same viewport
     instead of stacking vertically. */
  .sm-side-inner {
    height: 100%;
    flex-shrink: 0;
    position: relative;
    overflow: hidden;
  }
  .sm-side-content {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
</style>
