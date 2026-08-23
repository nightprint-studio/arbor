<!--
  FormNodeEmbed — a page a plugin ships, mounted in a sandboxed frame.

  ## What the host knows

  How to give a folder a URL, how to isolate a frame, and how to pass messages. Nothing about
  what runs inside: it could be a Bevy viewport, a graph, a map, a document viewer. That is the
  point — if this node knew, adding a second kind of embedded thing would be a change to Arbor.

  ## Isolation

  `sandbox="allow-scripts"` **without** `allow-same-origin` by default. The two together are
  famously not a sandbox at all — a frame with both can reach out and rewrite its own sandbox
  attribute — so the page runs opaque-origin: no cookies, no storage, no DOM access to the app
  around it. The only way in or out is `postMessage`, which is the seam this node exists to
  provide.

  A page that has to **fetch its own files** — a wasm module, a texture, a data file — cannot
  live with that: WebKit refuses custom-scheme sub-resource loads from an opaque origin, and
  the symptom is a 403 plus "Not allowed to download due to sandboxing", which reads like a
  permissions bug rather than the fetch failure it is. Those pages set `same_origin = true`
  and get the `asset:` origin — shared with other plugin files, never with the app, which
  lives on a different scheme. Opt-in, so the quiet case stays the default.

  Messages are filtered by `event.source` rather than by origin, because an opaque origin
  reports itself as `"null"` and every sandboxed frame in the window would match that — and
  because with `same_origin` the check must not become "trust anything on asset:".

  ## The contract

  Outbound, the plugin's `send` payloads go in as JSON text — see `encode` for why a string
  and not the object. Inbound, whatever the page posts is handed to the plugin's `on_message`
  slot verbatim. This node does not read either — a schema here would be this file learning
  what the page is for.
