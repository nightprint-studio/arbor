<script lang="ts">
  /**
   * TytoMiniBar — the compact "mini" presentation of the Tyto window: a slim,
   * always-on-top quick-capture toolbar in the spirit of Windows' Win+Shift+S bar.
   *
   * The 56px bar carries the essentials — a source selector, mode (Record / Screenshot),
   * the capture button, plus audio / output / expand. The source selector opens a
   * **selection-method** menu (Display · Window · Smart · Rectangle · Freehand); each
   * method opens the on-screen overlay in that mode, where the target is picked directly
   * (windows/monitors highlight in blue on hover; smart/rect/free draw over a frozen snap).
   *
   * A dropdown popup can't paint outside the 56px strip (WebView2 clips to the window),
   * so instead of a floating popup the menu is an **inline panel** that fills a
   * temporarily-grown window (`setTytoMiniMenu`): the bar stays put and the only new
   * thing on screen is the menu itself — no empty "grown window" showing.
   */
  import { Monitor, AppWindow, Crop, Square, PenTool, MousePointer2, Video, Camera, Circle, Maximize2, Volume2, VolumeX, Mic, MicOff, FolderOpen, ChevronDown } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { recorderStore, type CaptureMode } from '$lib/stores/tyto/recorder.svelte';
  import { tytoUiStore } from '$lib/stores/tyto/ui.svelte';
  import { setTytoMiniMenu } from '$lib/ipc/tyto/main-window';

  const modes: { mode: CaptureMode; icon: typeof Video; label: string }[] = [
    { mode: 'record',     icon: Video,  label: 'Record' },
    { mode: 'screenshot', icon: Camera, label: 'Screenshot' },
  ];

  // The 5 selection methods. Every one opens the on-screen overlay in that mode, where
  // the target is picked directly (windows/monitors highlight on hover; smart/rect/free
  // draw over a frozen snapshot). No sub-lists.
  type Method = 'display' | 'window' | 'smart' | 'rect' | 'free';
  const methods: { id: Method; icon: typeof Monitor; label: string; hint: string }[] = [
    { id: 'display', icon: Monitor,        label: 'Display',   hint: 'Pick a whole monitor' },
    { id: 'window',  icon: AppWindow,      label: 'Window',    hint: 'Pick one app window' },
    { id: 'smart',   icon: MousePointer2,  label: 'Smart',     hint: 'Snap to a UI element' },
    { id: 'rect',    icon: Square,         label: 'Rectangle', hint: 'Drag a box' },
    { id: 'free',    icon: PenTool,        label: 'Freehand',  hint: 'Trace a shape' },
  ];

  // Icon shown on the source trigger, reflecting the current target kind. Capitalized
  // so it can be used directly as a dynamic component in the markup.
  const SrcIcon = $derived(
    recorderStore.targetKind === 'monitor' ? Monitor
    : recorderStore.targetKind === 'window' ? AppWindow
    : Crop,
  );

  // ── Inline menu state ────────────────────────────────────────────────────────
  let menuOpen = $state(false);
  let panelEl = $state<HTMLDivElement | null>(null);
  let panelInnerEl = $state<HTMLDivElement | null>(null);

  function openMenu() { menuOpen = true; }
  function closeMenu() {
    if (!menuOpen) return;
    menuOpen = false;
    void setTytoMiniMenu(false).catch(() => {}); // shrink back to the 56px bar
  }
  function toggleMenu() { if (menuOpen) closeMenu(); else openMenu(); }

  // Grow the window to EXACTLY fit the bar (56px) + the measured menu content, so the
  // only new thing on screen is the menu — no empty grown area below it. The backend
  // clamps the height, so a long list scrolls inside instead of growing past the screen.
  $effect(() => {
    if (!menuOpen || !panelInnerEl) return;
    const BAR = 56, CHROME = 14; // panel padding + border
    void setTytoMiniMenu(true, BAR + panelInnerEl.offsetHeight + CHROME).catch(() => {});
  });

  // Every method opens the on-screen overlay directly in that mode; the target is picked
  // there (window/display hover-highlight; smart/rect/free draw over a frozen snapshot).
  function pickMethod(id: Method) {
    closeMenu();
    void recorderStore.openCaptureSelector(id);
  }

  // Mic on/off toggle: off ⇒ null; on ⇒ the default mic (or the first available).
  function toggleMic() {
    if (recorderStore.micId !== null) { recorderStore.setMic(null); return; }
    const d = recorderStore.mics.find((m) => m.default) ?? recorderStore.mics[0];
    recorderStore.setMic(d ? d.id : null);
  }

  function capture() {
    if (recorderStore.mode === 'record') recorderStore.startRecording();
    else recorderStore.takeScreenshot();
  }

  const disabled = $derived(!recorderStore.targetReady);

  // Focus the panel when it opens (keyboard-first: Tab cycles the rows, Esc closes).
  $effect(() => { if (menuOpen && panelEl) panelEl.querySelector<HTMLButtonElement>('button')?.focus(); });

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape' && menuOpen) { e.preventDefault(); closeMenu(); }
  }
  // Clicking outside the mini window (desktop / another app) blurs it → close the menu.
  function onWindowBlur() { closeMenu(); }
