<script lang="ts">
  /**
   * The Payload tab: a JSON payload, what the project binds it to and writes back, and what its
   * validator says about the result.
   *
   * The middle column is the one that answers the question nobody can answer from the class: which
   * properties a payload **loses** on a round trip — sent, bound, and not written back.
   */
  import { Play } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { languageForPath } from '../languages';
  import { bennuDtoLabStore as lab } from '$lib/stores/bennu/dtolab.svelte';
  import DtoLabObjectTree from './DtoLabObjectTree.svelte';
  import DtoLabViolations from './DtoLabViolations.svelte';

  const JSON_LANGUAGE = languageForPath('payload.json');
  const RESULT_VIEWS = [
    { id: 'object', label: 'Bound object' },
    { id: 'json', label: 'Written back' },
  ];

  let shown = $state<'object' | 'json'>('object');
  let container = $state<HTMLElement>();

  const read = $derived(lab.readResult);
  const lost = $derived(lostProperties(lab.payload, read?.json ?? null));

  /** The top-level properties the payload sent that the written-back JSON does not have. */
  function lostProperties(sent: string, back: string | null): string[] {
    if (!back) return [];
    try {
      const before: unknown = JSON.parse(sent);
      const after: unknown = JSON.parse(back);
      if (!isObject(before) || !isObject(after)) return [];
      return Object.keys(before).filter((key) => !(key in after));
    } catch {
      return [];
    }
  }

  function isObject(value: unknown): value is Record<string, unknown> {
    return typeof value === 'object' && value !== null && !Array.isArray(value);
  }

  // Ctrl+Enter runs the payload from anywhere inside the tab — the editor included, which is where
  // the hands are when the payload is ready.
  function onWindowKeydown(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.key !== 'Enter') return;
    if (!container?.contains(document.activeElement)) return;
    e.preventDefault();
    void lab.run();
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<div class="payload" bind:this={container}>
  <section class="col">
    <div class="col-head">
      <span class="col-title">Payload</span>
      <span class="spacer"></span>
      <Button
        variant="ghost"
        size="xs"
        loading={lab.skeletonLoading}
        disabled={lab.running}
        tooltip="Replace it with what the project's own ObjectMapper writes for a new instance"
        onclick={() => void lab.useProjectJson()}
      >
        Use the project's JSON
      </Button>
      <Button variant="primary" size="xs" loading={lab.running} tooltip="Bind and validate it (Ctrl+Enter)" onclick={() => void lab.run()}>
        {#snippet iconStart()}<Play size={12} />{/snippet}
        Run
      </Button>
    </div>
    <div class="editor">
      <CodeEditor
        value={lab.payload}
        language={JSON_LANGUAGE}
        lineNumbers={false}
        placeholder="&#123; &#125;"
        oninput={(text) => lab.setPayload(text)}
      />
    </div>
  </section>

  <section class="col">
    <div class="col-head">
      <Tabs
        items={RESULT_VIEWS}
        value={shown}
        variant="pill"
        size="sm"
        ariaLabel="What the payload became"
        onSelect={(id) => (shown = id === 'json' ? 'json' : 'object')}
      />
      <span class="spacer"></span>
      {#if read}
        <span
          class="binder"
          use:tooltip={read.binder === 'jackson' ? "Bound with the project's own Jackson" : 'The project has no Jackson, so the payload was bound by field name'}
        >{read.binder === 'jackson' ? 'Jackson' : 'by field name'}</span>
      {/if}
    </div>
    <div class="col-body">
      {#if lab.readError}
        <Alert variant="error" compact text={lab.readError} />
      {:else if !read}
        <EmptyState compact message="Not run yet" description="Ctrl+Enter sends the payload." />
      {:else if read.binding_error}
        <Alert variant="warning" compact title="The payload does not bind" text={read.binding_error} />
      {:else}
        {#if lost.length > 0}
          <Alert variant="warning" compact title="Lost on the way back" text={`Sent, but not written back: ${lost.join(', ')}`} />
        {/if}
        {#if shown === 'object'}
          {#if read.object}
            <div class="tree"><DtoLabObjectTree root={read.object} /></div>
          {/if}
        {:else if read.json !== null}
          <div class="editor">
            {#key read.json}<CodeEditor value={read.json} language={JSON_LANGUAGE} readOnly lineNumbers={false} />{/key}
          </div>
        {:else}
          <EmptyState compact message={read.writing_error ?? 'Nothing is written back without Jackson.'} />
        {/if}
      {/if}
    </div>
  </section>

  <DtoLabViolations />
</div>

<style>
  .payload {
    flex: 1; min-height: 0;
    display: grid; grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr) minmax(0, 1fr);
  }
  .col { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .col + .col { border-left: 1px solid var(--border-subtle); }
  .col-head {
    display: flex; align-items: center; gap: 6px; flex-shrink: 0;
    min-height: 30px; padding: 3px 8px 3px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .col-title {
    font-size: var(--font-size-2xs); font-weight: 700; letter-spacing: 0.4px; text-transform: uppercase;
    color: var(--text-muted);
  }
  .spacer { flex: 1; }
  .binder {
    font-size: var(--font-size-2xs); color: var(--text-muted);
    padding: 1px 6px; border-radius: var(--radius-sm); background: var(--bg-overlay);
  }
  .col-body { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 6px; padding: 6px 8px; overflow: auto; }
  .editor { flex: 1; min-height: 0; overflow: hidden; display: flex; flex-direction: column; }
  .col-body .editor { margin: -6px -8px; }
  .tree { flex: 1; min-height: 0; overflow: auto; }
</style>