-->
<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core';

  /** Matches `plugin_assets::SCHEME` in the shell. */
  const PLUGIN_SCHEME = 'plugin';
  import { onDestroy } from 'svelte';
  import type { FormNode } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';
  import { reportPluginError } from '$lib/utils/plugin-report';

  interface Props {
    node: FormNode;
    ctx:  FormNodeCtx;
  }
  let { node, ctx }: Props = $props();

  const n = $derived(node as any);

  /** Absolute path of the page to load, resolved by the plugin (it knows its own folder). */
  const src    = $derived(String(n.src ?? ''));
  /** `height = "fill"` takes whatever vertical space the surface has left; a number is that
   *  many pixels. Fill is what a viewport in a panel wants — a fixed 320px in a 900px-tall
   *  split leaves two thirds of it empty and the picture too small to judge. A modal still
   *  wants a number, because a modal is sized by its content rather than the other way round. */
  const fill   = $derived(n.height === 'fill');
  const height = $derived(typeof n.height === 'number' ? n.height : 360);
  /** A floor for fill mode: a flex child with no basis collapses to nothing when the content
   *  above it is tall, and an invisible viewport reads as a broken one. */
  const minH   = $derived(typeof n.min_height === 'number' ? n.min_height : 320);
  /** Messages to post into the frame. Replacing the array is what sends. */
  const send   = $derived(Array.isArray(n.send) ? n.send : []);
  /** Scoped slot fired with whatever the page posts back. */
  const onMessage = $derived(n.on_message);

  /** Opt-in `allow-same-origin`.
   *
   *  The default is a frame with an OPAQUE origin — no storage, no cookies, no reach into the
   *  app around it — and for a page that only draws and posts messages that is right.
   *
   *  It is not enough for a page that has to FETCH: a wasm module, a texture, its own data
   *  file. WebKit refuses custom-scheme sub-resource loads from an opaque origin, so the page
   *  comes back 403 and "Not allowed to download due to sandboxing", which is a fetch failure
   *  wearing a confusing hat. Such a page opts in, and gets the `asset:` origin — shared with
   *  other plugin files, never with the app, which lives on a different scheme entirely.
   *
   *  Opt-in rather than always-on because the two cases are genuinely different, and the
   *  quiet one should stay the default. */
  const sameOrigin = $derived(!!n.same_origin);
  const sandbox    = $derived(
    sameOrigin ? 'allow-scripts allow-same-origin' : 'allow-scripts',
  );

  let frame = $state<HTMLIFrameElement | null>(null);
  let ready = $state(false);
  let failed = $state<string | null>(null);

  // A local path becomes a `plugin:` URL; anything already a URL is left alone, so a plugin
  // can point at a data: page for something tiny without a file on disk.
  //
  // `plugin:`, not `asset:`. The asset protocol is for the user's media and does not know
  // `wasm` — a module served as `application/octet-stream` makes `instantiateStreaming`
  // reject, and every wasm-bindgen loader then buffers the whole thing and compiles it in one
  // go. The shell's own scheme (`src-tauri/src/plugin_assets.rs`) types it correctly and
  // checks the path against the plugin roots instead of a media glob.
  //
  // `convertFileSrc` alone is not enough, and the way it fails is nasty. It percent-encodes
  // the whole filesystem path into ONE url segment — slashes included — so
  // `/plugins/pkg/web/index.html` becomes `asset://localhost/%2Fplugins%2Fpkg%2Fweb%2F…`.
  // The document loads fine. Then the page's own `import './runtime.js'` resolves relative to
  // that URL, and since there is only one segment to replace, it resolves to
  // `asset://localhost/runtime.js` — the directory is simply gone. What you see is a 403
  // naming a bare filename, and a page that got as far as its first import and stopped.
  //
  // So the path is rebuilt with its structure intact: each segment encoded on its own,
  // slashes left as slashes. The host and scheme come from `convertFileSrc` because those
  // differ per platform (`asset:` vs `http://asset.localhost`) and that part it gets right.
  //
  // The leading `/` is the subtle half. Tauri's asset handler drops exactly one byte off the
  // URL path (`request.uri().path().as_bytes()[1..]`) and treats what is left as the
  // filesystem path — so what the frame asks for must carry ONE slash more than the real
  // path. Get it wrong and the request arrives as `Users/christian/…`, relative, matching no
  // allowed directory: another 403, this time naming a path that looks right. Splitting
  // without discarding the empty first segment is what preserves it on Unix, and leaves a
  // Windows drive letter first where it belongs.
  const url = $derived.by(() => {
    if (!src) return '';
    if (/^(https?|data|blob):/.test(src)) return src;
    try {
      const u = new URL(convertFileSrc(src, PLUGIN_SCHEME));
      const path = decodeURIComponent(u.pathname.slice(1));
      u.pathname = '/' + path.split(/[/\\]/).map(encodeURIComponent).join('/');
      return u.toString();
    } catch {
      return '';
    }
  });

  // ── Inbound ──────────────────────────────────────────────────────────────
  $effect(() => {
    const el = frame;
    if (!el) return;
    const onMsg = (e: MessageEvent) => {
      // By source, not by origin: a sandboxed frame's origin is the string "null", which
      // every other sandboxed frame in this window also reports.
      if (e.source !== el.contentWindow) return;
      ready = true;
      if (onMessage) {
        ctx.handleScopedDispatch(n.id, 'message', onMessage, e.data, { stateKeys: n.scope_state });
      }
    };
    window.addEventListener('message', onMsg);
    return () => window.removeEventListener('message', onMsg);
  });

  // ── Outbound ─────────────────────────────────────────────────────────────
  //
  // Queued until the page says something, because a message posted before its listener is
  // attached is simply lost — and "the first open did nothing, the second worked" is the
  // hardest version of this bug to see.
  let pending: string[] = [];
  let sentCount = 0;
  // Highest `seq` delivered, for the stamped path below. `-Infinity` so a plugin numbering
  // from zero — or from anywhere — has its first message counted as new.
  let sentSeq = -Infinity;

  /** Serialise one outbound message.
   *
   *  A string, not the object. Two reasons, and either alone is enough:
   *
   *  · The payload arrives here as **reactive state** — a Svelte `$state` proxy — and
   *    `postMessage` structured-clones its argument, which throws `DataCloneError: The object
   *    can not be cloned` on a Proxy. The failure lands in an `$effect` with no hint that a
   *    plugin's message was what could not cross.
   *  · The page on the other side stringifies whatever it receives anyway, because its own
   *    relay has to hand the runtime a string. Posting the object made a copy that was
   *    immediately re-serialised.
   *
   *  A payload that is not JSON-able is refused loudly here rather than crossing as `"null"`
   *  and confusing the page: the contract for this seam is JSON, since it started life as a
   *  Lua table. */
  function encode(message: unknown): string | null {
    try {
      return JSON.stringify(message) ?? null;
    } catch (e) {
      reportPluginError(ctx.pluginName, `embed '${n.id}': message could not be serialised`, e);
      return null;
    }
  }

  function flush() {
    const w = frame?.contentWindow;
    if (!w || !ready) return;
    for (const m of pending) w.postMessage(m, '*');
    pending = [];
  }

  $effect(() => {
    // Two ways to say what is new, and a plugin picks by whether it stamps its messages.
    //
    // With `seq`: delivery is "every message whose seq is above the highest one delivered".
    // That lets the outbox be REWRITTEN rather than appended to — the natural shape for a
    // live surface, where the truth is one `open` plus the latest state and everything
    // between them is superseded. A panel driving a viewport at pointer rate would otherwise
    // append thousands of messages that only exist to be skipped, and re-serialise all of
    // them on every tick.
    //
    // Without it: the historical index count, so a plugin that just appends keeps working.
    const seqOf = (m: any): number =>
      (m && typeof m === 'object' && Number.isFinite(m.seq)) ? Number(m.seq) : NaN;
    const stamped = send.length > 0 && Number.isFinite(seqOf(send[send.length - 1]));

    let fresh: unknown[];
    if (stamped) {
      fresh = send.filter((m) => { const s = seqOf(m); return !Number.isFinite(s) || s > sentSeq; });
      for (const m of send) { const s = seqOf(m); if (s > sentSeq) sentSeq = s; }
    } else {
      // A list SHORTER than what has already been sent is a reset, not an empty delta: the
      // plugin has replaced the outbox with a new minimal replay set. Replaying from the
      // start is exactly right — the frame is being told the whole current truth.
      if (send.length < sentCount) sentCount = 0;
      fresh = send.slice(sentCount);
      sentCount = send.length;
    }
    if (fresh.length === 0) return;
    for (const m of fresh) {
      const encoded = encode(m);
      if (encoded !== null) pending.push(encoded);
    }
    flush();
  });

  $effect(() => {
    if (ready) flush();
  });

  onDestroy(() => { pending = []; });
