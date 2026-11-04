<script lang="ts">
  /**
   * Widget catalog — every variant and state of the shared/ui widgets that were extended recently,
   * on one page, so a visual regression shows up in a single screenshot (dark and light).
   * Dev-only: lives under src/lib/dev and is reached through the gallery's `shared/WidgetCatalog` scene.
   */
  import { Plus, Settings, Trash2, Search, Star, GitBranch, X, Play, Bell } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import MarkdownToolbar from '$lib/components/shared/ui/MarkdownToolbar.svelte';
  import { NO_FORMAT } from '$lib/utils/markdown-format';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';

  const buttonVariants = ['primary', 'secondary', 'ghost', 'outline', 'danger', 'tonal', 'link'] as const;
  const buttonSizes = ['xs', 'sm', 'md', 'lg'] as const;
  const tabVariants = ['underline', 'pill', 'panel', 'solid', 'segmented'] as const;
  const tabSizes = ['xs', 'sm', 'md'] as const;
  const tabItems = [
    { id: 'a', label: 'Changes' },
    { id: 'b', label: 'History', icon: GitBranch },
    { id: 'c', label: 'Branches', badge: 3 },
    { id: 'd', label: 'Stashes' },
  ];
  let tabValue = $state<Record<string, string>>({});

  const badgeTones = ['info', 'success', 'warning', 'error', 'accent', 'neutral', 'tag', 'stash'] as const;
  const badgeVariants = ['pill', 'status', 'chip', 'count', 'sync', 'tone'] as const;

  let c1 = $state(true);
  let c2 = $state(false);
  let c3 = $state(false);
  let c4 = $state(true);
  let c5 = $state(true);
  let c6 = $state(false);
  let t1 = $state(true);
  let t2 = $state(false);
  let t3 = $state(true);
  let t4 = $state(false);
  let query = $state('');
  let filterWithText = $state('main');
</script>

