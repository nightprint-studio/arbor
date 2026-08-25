<!--
  The extensions a package provides, in the Plugin Manager.

  ## Why they are not rows in the plugin list

  A plugin *calls* Arbor's API — it reacts to a hook, opens a form, has settings and an on/off
  switch. An extension is the other direction: it **implements an interface Arbor calls into**,
  and it has none of those. Giving it a plugin row would mean a row where four of the five
  controls are disabled and the fifth means something different: switching off a format backend
  does not stop a feature, it makes those files unopenable.

  So: its own section, and it is empty on almost every install. That is the correct default —
  extensions are how Arbor's own subsystems get shipped separately, not something most users
  add.

  ## Problems are the reason this exists

  A missing module or an id claimed twice makes a file type simply not open, with nothing
  anywhere saying why. Those are shown first and in full: the message arrives already written
  from the backend, beside the code that decided something was broken, so this component never
  reassembles an explanation out of fields.
-->
<script lang="ts">
  import { Binary, AlertTriangle, Info, CircleCheck, CircleAlert } from 'lucide-svelte';
  import { untrack } from 'svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { listExtensions, probeExtension } from '$lib/ipc/plugin';
  import type { ExtensionsReport } from '$lib/types/plugin';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';

  interface Props {
    /** Bumped by the parent after a reload, so the section refetches with everything else. */
    refreshKey?: number;
  }
  let { refreshKey = 0 }: Props = $props();

  let report = $state<ExtensionsReport | null>(null);

  /** Per-entry probe outcome, keyed by `interface@version/id`.
   *
   *  Probed rather than assumed: an entry appearing in the index means it is correctly
   *  DECLARED, which is a different fact from a module that runs. Showing only the first
   *  would be wrong in whichever direction the truth happened to be. */
  let probes = $state<Record<string, 'checking' | 'ok' | string>>({});

  const keyOf = (e: { interface: string; version: number; id: string }) =>
    `${e.interface}@${e.version}/${e.id}`;

  /** Record one probe outcome.
   *
   *  `untrack` around the read is load-bearing. `probes = { ...probes, … }` READS `probes` to
   *  copy it and WRITES it back, and the first iteration of the probe loop runs synchronously
   *  inside the effect — so the read registers as a dependency, the write re-runs the effect,
   *  and Svelte stops it with `effect_update_depth_exceeded`. Which is invisible until an
   *  install actually has an extension to probe. */
  function mark(key: string, outcome: 'checking' | 'ok' | string) {
    probes = { ...untrack(() => probes), [key]: outcome };
  }

  $effect(() => {
    // Read so the effect re-runs when the parent reloads plugins.
    void refreshKey;
    let cancelled = false;
    void listExtensions()
      .then((r) => { if (!cancelled) report = r; })
      // A backend that does not know the command yet is not an error worth a banner: the
      // section simply stays hidden, which is also what an install with no extensions looks
      // like.
      .catch(() => { if (!cancelled) report = null; });
    return () => { cancelled = true; };
  });

  // Probe each entry once the list is known. Sequential rather than parallel: each probe
  // instantiates a component, and a dozen at once would compile a dozen modules at once for a
  // panel nobody is waiting on.
  $effect(() => {
    const entries = report?.entries ?? [];
    if (entries.length === 0) return;
    let cancelled = false;
    void (async () => {
      for (const e of entries) {
        const k = keyOf(e);
        if (cancelled) return;
        mark(k, 'checking');
        try {
          await probeExtension(e.interface, e.version, e.id);
          if (!cancelled) mark(k, 'ok');
        } catch (err) {
          if (!cancelled) mark(k, String(err));
        }
      }
    })();
    return () => { cancelled = true; };
  });

  const hasAnything = $derived(
    !!report && (report.entries.length > 0 || report.problems.length > 0),
  );
  /** Read through a `$derived` rather than off `report` inside the markup: a `{#snippet}` is
   *  its own closure, so the enclosing `{#if report}` does not narrow inside it. */
  const runtimeAvailable = $derived(report?.runtime_available ?? false);
</script>

{#if hasAnything && report}
  <div class="pxs">
    <SectionHeader title={report.entries.length > 0 ? `Extensions (${report.entries.length})` : 'Extensions'}>
      {#snippet actions()}
        {#if !runtimeAvailable}
          <span
            class="pxs-note"
            use:tooltip={'This build found and validated these declarations but cannot instantiate them.'}
          >
            <Info size={11} /> declared only
          </span>
        {/if}
      {/snippet}
    </SectionHeader>

    {#each report.problems as p (p.key + p.kind)}
      <Alert
        variant={p.kind === 'conflict' ? 'warning' : 'error'}
        title={p.kind === 'conflict' ? `${p.key} is claimed twice` : `${p.key} is unavailable`}
        text={p.message}
      />
    {/each}

    {#each report.entries as e (e.interface + e.version + e.id)}
      {@const state = probes[keyOf(e)]}
      <div class="pxs-row">
        <Binary size={13} />
        <span class="pxs-id">{e.id}</span>
        <Badge variant="tone" tone="neutral">{e.interface}@{e.version}</Badge>
        {#if state === 'ok'}
          <span class="pxs-ok" use:tooltip={'Instantiated: its imports resolve and its exports match.'}>
            <CircleCheck size={11} /> runs
          </span>
        {:else if state && state !== 'checking'}
          <span class="pxs-bad" use:tooltip={state}>
            <CircleAlert size={11} /> will not load
          </span>
        {/if}
        <span class="pxs-owner">{e.plugin}</span>
      </div>
    {/each}

    {#if report.entries.length === 0 && report.problems.length > 0}
      <p class="pxs-none">
        <AlertTriangle size={11} /> No extension is available — every declaration above is broken.
      </p>
    {/if}
  </div>
{/if}

<style>
  .pxs { display: flex; flex-direction: column; gap: 6px; padding: 10px 12px 4px; }

  .pxs-row {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 8px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .pxs-id { color: var(--text-primary); font-weight: 600; }
  /* Pushed right: which package provides it matters less than what it provides, and the
     eye should land on the id. */
  .pxs-owner { margin-left: auto; color: var(--text-faint); }

  .pxs-ok, .pxs-bad {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: var(--font-size-2xs); font-weight: 600;
  }
  .pxs-ok  { color: var(--success); }
  .pxs-bad { color: var(--error); cursor: help; }

  .pxs-note {
    display: inline-flex; align-items: center; gap: 4px;
    color: var(--text-faint); font-size: var(--font-size-2xs);
  }

  .pxs-none {
    display: flex; align-items: center; gap: 6px;
    margin: 0; color: var(--warning); font-size: var(--font-size-xs);
  }
</style>