</script>

<div
  class="embed"
  class:embed-fill={fill}
  style={fill ? `min-height:${minH}px` : `height:${height}px`}
>
  {#if !url}
    <p class="embed-empty">This node has no page to show.</p>
  {:else}
    <iframe
      bind:this={frame}
      class="embed-frame"
      title={String(n.label ?? 'Plugin view')}
      src={url}
      {sandbox}
      referrerpolicy="no-referrer"
      onload={() => { failed = null; }}
      onerror={() => { failed = 'The page could not be loaded.'; }}
    ></iframe>
  {/if}
  {#if failed}
    <p class="embed-error">{failed}</p>
  {/if}
</div>

<style>
  .embed-fill {
    flex: 1 1 auto;
    align-self: stretch;
  }

  .embed {
    position: relative;
    width: 100%;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--bg-base);
  }
  .embed-frame {
    display: block;
    width: 100%;
    height: 100%;
    border: 0;
    /* The page paints its own background; a transparent frame would show the panel through a
       viewport that has not drawn its first frame yet. */
    background: var(--bg-base);
  }
  .embed-empty, .embed-error {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    margin: 0;
    padding: 12px;
    text-align: center;
    font-size: var(--font-size-xs);
  }
  .embed-empty { color: var(--text-faint); }
  .embed-error { color: var(--error); background: var(--bg-base); }
</style>