<div class="catalog">
  <h1>Widget catalog</h1>

  <section>
    <h2>Button — variants × sizes</h2>
    {#each buttonVariants as v (v)}
      <div class="row">
        <span class="lbl">{v}</span>
        {#each buttonSizes as s (s)}
          <Button variant={v} size={s}>{s} label</Button>
        {/each}
        <Button variant={v}>
          {#snippet iconStart()}<Plus size={12} />{/snippet}
          icon start
        </Button>
        <Button variant={v} disabled>disabled</Button>
        <Button variant={v} loading>loading</Button>
      </div>
    {/each}
    <div class="row">
      <span class="lbl">radius sm</span>
      <Button variant="secondary" radius="sm">radius sm</Button>
      <Button variant="outline" radius="sm">outline sm</Button>
      <span class="lbl">borderTone subtle</span>
      <Button variant="outline" borderTone="subtle">subtle</Button>
      <Button variant="secondary" borderTone="subtle">subtle</Button>
      <Button variant="icon" ariaLabel="Settings">
        {#snippet iconStart()}<Settings size={14} />{/snippet}
      </Button>
      <Button variant="primary" block>block</Button>
    </div>
  </section>

  <section>
    <h2>MarkdownToolbar — nothing active, caret in bold + a task list, disabled, table hidden</h2>
    <div class="col">
      <MarkdownToolbar onCommand={() => {}} />
      <MarkdownToolbar onCommand={() => {}} state={{ ...NO_FORMAT, bold: true, list: 'task', heading: 2 }} />
      <MarkdownToolbar onCommand={() => {}} disabled />
      <MarkdownToolbar onCommand={() => {}} hide={['table']} state={{ ...NO_FORMAT, codeBlock: true }} />
    </div>
  </section>

  <section>
    <h2>IconButton — variants, tone, hoverBg, bordered, active, disabled (hover each to see the state)</h2>
    <div class="row">
      {#each ['ghost', 'accent', 'success', 'danger'] as v (v)}
        <span class="lbl">{v}</span>
        <IconButton variant={v as any} tooltip={`${v} tooltip`}><Star size={14} /></IconButton>
        <IconButton variant={v as any} bordered tooltip="bordered"><Star size={14} /></IconButton>
        <IconButton variant={v as any} active tooltip="active"><Star size={14} /></IconButton>
        <IconButton variant={v as any} disabled tooltip="disabled"><Star size={14} /></IconButton>
      {/each}
    </div>
    <div class="row">
      <span class="lbl">tone=rest</span>
      <IconButton tone="rest" tooltip="rest"><Trash2 size={14} /></IconButton>
      <span class="lbl">tone=hover</span>
      <IconButton tone="hover" tooltip="hover"><Trash2 size={14} /></IconButton>
      <span class="lbl">hoverBg overlay</span>
      <IconButton hoverBg="overlay" tooltip="overlay"><Search size={14} /></IconButton>
      <span class="lbl">elevated</span>
      <IconButton hoverBg="elevated" tooltip="elevated"><Search size={14} /></IconButton>
      <span class="lbl">sizes 18/24/32</span>
      <IconButton size={18} tooltip="18"><Bell size={12} /></IconButton>
      <IconButton size={24} tooltip="24"><Bell size={14} /></IconButton>
      <IconButton size={32} tooltip="32" bordered><Bell size={16} /></IconButton>
      <span class="lbl">shortcut</span>
      <IconButton tooltip="Run" shortcut="Ctrl+Shift+R"><Play size={14} /></IconButton>
    </div>
  </section>

  <section>
    <h2>Checkbox — label, children, labelSize, labelTone, indeterminate, disabled</h2>
    <div class="row">
      <Checkbox bind:checked={c1} label="Label (sm, secondary)" />
      <Checkbox bind:checked={c2} label="Unchecked" />
      <Checkbox bind:checked={c3} indeterminate label="Indeterminate" />
      <Checkbox bind:checked={c4} disabled label="Disabled checked" />
      <Checkbox bind:checked={c2} disabled label="Disabled" />
    </div>
    <div class="row">
      <Checkbox bind:checked={c5} label="labelSize 2xs" labelSize="2xs" />
      <Checkbox bind:checked={c5} label="labelSize xs" labelSize="xs" />
      <Checkbox bind:checked={c5} label="labelSize sm" labelSize="sm" />
      <Checkbox bind:checked={c6} label="tone muted" labelTone="muted" />
      <Checkbox bind:checked={c5}>
        <span>children: <b>rich</b> label with <Kbd label="Ctrl+K" size="sm" /></span>
      </Checkbox>
    </div>
  </section>

  <section>
    <h2>Toggle — sizes, label, description, position</h2>
    <div class="row">
      <Toggle bind:checked={t1} size="sm" label="sm" />
      <Toggle bind:checked={t1} size="md" label="md" />
      <Toggle bind:checked={t1} size="lg" label="lg" />
      <Toggle bind:checked={t2} label="before" labelPosition="before" />
      <Toggle bind:checked={t3} label="With description" description="Explains what flipping this does." />
      <Toggle bind:checked={t4} disabled label="Disabled" />
      <Toggle bind:checked={t3} disabled label="Disabled on" />
    </div>
  </section>

  <section>
    <h2>Tabs — variants × sizes (xs and segmented are new)</h2>
    {#each tabVariants as v (v)}
      <div class="row tabs-row">
        <span class="lbl">{v}</span>
        {#each tabSizes as s (s)}
          <Tabs
            items={tabItems}
            variant={v}
            size={s}
            value={tabValue[`${v}-${s}`] ?? 'a'}
            onSelect={(id) => (tabValue[`${v}-${s}`] = id)}
            ariaLabel={`${v} ${s}`}
          />
        {/each}
      </div>
    {/each}
    <div class="row tabs-row">
      <span class="lbl">closable + add</span>
      <Tabs items={tabItems} variant="underline" size="sm" value="b" closable onAdd={() => {}} onClose={() => {}} onSelect={() => {}} />
      <span class="lbl">fill</span>
      <div style="width:360px"><Tabs items={tabItems.slice(0, 3)} variant="segmented" size="sm" fill value="a" onSelect={() => {}} /></div>
    </div>
  </section>

  <section>
    <h2>Spinner — sizes × variants (delay=0 here so it shows at once; default is 250 ms)</h2>
    <div class="row">
      {#each ['xs', 'sm', 'md', 'lg', 'xl'] as s (s)}
        <Spinner size={s as any} delay={0} />
      {/each}
      <Spinner variant="dots" delay={0} />
      <Spinner variant="bars" delay={0} />
      <Spinner delay={0} label="with label" />
      <Spinner delay={0} color="var(--accent)" />
    </div>
  </section>

  <section>
    <h2>Badge — variants × tones</h2>
    {#each badgeVariants as v (v)}
      <div class="row">
        <span class="lbl">{v}</span>
        {#each badgeTones as t (t)}
          <Badge variant={v} tone={t} label={t} />
        {/each}
        <Badge variant={v} label="dot" dot tone="success" />
        <Badge variant={v} label="sm" size="sm" tone="info" />
      </div>
    {/each}
  </section>

  <section>
    <h2>Alert / StateBlock</h2>
    <div class="col">
      <Alert variant="info" title="Info" text="Informational message with a title." />
      <Alert variant="warning" text="Warning without title, dismissible." dismissible />
      <Alert variant="error" title="Error" text="Something failed." compact />
      <Alert variant="success" text="Saved." noIcon />
    </div>
    <div class="row states">
      <div class="box"><StateBlock tone="loading" label="Loading branches…" /></div>
      <div class="box"><StateBlock tone="error" title="Could not load" message="The remote refused the connection." onretry={() => {}} /></div>
      <div class="box"><StateBlock tone="neutral" title="Nothing here" message="No stashes yet." /></div>
      <div class="box"><StateBlock tone="success" label="All checks passed" /></div>
    </div>
  </section>

  <section>
    <h2>FilterInput / Popover / Kbd</h2>
    <div class="row">
      <div style="width:240px"><FilterInput bind:value={query} placeholder="Filter branches…" /></div>
      <div style="width:240px"><FilterInput bind:value={filterWithText} /></div>
      <Kbd label="Ctrl+Enter" />
      <Kbd label="Alt+Shift+P" />
      <Kbd label="Esc" size="sm" />
      <Kbd label="Ctrl+K" tone="accent" />
      <Kbd label="Ctrl+K" tone="muted" />
      <Kbd label="Ctrl+Shift+E" variant="inline" />
    </div>
  </section>
</div>

<style>
  .catalog { position: fixed; inset: 0; overflow: auto; padding: 18px 24px 80px; background: var(--bg-base); color: var(--text-primary); font-family: var(--font-ui); }
  h1 { font-size: 16px; margin: 0 0 12px; }
  h2 { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-muted); margin: 0 0 8px; font-weight: 600; }
  section { margin-bottom: 22px; padding-bottom: 14px; border-bottom: 1px solid var(--border-subtle, var(--border)); }
  .row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px; margin-bottom: 8px; }
  .col { display: flex; flex-direction: column; gap: 8px; margin-bottom: 10px; max-width: 560px; }
  .lbl { font-size: 10px; color: var(--text-muted); min-width: 64px; }
  .tabs-row { gap: 18px; }
  .states { align-items: stretch; }
  .box { width: 220px; height: 110px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-elevated); display: flex; }
</style>
