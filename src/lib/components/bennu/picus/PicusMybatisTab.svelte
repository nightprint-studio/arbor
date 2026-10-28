<script lang="ts">
  /**
   * MyBatis mapper statements vs. the database. Each statement is rendered to plain SQL (dynamic
   * tags flattened, `#{}` as NULL) and prepared on the server, so a column or table that does not
   * exist, or a join on a missing column, comes back as the server's own message.
   *
   * `${name}` is text spliced in at runtime — only the project knows what. Each one is listed once,
   * with a field to say what it stands for; with a value the statements using it are checked too.
   * A statement that still cannot be rendered into something preparable says why rather than showing
   * a clean result it did not earn.
   */
  import { Play } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuPicusStore, mybatisKey } from '$lib/stores/bennu/picus.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { openAt } from './open-at';
  import PicusOutcomeCell from './PicusOutcomeCell.svelte';

  let query = $state('');

  const root = $derived(projectStore.project?.root ?? '');
  const statements = $derived(bennuPicusStore.mybatis);
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? statements.filter((s) => `${s.namespace}.${s.id}`.toLowerCase().includes(q) || s.sql.toLowerCase().includes(q)) : statements;
  });
  const anyChecking = $derived(Object.values(bennuPicusStore.mybatisOutcome).some((o) => o.state === 'checking'));

  /** Every `${name}` the mappers splice, once, with the value this project gave it. */
  const splices = $derived.by(() => {
    const given = new Map(root ? bennuSettingsStore.mybatisSplicesFor(root) : []);
    const names = [...new Set(statements.flatMap((s) => s.splices))].sort();
    return names.map((name) => ({ name, value: given.get(name) ?? '' }));
  });
  const unvalued = $derived(splices.filter((s) => !s.value).length);
</script>

{#if statements.length === 0}
  <EmptyState message="No MyBatis statements." description="Mapper XML statements appear here once the project's mappers are indexed." />
{:else}
  {#if splices.length}
    <div class="splices">
      <Callout variant={unvalued ? 'info' : 'tip'} title="Spliced text">
        {#snippet children()}
          <code>$&#123;…&#125;</code> is filled in at runtime, so Bennu cannot know it. Say what each one stands for
          {unvalued ? `— ${unvalued} still without a value leave their statements unchecked.` : '— every one has a value.'}
        {/snippet}
      </Callout>
      {#each splices as s (s.name)}
        <FormField label={'${' + s.name + '}'}>
          <Input size="sm" value={s.value} placeholder="e.g. a table name or a column list"
            onchange={(v) => void bennuPicusStore.setSplice(root, s.name, v)} />
        </FormField>
      {/each}
    </div>
  {/if}

  <div class="tools">
    <SearchBar bind:query showRegex={false} showCounter={false} placeholder="Filter statements…" ariaLabel="Filter statements" />
  </div>
  <div class="bar">
    <span class="count">{shown.length === statements.length ? statements.length : `${shown.length} of ${statements.length}`} {statements.length === 1 ? 'statement' : 'statements'}</span>
    <Button size="xs" variant="tonal" disabled={anyChecking} onclick={() => void bennuPicusStore.checkAllMybatis()}>
      {#snippet iconStart()}<Play size={11} />{/snippet}
      Validate all
    </Button>
  </div>
  <ul class="rows">
    {#each shown as s (mybatisKey(s))}
      {@const outcome = bennuPicusStore.mybatisOutcome[mybatisKey(s)]}
      <li>
        <div class="row">
          <button class="name" type="button" use:tooltip={`${s.namespace}.${s.id}`} onclick={() => openAt(s.file, s.line)}>
            <span class="cls">{s.namespace.split('.').pop()}.{s.id}</span>
            <span class="sql">{s.kind} · {s.sql.replace(/\s+/g, ' ').trim()}</span>
          </button>
          <PicusOutcomeCell {outcome} oncheck={() => void bennuPicusStore.checkMybatis(s)} />
        </div>
        {#if outcome?.state === 'rejected'}
          <ul class="msgs">
            {#each outcome.messages as m, i (i)}<li>{m}</li>{/each}
            {#if s.dynamic}<li class="note">Checked with every dynamic branch included.</li>{/if}
          </ul>
        {:else if outcome?.state === 'unchecked'}
          <ul class="msgs"><li class="note">{outcome.reason}</li></ul>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .splices { display: flex; flex-direction: column; gap: 8px; padding: 8px 12px 4px; }
  .tools { padding: 8px 12px 0; }
  .bar { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 6px 12px; }
  .count { font-size: 11px; color: var(--text-muted); }
  .rows, .msgs { list-style: none; margin: 0; padding: 0; }
  .row { display: flex; align-items: center; gap: 8px; padding: 3px 8px 3px 12px; min-width: 0; }
  .row:hover { background: var(--bg-hover); }
  .name { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; align-items: flex-start; background: none; border: none; padding: 0; text-align: left; cursor: pointer; color: inherit; }
  .cls { font-size: 12px; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .sql { font-size: 10.5px; color: var(--text-muted); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .msgs { padding: 0 12px 4px 12px; }
  .msgs li { font-size: 11px; line-height: 1.5; color: var(--danger, var(--text-secondary)); white-space: pre-wrap; overflow-wrap: anywhere; }
  .msgs li.note { color: var(--text-muted); }
</style>
