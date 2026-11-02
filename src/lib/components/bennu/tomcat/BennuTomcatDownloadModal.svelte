<script lang="ts">
  /**
   * Download a Tomcat: pick the release line, pick the version, watch it arrive.
   *
   * The lines say what matters about each — the servlet package it speaks and the Java it needs —
   * because that is what decides whether a given project can run on it, and the project's own
   * namespace is shown beside them so the right line is the one already marked. The versions are
   * read from Apache's own directory listing, so "latest" is whatever Apache says it is, not what
   * this build knew when it shipped.
   *
   * Nothing is chosen for the person beyond the line that matches the project: the newest stable
   * release of it, which they can change.
   */
  import { onMount } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { Download } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import DownloadProgress from '$lib/components/shared/ui/DownloadProgress.svelte';
  import { createDownloadMeter, type DownloadStats } from '$lib/utils/download-progress';
  import {
    EVT_TOMCAT_INSTALL,
    tomcatDownload,
    tomcatVersions,
    type TomcatInstallEvent,
    type TomcatLine,
  } from '$lib/ipc/bennu/tomcat';

  let {
    root,
    installDir,
    lines,
    projectNamespace,
    onClose,
    onInstalled,
  }: {
    root: string;
    /** The configured installs folder; empty = the default. */
    installDir: string;
    lines: TomcatLine[];
    /** `javax` / `jakarta` when the project's sources say, else `null`. */
    projectNamespace: string | null;
    onClose: () => void;
    /** Called with the folder of the Tomcat that was just unpacked. */
    onInstalled: (home: string) => void;
  } = $props();

  /** The line that matches the project, else the newest one that does not need a new Java. */
  const initialLine = (): string =>
    lines.find((l) => l.namespace === projectNamespace)?.line ?? lines[Math.min(2, lines.length - 1)]?.line ?? '9.0';

  let line = $state(initialLine());
  let versions = $state<string[]>([]);
  let version = $state('');
  let listing = $state(false);
  let failure = $state('');

  type Phase = 'idle' | 'lookup' | 'download' | 'verify' | 'extract';
  let phase = $state<Phase>('idle');
  let stats = $state<DownloadStats | null>(null);
  const meter = createDownloadMeter();
  const busy = $derived(phase !== 'idle');
  /** Closing mid-download would leave the person with no way to see how it ended. */
  const close = () => { if (!busy) onClose(); };

  const PHASES: Record<Exclude<Phase, 'idle'>, string> = {
    lookup: 'Reading the checksum…',
    download: 'Downloading…',
    verify: 'Verifying the checksum…',
    extract: 'Unpacking…',
  };

  // The newest releases of the chosen line. A stale reply (the person changed line meanwhile) is dropped.
  let asked = 0;
  $effect(() => {
    const wanted = line;
    const mine = ++asked;
    listing = true;
    failure = '';
    versions = [];
    tomcatVersions(wanted)
      .then((v) => {
        if (mine !== asked) return;
        versions = v;
        version = v[0] ?? '';
      })
      .catch((e) => {
        if (mine === asked) failure = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        if (mine === asked) listing = false;
      });
  });

  let unlisten: UnlistenFn | null = null;
  onMount(() => {
    void listen<TomcatInstallEvent>(EVT_TOMCAT_INSTALL, (e) => {
      const p = e.payload;
      if (p.kind === 'phase') phase = p.phase;
      else if (p.kind === 'progress') stats = meter.push(p.done, p.total);
      else if (p.kind === 'done') {
        phase = 'idle';
        onInstalled(p.home);
      } else {
        phase = 'idle';
        failure = p.error;
      }
    }).then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  async function start() {
    if (!version || busy) return;
    failure = '';
    stats = null;
    phase = 'lookup';
    try {
      await tomcatDownload(root, version, installDir);
    } catch (e) {
      phase = 'idle';
      failure = e instanceof Error ? e.message : String(e);
    }
  }

  const selected = $derived(lines.find((l) => l.line === line) ?? null);
</script>

<Modal onClose={close} width="560px" padBody={false} ariaLabel="Download Tomcat">
  {#snippet header()}
    <ModalHeader onClose={close}>
      <Download size={14} />
      <span class="title">Download Apache Tomcat</span>
    </ModalHeader>
  {/snippet}

  <div class="body">
    <FormField label="Release line" hint="Servlet 3.1 to 6.1. Choose the one your project's sources import.">
      <div class="lines" role="radiogroup" aria-label="Release line">
        {#each lines as l (l.line)}
          <button
            type="button"
            class="line"
            class:on={l.line === line}
            role="radio"
            aria-checked={l.line === line}
            disabled={busy}
            onclick={() => (line = l.line)}
          >
            <span class="name">Tomcat {l.line}</span>
            <span class="chips">
              <Badge variant="tone" tone={l.namespace === 'jakarta' ? 'accent' : 'warning'} size="sm" label={`${l.namespace}.servlet`} />
              <Badge variant="tone" tone="neutral" size="sm" label={`Java ${l.min_java}+`} />
              {#if projectNamespace === l.namespace}<Badge variant="tone" tone="success" size="sm" label="matches your project" />{/if}
            </span>
          </button>
        {/each}
      </div>
    </FormField>

    <FormField label="Version" hint="Stable releases only, newest first — read from Apache.">
      <div class="ver">
        <Select
          value={version}
          options={versions.map((v) => ({ value: v, label: v }))}
          fill
          disabled={busy || listing || versions.length === 0}
          searchable={versions.length > 12}
          onchange={(v) => (version = v)}
        />
        {#if listing}<Spinner size={14} />{/if}
      </div>
    </FormField>

    {#if busy}
      <div class="progress" aria-live="polite">
        <span class="phase">{PHASES[phase as Exclude<Phase, 'idle'>]}</span>
        <DownloadProgress stats={phase === 'download' ? stats : null} ariaLabel="Tomcat download" />
      </div>
    {/if}

    {#if failure}
      <Alert variant="error" compact text={failure} />
    {/if}

    {#if selected && !busy}
      <p class="note">
        Unpacked into {#if installDir}<code>{installDir}/{version || selected.line}</code>{:else}the Tomcat folder of your Bennu profile, shared by every project{/if},
        after the download matches the SHA-512 Apache publishes.
      </p>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="secondary" size="sm" onclick={close} disabled={busy}>Cancel</Button>
      <Button variant="primary" size="sm" onclick={start} disabled={!version || busy || listing} loading={busy}>
        {#snippet iconStart()}<Download size={13} />{/snippet}
        Download {version}
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .body { display: flex; flex-direction: column; gap: 14px; padding: 16px 18px; }
  .lines { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .line {
    display: flex; flex-direction: column; gap: 6px; align-items: flex-start; text-align: left;
    padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--radius-md);
    background: var(--bg-base); color: var(--text-primary); cursor: pointer;
  }
  .line:hover:not(:disabled) { border-color: var(--text-muted); }
  .line.on { border-color: var(--accent); background: var(--accent-subtle); }
  .line:disabled { cursor: default; opacity: 0.7; }
  .line .name { font-weight: 600; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .ver { display: flex; align-items: center; gap: 8px; }
  .progress { display: flex; flex-direction: column; gap: 6px; }
  .phase { font-size: var(--font-size-xs); color: var(--text-muted); }
  .note { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  code { font-family: var(--font-code); color: var(--text-secondary); }
</style>
