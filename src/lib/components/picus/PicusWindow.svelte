<script lang="ts">
  /**
   * PicusWindow — standalone boot shell for the dedicated Picus window (SQL
   * studio). Mirrors TytoWindow / MerulaWindow: it is NOT the full Arbor app, it
   * only boots the theme / appearance / animation config and mounts `PicusShell`.
   *
   * `picus-be` serves both halves: the studio's settings and connections, and the
   * script repository — its tree, its inventory and its consistency findings. Each
   * window is its own JS context, so these stores are independent of the main
   * window's.
   */
  import { onMount, untrack } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { picusSettingsStore } from '$lib/stores/picus/settings.svelte';
  import { connectionsStore, isSessionOpen } from '$lib/stores/picus/connections.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { picusResultsStore } from '$lib/stores/picus/result.svelte';
  import { picusScratchStore } from '$lib/stores/picus/scratch.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';
  import PicusShell from './PicusShell.svelte';

  onMount(() => {
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    void picusSettingsStore.loadConfig();
    void connectionsStore.load();

    // `picus-be` spawns off-thread, racing this window's first reads: if it
    // attaches after we already asked, the persisted settings would silently stay
    // on defaults — and the next toggle would write those defaults back over the
    // user's file — while the connections panel would sit empty. Re-read both once
    // the backend signals it's routable.
    const unlisten = listen('arbor://picus-be-up', () => {
      void picusSettingsStore.loadConfig();
      void connectionsStore.load();
    });

    // The unsaved query tabs, re-opened with their text. Before the `picus-be-up`
    // listener above rather than inside it: the backend is usually already there
    // (the window is opened after it is spawned), and a scratchpad that only
    // appeared on a re-attach would look like it had been lost.
    void picusScratchStore.restore();

    // The last two paths out. Every tab open at this moment holds a cursor, and
    // closing the window closes none of them by itself — the sessions outlive the
    // webview. The scratchpad is flushed in the same breath, so the sentence typed
    // immediately before Alt+F4 is the one that is saved.
    const release = () => {
      picusScratchStore.flush();
      picusResultsStore.releaseAll();
    };
    globalThis.addEventListener('beforeunload', release);

    return () => {
      globalThis.removeEventListener('beforeunload', release);
      release();
      void unlisten.then((off) => off());
    };
  });

  /**
   * Save the scratchpad when a buffer or the tab list changes.
   *
   * Reads the text of every query tab so the effect depends on all of them; the
   * store debounces, so a keystroke costs a timer rather than a file write.
   */
  $effect(() => {
    for (const tab of picusTabsStore.tabs) {
      if (tab.kind === 'query') void queryStore.read(tab.id).sql;
    }
    void picusTabsStore.activeId;
    picusScratchStore.touch();
  });

  /**
   * Keep the schema tree pointed at the active connection.
   *
   * Only for a connection that is actually **open** — `connecting` is not open.
   * The button sets that state the instant it is pressed and only clears it when
   * the session has been established, so a condition of "not disconnected" fires
   * this while the connect is still in flight: the read then goes out against a
   * session that does not exist yet, or against the *previous* one that is about to
   * be replaced and closed underneath it. Either way the answer describes something
   * that is no longer true, and the second read — the one issued once the connection
   * really is up — queues behind it on the same connection.
   *
   * A disconnected connection has no catalogue at all, and showing the previous
   * connection's tables under a new connection's name is the kind of quiet
   * wrongness that gets a DELETE written against the wrong database.
   */
  $effect(() => {
    const active = connectionsStore.active;
    const open = isSessionOpen(active);
    if (active && open) {
      schemaStore.select(active.id);
      void schemaStore.ensure(active.id);
    } else if (!active || active.state === 'disconnected') {
      // Only this connection's catalogue. The others belong to sessions that are
      // still up, and dropping them would blank the tabs bound to them.
      if (active) schemaStore.forget(active.id);
      else schemaStore.select('');
    }
  });

  /**
   * …and give the tab you are typing in its own connection's catalogue.
   *
   * A tab carries its own binding, which need not be the selected connection —
   * that is the whole point of binding a tab. Without this the editor's
   * intelligence followed the **sidebar** rather than the buffer: completion,
   * abbreviation expansion and live validation all went quiet on a tab bound
   * anywhere else, with nothing on screen to say why.
   *
   * `ensure` rather than `load`: this fires on every tab switch, and a catalogue
   * already held is the common case. The store keeps a few, so moving between two
   * tabs on two databases costs one read each and nothing after that.
   */
  $effect(() => {
    const bound = picusTabsStore.connectionOf(picusTabsStore.active);
    if (bound && isSessionOpen(bound)) void schemaStore.ensure(bound.id);
  });

  /**
   * Keep the script repository pointed at the active connection's.
   *
   * Picus is database-oriented: you open a database and *its* scripts are what you
   * see. Unlike the schema, this does NOT require an open session — a repository
   * with an Oracle branch is maintained, checked and generated into with no Oracle
   * driver in existence, which is exactly the case the product was built for.
   *
   * `open()` is a no-op when the root is already the one loaded, so this settles
   * after one pass; the read is deliberately not awaited, so nothing here can
   * block a paint.
   */
  $effect(() => {
    const root = connectionsStore.activeScriptRoot;
    untrack(() => {
      if (root === picusProjectStore.root) return;
      // The destinations name files inside the repository being left behind.
      dmlStore.resetTargets();
      if (root) void picusProjectStore.open(root);
      else picusProjectStore.close();
    });
  });
</script>

<div class="picus-window">
  <PicusShell />
</div>

<style>
  .picus-window {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-elevated);
    overflow: hidden;
  }
</style>
