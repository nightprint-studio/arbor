<script lang="ts">
  /**
   * Mounts one scene's component and reports whether it survived.
   *
   * The boundary is the point: a modal that throws on a missing prop or an unmocked command
   * must show up as "error" in the gallery's report, not blank the whole page.
   */
  import type { Component } from 'svelte';

  let {
    Comp,
    props,
    onstatus,
  }: {
    Comp: Component<any>;
    props: Record<string, unknown>;
    onstatus: (s: { state: 'ok' | 'error'; error?: string }) => void;
  } = $props();

  const fail = (e: unknown) =>
    onstatus({ state: 'error', error: e instanceof Error ? `${e.message}\n${e.stack ?? ''}`.slice(0, 600) : String(e) });
</script>

<svelte:boundary onerror={fail}>
  <Comp {...props} />
</svelte:boundary>
