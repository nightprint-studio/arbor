<script lang="ts">
  /**
   * The pending write set: which files a generation would touch, before it does.
   *
   * Its own panel with its own header, like the rest of the dock. The preview is
   * built here rather than by whatever hosts it — it reads disk, so it is paid for
   * when somebody is actually looking at what would be written.
   */
  import { untrack } from 'svelte';
  import { RefreshCw } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import PatchDiffCard from '../generate/PatchDiffCard.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';

  /**
   * `ensurePreview` is self-guarding, so this effect can simply watch the payload
   * key and fire: a landed preview does not re-trigger it, and neither does a
   * failed one.
   */
  $effect(() => {
    if (!dmlStore.generated) return;
    void dmlStore.previewKey;
    untrack(() => void dmlStore.ensurePreview());
  });

  /** The destination a previewed file belongs to — for its dialect and role chips. */
  function targetFor(path: string) {
    return dmlStore.targets.find((t) => t.file === path) ?? null;
  }
</script>

<div class="ch">
  <BottomPanelHeader
    title="Changes"
    count={dmlStore.previewFiles.length}
    onClose={() => picusUiStore.closeBottom()}
  >
    {#snippet actions()}
      <Button
        variant="icon"
        size="xs"
        tooltip={'Re-read the destinations from disk and recompute the patch'}
        ariaLabel="Rebuild the preview"
        disabled={dmlStore.previewing || !dmlStore.generated}
        onclick={() => void dmlStore.rebuildPreview()}
      >
        {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
      </Button>
    {/snippet}
  </BottomPanelHeader>

  <div class="ch-body">
    {#if dmlStore.applyError}
      <!-- The backend's refusal, word for word: it names the file that moved, which
           is the only part that tells the user what to do next. -->
      <Alert variant="error" title="Nothing was written" text={dmlStore.applyError}>
        {#snippet actions()}
          <Button variant="secondary" size="xs" onclick={() => void dmlStore.rebuildPreview()}>
            Read the files again
          </Button>
        {/snippet}
      </Alert>
    {/if}

    {#if !dmlStore.generated}
      <StateBlock
        tone="info"
        fill={false}
        label="No pending change. Generate from the DML tab to see which files would be touched."
      />
    {:else if dmlStore.previewError}
      <Alert variant="error" title="The patch could not be computed" text={dmlStore.previewError} />
    {:else if dmlStore.previewing && !dmlStore.previewFiles.length}
      <StateBlock tone="loading">
        {#snippet spinner()}<Spinner size={14} />{/snippet}
        <span>Reading the destinations…</span>
      </StateBlock>
    {:else if !dmlStore.previewFiles.length}
      <StateBlock
        tone="info"
        fill={false}
        label="Nothing to write — no destination is enabled, or none of them would change."
      />
    {:else}
      {#if !dmlStore.previewFresh}
        <!-- The generation moved after this patch was computed. Writing it is
             refused rather than silently re-planned, so say so here. -->
        <Alert
          variant="warning"
          compact
          text="The generation changed after this patch was computed — it is out of date and will not be written as it stands. Rebuild it to see what would land now."
        >
          {#snippet actions()}
            <Button variant="secondary" size="xs" onclick={() => void dmlStore.rebuildPreview()}>
              Rebuild
            </Button>
          {/snippet}
        </Alert>
      {/if}
      <p class="ch-note">
        {dmlStore.changedFiles.length} file{dmlStore.changedFiles.length === 1 ? '' : 's'} would be
        written, exactly as shown below — this is the backend's own output, not a rendering of it.
        Encoding and line endings stay as they are, and every original is copied to
        <code>.arbor/backup</code> first. Nothing is written until you confirm.
      </p>
      {#each dmlStore.previewFiles as file (file.path)}
        <PatchDiffCard {file} target={targetFor(file.path)} />
      {/each}
    {/if}
  </div>
</div>

<style>
  .ch { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }
  .ch-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
  }
  .ch-note {
    font-size: var(--font-size-xs);
    line-height: 1.55;
    color: var(--text-muted);
    max-width: 90ch;
  }
  .ch-note code { font-family: var(--font-code); font-size: var(--font-size-xs); }
</style>