</script>

<svelte:window onkeydown={onKeyDown} onblur={onWindowBlur} />

<div class="mini">
  <div class="bar" data-tauri-drag-region>
    <div class="grip" data-tauri-drag-region aria-hidden="true"><span></span><span></span></div>

    <button
      type="button"
      class="src"
      class:open={menuOpen}
      onclick={toggleMenu}
      use:tooltip={'Capture source & method'}
      aria-haspopup="menu"
      aria-expanded={menuOpen}
      aria-label="Capture source and method"
    >
      <SrcIcon size={15} />
      <span class="src-label">{recorderStore.currentTargetLabel}</span>
      <ChevronDown size={13} />
    </button>

    <span class="div"></span>

    <div class="seg" role="group" aria-label="Mode">
      {#each modes as m (m.mode)}
        {@const Icon = m.icon}
        <button
          type="button"
          class="ib"
          class:on={recorderStore.mode === m.mode}
          onclick={() => recorderStore.setMode(m.mode)}
          use:tooltip={m.label}
          aria-pressed={recorderStore.mode === m.mode}
          aria-label={m.label}
        >
          <Icon size={16} />
        </button>
      {/each}
    </div>

    <button
      type="button"
      class="go"
      class:rec={recorderStore.mode === 'record'}
      onclick={capture}
      {disabled}
      use:tooltip={{ content: recorderStore.mode === 'record' ? 'Start recording' : 'Take screenshot', shortcut: 'Ctrl+Enter' }}
    >
      {#if recorderStore.mode === 'record'}<Circle size={12} fill="currentColor" /> Record{:else}<Camera size={13} /> Shot{/if}
    </button>

    <div class="spacer"></div>

    {#if recorderStore.mode === 'record'}
      <button
        type="button"
        class="ib"
        class:on={recorderStore.micId !== null}
        onclick={toggleMic}
        use:tooltip={{ content: recorderStore.micId !== null ? 'Microphone: on' : 'Microphone: off' }}
        aria-pressed={recorderStore.micId !== null}
        aria-label="Toggle microphone"
      >
        {#if recorderStore.micId !== null}<Mic size={16} />{:else}<MicOff size={16} />{/if}
      </button>
      <button
        type="button"
        class="ib"
        class:on={recorderStore.systemAudio}
        onclick={() => recorderStore.toggleSystemAudio()}
        use:tooltip={{ content: recorderStore.systemAudio ? 'System audio: on' : 'System audio: off', shortcut: 'Ctrl+Shift+A' }}
        aria-pressed={recorderStore.systemAudio}
        aria-label="Toggle system audio"
      >
        {#if recorderStore.systemAudio}<Volume2 size={16} />{:else}<VolumeX size={16} />{/if}
      </button>
    {/if}

    <button
      type="button"
      class="ib"
      onclick={() => recorderStore.revealOutputFolder()}
      use:tooltip={{ content: 'Open the output folder', shortcut: 'Ctrl+Shift+O' }}
      aria-label="Open the output folder"
    >
      <FolderOpen size={15} />
    </button>

    <button
      type="button"
      class="ib expand"
      onclick={() => tytoUiStore.setCompact(false)}
      use:tooltip={{ content: 'Expand to full window', shortcut: 'Ctrl+Shift+C' }}
      aria-label="Expand to full window"
    >
      <Maximize2 size={14} />
    </button>
  </div>

  {#if menuOpen}
    <div class="panel" bind:this={panelEl} role="menu" tabindex="-1" aria-label="Capture method">
     <div class="panel-inner" bind:this={panelInnerEl}>
      {#each methods as m (m.id)}
        {@const Icon = m.icon}
        <button type="button" class="row" onclick={() => pickMethod(m.id)} role="menuitem">
          <Icon size={16} />
          <span class="row-body">
            <span class="row-label">{m.label}</span>
            <span class="row-hint">{m.hint}</span>
          </span>
        </button>
      {/each}
     </div>
    </div>
  {/if}
</div>

<style>
  .mini {
    position: fixed; inset: 0;
    box-sizing: border-box;
    display: flex; flex-direction: column;
    background: var(--bg-elevated);
    color: var(--text-primary);
    user-select: none; -webkit-user-select: none;
    /* A soft bluish accent border so the floating toolbar reads distinctly, instead
       of relying on a barely-visible black drop shadow. */
    border: 1px solid color-mix(in srgb, var(--accent) 55%, var(--border));
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05),
                0 0 0 1px color-mix(in srgb, var(--accent) 14%, transparent);
    overflow: hidden;
  }

  /* The bar row keeps its exact 56px look whether or not the menu is open. */
  .bar {
    height: 56px; flex-shrink: 0;
    display: flex; align-items: center; gap: 8px;
    padding: 0 8px;
  }

  .grip {
    display: flex; flex-direction: column; gap: 3px;
    padding: 0 4px; flex-shrink: 0; cursor: grab;
  }
  .grip span { width: 3px; height: 3px; border-radius: 50%; background: var(--text-muted); opacity: 0.6; }

  .seg { display: flex; align-items: center; gap: 2px; }
  .div { width: 1px; height: 22px; background: var(--border); flex-shrink: 0; }

  /* Source trigger — shows the current source (icon + label) and opens the method menu. */
  .src {
    display: inline-flex; align-items: center; gap: 7px;
    height: 32px; max-width: 210px; padding: 0 8px 0 10px; flex-shrink: 0;
    border: 1px solid var(--border); border-radius: var(--radius-sm); cursor: pointer;
    background: var(--bg-input, var(--bg-hover)); color: var(--text-primary);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .src:hover, .src.open { background: var(--bg-hover); border-color: color-mix(in srgb, var(--accent) 45%, var(--border)); }
  .src :global(svg:first-child) { color: var(--accent); flex-shrink: 0; }
  .src-label {
    flex: 1; min-width: 0; font-size: 12.5px; font-weight: 550;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  .ib {
    display: inline-flex; align-items: center; justify-content: center;
    width: 32px; height: 32px; flex-shrink: 0;
    border: none; border-radius: var(--radius-sm); cursor: pointer;
    background: transparent; color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ib:hover { background: var(--bg-hover); color: var(--text-primary); }
  .ib.on { background: var(--accent-subtle); color: var(--accent); }
  .spacer { flex: 1; min-width: 8px; }

  .go {
    display: inline-flex; align-items: center; justify-content: center; gap: 6px;
    height: 32px; padding: 0 14px; flex-shrink: 0;
    border: none; border-radius: 999px; cursor: pointer;
    background: var(--accent); color: var(--text-on-accent, #fff);
    font-size: 12.5px; font-weight: 650;
    transition: filter var(--transition-fast);
  }
  .go.rec { background: var(--error); color: #fff; }
  .go:hover:not(:disabled) { filter: brightness(1.1); }
  .go:disabled { opacity: 0.5; cursor: default; }

  /* ── Inline method panel (fills the temporarily-grown window) ────────────────── */
  /* .panel fills whatever height the window grew to (flex:1) and scrolls if a long
     list was clamped; .panel-inner is the content we measure to size the window so it
     hugs the menu exactly (no empty strip). */
  .panel {
    flex: 1; min-height: 0; overflow-y: auto; outline: none;
    border-top: 1px solid var(--border);
  }
  .panel-inner {
    display: flex; flex-direction: column; gap: 2px;
    padding: 6px;
  }
  .row {
    display: flex; align-items: center; gap: 10px;
    width: 100%; padding: 8px 10px;
    border: none; border-radius: var(--radius-sm); cursor: pointer;
    background: transparent; color: var(--text-primary); text-align: left;
    transition: background var(--transition-fast);
  }
  .row:hover, .row:focus-visible { background: var(--bg-hover); outline: none; }
  .row :global(svg:first-child) { color: var(--text-secondary); flex-shrink: 0; }
  .row-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .row-label { font-size: 12.5px; font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-hint { font-size: 10.5px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
