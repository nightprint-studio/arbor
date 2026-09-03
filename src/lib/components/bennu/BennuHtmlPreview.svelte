<script lang="ts">
  /**
   * An HTML file, rendered.
   *
   * ## One question, and it is about scripts
   *
   * Rendering asks nothing: the frame is sandboxed, so a page that only lays itself out can do
   * nothing to anybody, and a dialog in front of every preview would be a dialog nobody reads.
   * **Running the page's own scripts** is the decision worth stopping for, and it is asked once
   * per file — with the answer remembered when the reader says so, because a report you open
   * every morning should not ask every morning.
   *
   * ## What the sandbox actually stops
   *
   * The frame is `sandbox`ed and — this is the load-bearing part — **never with
   * `allow-same-origin`**. Without it the frame has an opaque origin: it cannot read the app's
   * DOM, its storage, or anything Arbor holds, whatever it runs. `allow-scripts` alone is safe in
   * that sense, which is why the two consents are separate: rendering the layout of a page is a
   * far smaller thing to agree to than letting its JavaScript run, and most pages you want to look
   * at do not need the second.
   *
   * Content goes in through `srcdoc` rather than as a `file://` / asset URL, so the frame's origin
   * is opaque by construction rather than by configuration. A `<base>` is injected so the page's
   * own stylesheets, images and scripts resolve beside it — that is what makes the preview show
   * the page rather than its skeleton. Those siblings arrive over Tauri's **asset protocol**,
   * which serves only the extensions listed in `tauri.conf.json` (`assetProtocol.scope`): a page
   * that pulls in something outside that list shows without it, rather than being served
   * something it did not ask for.
   */
  import { RefreshCw, Maximize2, Minimize2, ShieldAlert, Code2 } from 'lucide-svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    /** Absolute path of the file — its directory becomes the page's `<base>`. */
    path,
    /** The buffer's text. The preview shows what you are looking at, not what is on disk. */
    html,
    /** Whether the page's own scripts may run. Owned by the host — it is the consent. */
    scripts = false,
    /** Turn scripts on (asks) or off (immediate). The off half is the one that has to exist:
     *  a permission you cannot take back is a one-way door. */
    onToggleScripts,
    /** Leave the preview and go back to the source. */
    onClose,
    /** Grow to a window nearly the size of Arbor, or come back. Absent = no such control. */
    onToggleFullscreen,
    fullscreen = false,
  }: {
    path: string;
    html: string;
    scripts?: boolean;
    onToggleScripts?: (next: boolean) => void;
    onClose?: () => void;
    onToggleFullscreen?: () => void;
    fullscreen?: boolean;
  } = $props();

  /** Bumped to re-mount the frame — a reload has to throw the old document away, and changing
   *  `srcdoc` in place leaves the previous one's timers and listeners running. */
  let generation = $state(0);

  const dir = $derived(path.replace(/[\\/][^\\/]*$/, ''));

  /**
   * The document handed to the frame: the buffer, with a `<base>` in front of it.
   *
   * Prepended rather than inserted into `<head>`: parsing the page here to find one would mean
   * having a second, worse HTML parser in the app, and a `<base>` before `<html>` is moved into
   * the head by the browser's own parser — which is the one that has to agree with it anyway.
   * A page that declares its own `<base>` wins, because the first one in the document does.
   */
  const doc = $derived(`<base href="${convertFileSrc(dir)}/">\n${html}`);

  /** `allow-scripts` is added only on the second consent. `allow-same-origin` is never added:
   *  with `allow-scripts` beside it the sandbox would be one line of script away from removing
   *  itself, which is the whole reason this frame is safe to show at all. */
  const sandbox = $derived(
    ['allow-forms', 'allow-popups-to-escape-sandbox', 'allow-modals']
      .concat(scripts ? ['allow-scripts'] : [])
      .join(' '),
  );
</script>

<div class="hp" class:fullscreen>
  <div class="hp-bar">
    <!-- No file name here: in the split view the editor's own toolbar is one row up and already
         says which file this is, and two rows naming the same thing is how a toolbar stops being
         read at all. This bar says what is true about the PREVIEW. -->
    <span class="hp-label">Preview</span>
    <!-- The state is the control. Saying "scripts blocked" beside a button that would change it
         is two things where one will do, and it is the only affordance that has to be obvious. -->
    <button
      type="button"
      class="hp-state"
      class:on={scripts}
      aria-pressed={scripts}
      onclick={() => onToggleScripts?.(!scripts)}
      use:tooltip={scripts ? 'Block this page\u2019s scripts again' : 'Let this page\u2019s scripts run'}
    >
      <ShieldAlert size={11} />
      {scripts ? 'scripts allowed' : 'scripts blocked'}
    </button>
    <div class="hp-actions">
      <IconButton tooltip="Reload the preview" size={24} onclick={() => (generation += 1)}>
        <RefreshCw size={13} />
      </IconButton>
      {#if onToggleFullscreen}
        <IconButton
          tooltip={fullscreen ? 'Back to the editor' : 'Fill the window'}
          size={24}
          onclick={onToggleFullscreen}
        >
          {#if fullscreen}<Minimize2 size={13} />{:else}<Maximize2 size={13} />{/if}
        </IconButton>
      {/if}
      {#if onClose}
        <IconButton tooltip="Show the source" size={24} onclick={onClose}>
          <Code2 size={13} />
        </IconButton>
      {/if}
    </div>
  </div>

  {#key `${generation}:${sandbox}`}
    <iframe class="hp-frame" title={`Preview of ${path}`} {sandbox} srcdoc={doc}></iframe>
  {/key}
</div>

<style>
  .hp { flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--bg-base); }
  .hp-bar {
    display: flex; align-items: center; gap: 8px; flex: none;
    height: 28px; padding: 0 6px 0 10px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .hp-label { text-transform: uppercase; letter-spacing: 0.05em; font-size: var(--font-size-2xs); color: var(--text-faint); }
  /* The sandbox state is the one thing on this bar that is about safety, so it is said in words
     rather than left to an icon — and it is green only when it is the quiet answer. A button,
     because the state and the switch are the same thing: press it to change your mind. */
  .hp-state {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 8px; border-radius: 999px; border: 1px solid transparent;
    font-size: var(--font-size-2xs); letter-spacing: 0.02em; cursor: pointer;
    font-family: inherit;
    color: var(--success); background: color-mix(in srgb, var(--success) 12%, transparent);
  }
  .hp-state:hover { border-color: color-mix(in srgb, var(--success) 45%, transparent); }
  .hp-state.on { color: var(--warning); background: color-mix(in srgb, var(--warning) 14%, transparent); }
  .hp-state.on:hover { border-color: color-mix(in srgb, var(--warning) 50%, transparent); }
  .hp-actions { margin-left: auto; display: flex; align-items: center; gap: 2px; }
  .hp-frame { flex: 1; min-height: 0; width: 100%; border: 0; background: #fff; }
</style>
