<script lang="ts">
  /**
   * The clean view's settings, beside the console: how each column is drawn, the rules, and
   * which frames of a stack trace are the user's.
   *
   * Everything here is about what is *drawn*; the buffer is never edited. The easiest way to make
   * a rule is not this page but the menu on a line ("Hide this logger") — this is where they are
   * reviewed, edited and removed.
   */
  import { Plus, Trash2 } from 'lucide-svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import TokenListInput from '$lib/components/shared/ui/TokenListInput.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { bennuLogViewStore as store } from '$lib/stores/bennu/log-view.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import type { LogViewRule, LogViewSettings } from './log-view';

  let tab = $state('columns');
  const s = $derived(store.settings);

  // Read again whenever the panel opens: the configuration may have been edited since the run began.
  $effect(() => { void store.refreshLayouts(projectStore.project?.root); });
  const layout = $derived(store.layout);
  /** Configurations that name a pattern the console cannot read, when none can be. */
  const unreadable = $derived((store.layouts ?? []).filter((l) => !l.usable));
  /** What Logger and Thread depend on: without a layout they are guesses, and the shape rules rarely make them. */
  const guessing = $derived(store.layouts !== null && !layout);

  const TABS = [
    { id: 'columns', label: 'Columns' },
    { id: 'rules', label: 'Rules' },
    { id: 'traces', label: 'Traces' },
  ];
  const FIELDS = [
    { value: 'logger', label: 'Logger' }, { value: 'message', label: 'Message' },
    { value: 'thread', label: 'Thread' }, { value: 'level', label: 'Level' },
  ];
  const OPS = [
    { value: 'starts', label: 'starts with' }, { value: 'contains', label: 'contains' },
    { value: 'equals', label: 'equals' }, { value: 'regex', label: 'matches' },
  ];
  const ACTIONS = [
    { value: 'hide', label: 'Hide' }, { value: 'dim', label: 'Dim' }, { value: 'highlight', label: 'Highlight' },
  ];
  /** A small palette rather than a free colour: a highlight has to read on both themes. */
  const COLORS = [
    { value: '#d29922', label: 'Amber' }, { value: '#a371f7', label: 'Purple' },
    { value: '#58a6ff', label: 'Blue' }, { value: '#3fb950', label: 'Green' }, { value: '#f85149', label: 'Red' },
  ];

  function addRule() {
    store.addRule({ on: true, field: 'logger', op: 'starts', value: '', action: 'hide', color: '#d29922' });
  }
  const patchRule = (i: number, p: Partial<LogViewRule>) => store.updateRule(i, p);

  /** Patch a column-visible setting and auto-enable clean view when it is off — same pattern as addRuleFrom. */
  function colPatch(p: Partial<LogViewSettings>) {
    store.patch(s.clean ? p : { clean: true, ...p });
  }
</script>

