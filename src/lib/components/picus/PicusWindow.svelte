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
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
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
    return () => { void unlisten.then((off) => off()); };
  });

  /**
   * Keep the schema tree pointed at the active connection.
   *
   * Only for a connection that is actually open: a disconnected one has no
   * catalogue to read, and showing the previous connection's tables under a new
   * connection's name is the kind of quiet wrongness that gets a DELETE written
   * against the wrong database.
   */
  $effect(() => {
    const active = connectionsStore.active;
    if (active && active.state !== 'disconnected' && schemaStore.connectionId !== active.id) {
      void schemaStore.load(active.id);
    } else if (!active || active.state === 'disconnected') {
      if (schemaStore.connectionId) schemaStore.clear();
    }
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