<aside class="lvs" aria-label="Log view settings">
  <Tabs items={TABS} value={tab} size="sm" fill onSelect={(id) => (tab = id)} />

  <div class="body">
    {#if tab === 'columns'}
      {#if layout}
        <Alert variant="success" compact title={`Layout read from ${layout.file}`}>
          <code class="pat">{layout.pattern}</code>
        </Alert>
      {:else if guessing}
        <Alert variant="warning" compact title="No log layout found"
          text={unreadable.length
            ? `${unreadable[0].file} names a pattern Bennu cannot read, so the columns are guessed from the shape of each line.`
            : 'No logback, log4j2, log4j or Spring configuration names a console pattern, so the columns are guessed from the shape of each line. Logger and Thread need a layout.'} />
      {/if}
      <FormField label="Time">
        <Select size="sm" fill value={s.time} onchange={(v) => colPatch({ time: v as typeof s.time })}
          options={[{ value: 'time', label: 'Time of day' }, { value: 'hide', label: 'Hidden' }]} />
      </FormField>
      <FormField label="Level">
        <Select size="sm" fill value={s.level} onchange={(v) => colPatch({ level: v as typeof s.level })}
          options={[{ value: 'text', label: 'Word' }, { value: 'letter', label: 'Letter only' }, { value: 'hide', label: 'Hidden (row colour only)' }]} />
      </FormField>
      <FormField label="Thread">
        <Select size="sm" fill disabled={guessing} value={s.thread} onchange={(v) => colPatch({ thread: v as typeof s.thread })}
          options={[{ value: 'hide', label: 'Hidden' }, { value: 'short', label: 'Shortened' }, { value: 'full', label: 'Full' }]} />
      </FormField>
      <FormField label="Logger" hint="Only on lines whose layout Bennu could read from the project’s logging configuration.">
        <Select size="sm" fill disabled={guessing} value={s.logger} onchange={(v) => colPatch({ logger: v as typeof s.logger })}
          options={[
            { value: 'abbr', label: 'Abbreviated (c.a.d.FooDao)' }, { value: 'last', label: 'Class name only' },
            { value: 'full', label: 'Full name' }, { value: 'hide', label: 'Hidden' },
          ]} />
      </FormField>
      <FormField label="MDC" hint="The key=value entries a layout prints for each line (tenant, trace id, user). Needs a layout read from the project’s logging configuration.">
        <Select size="sm" fill disabled={guessing} value={s.mdc} onchange={(v) => colPatch({ mdc: v as typeof s.mdc })}
          options={[{ value: 'show', label: 'Shown' }, { value: 'hide', label: 'Hidden' }]} />
      </FormField>
      <FormField label="Hidden MDC keys" hint="Hide only these entries, key and value. Quickest: right-click a line and pick the key.">
        <TokenListInput ariaLabel="Hidden MDC keys" placeholder="trace_id" values={s.hidden_mdc}
          onchange={(v) => colPatch({ hidden_mdc: v })} />
      </FormField>
      <Toggle checked={s.dedupe} label="Collapse repeated lines (×N)" onchange={(v) => colPatch({ dedupe: v })} />
      <p class="note">Only what is drawn changes. Copy, save and the assistant still get the text the program wrote.</p>

    {:else if tab === 'rules'}
      <p class="note">Top to bottom, on the parsed fields. Quickest way to add one: right-click a line.</p>
      {#each s.rules as r, i (i)}
        <div class="rule">
          <div class="r1">
            <Checkbox checked={r.on} ariaLabel="Rule enabled" onchange={(v) => patchRule(i, { on: v })} />
            <Select size="sm" value={r.field} options={FIELDS} onchange={(v) => patchRule(i, { field: v as LogViewRule['field'] })} />
            <Select size="sm" value={r.op} options={OPS} onchange={(v) => patchRule(i, { op: v as LogViewRule['op'] })} />
            <IconButton tooltip="Delete rule" variant="danger" size={22} onclick={() => store.removeRule(i)}><Trash2 size={13} /></IconButton>
          </div>
          <Input size="sm" value={r.value} placeholder="text or regular expression" oninput={(v) => patchRule(i, { value: v })} />
          <div class="r2">
            <Select size="sm" value={r.action} options={ACTIONS} onchange={(v) => patchRule(i, { action: v as LogViewRule['action'] })} />
            {#if r.action === 'highlight'}
              <Select size="sm" value={r.color} options={COLORS} onchange={(v) => patchRule(i, { color: v })} />
            {/if}
          </div>
        </div>
      {:else}
        <p class="note">No rules yet.</p>
      {/each}
      <Button variant="secondary" size="sm" onclick={addRule}>{#snippet iconStart()}<Plus size={13} />{/snippet}Add rule</Button>

    {:else}
      <Toggle checked={s.fold_frames} label="Fold frames that are not mine" onchange={(v) => colPatch({ fold_frames: v })} />
      <FormField label="My packages" hint="Read from the project’s sources. Frames from anywhere else fold into one row; “Caused by:” never does. With no package known, nothing folds.">
        {#if store.detected.length}
          <div class="chips">{#each store.detected as p (p)}<Badge variant="tone" tone="neutral" size="sm" label={p} />{/each}</div>
        {:else}
          <p class="note">None detected in this project.</p>
        {/if}
      </FormField>
      <FormField label="Also mine">
        <TokenListInput ariaLabel="Additional packages" placeholder="com.acme.shared" values={s.extra_own_packages}
          onchange={(v) => colPatch({ extra_own_packages: v })} />
      </FormField>
    {/if}
  </div>
</aside>

<style>
  .lvs { width: 300px; flex: none; display: flex; flex-direction: column; min-height: 0; border-left: 1px solid var(--border-subtle); background: var(--bg-base); }
  .body { flex: 1; min-height: 0; overflow: auto; padding: 12px; display: flex; flex-direction: column; gap: 12px; }
  .note { margin: 0; color: var(--text-muted); font-size: var(--font-size-xs); line-height: 1.5; }
  .rule { display: flex; flex-direction: column; gap: 6px; padding: 8px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); background: var(--bg-elevated); }
  .r1 { display: grid; grid-template-columns: auto 1fr 1fr auto; gap: 6px; align-items: center; }
  .r2 { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  .pat { display: block; margin-top: 2px; font-family: var(--font-code); font-size: var(--font-size-2xs); overflow-wrap: anywhere; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
</style>
