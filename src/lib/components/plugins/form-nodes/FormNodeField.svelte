<!--
  FormNodeField — every node type that owns an editable value.
    · the leaf `field` node (plugin-emitted single field from a reflected
      value, with sub-`kind`: readonly | number | text | checkbox/toggle |
      select)
    · text, password, email, url, textarea
    · date / datetime / time
    · number, range
    · checkbox, toggle
    · select, multiselect, radio
    · color
    · file (browse + clear)
    · autocomplete (static or dynamic)
    · tags (chips input)
    · tree (single + multi)
    · table (multi-column rows)
    · kv_list (key=value pairs)

  Trailing validation error / hint / pill chrome is rendered uniformly for
  the non-leaf branches at the bottom.
-->
<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';

  import {
    ChevronDown, Plus, Trash2, Check, X as XIcon,
    File as FileIconLucide, FolderOpen,
  } from 'lucide-svelte';
  import PluginIcon from '$lib/components/plugins/PluginIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Dropdown      from '$lib/components/shared/ui/Dropdown.svelte';
  import RadioGroup    from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle        from '$lib/components/shared/ui/Toggle.svelte';
  import TypePill      from '$lib/components/shared/ui/TypePill.svelte';

  import type {
    FormNode, FormFieldRange,
    FormTableColumn, FormFieldAutocomplete, FormTreeNode, FormSelectOption,
  } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';

  interface Props {
    node: FormNode;
    ctx:  FormNodeCtx;
  }
  let { node, ctx }: Props = $props();
</script>

<!-- ── treeNode snippet (recursive) ──────────────────────────────────── -->
{#snippet treeNode(field: any, tnode: FormTreeNode, depth: number, sel: string | string[], multi: boolean)}
  {@const hasChildren = !!(tnode.children && tnode.children.length)}
  {@const expanded = !!ctx.treeExpanded[ctx.treeKey(field.name, tnode.value)]}
  {@const selected = multi
    ? (sel as string[]).includes(tnode.value)
    : (sel as string) === tnode.value}
  <div class="pf-tree-row" style="padding-left: {depth * 14 + 4}px" role="treeitem" aria-expanded={hasChildren ? expanded : undefined} aria-selected={selected}>
    {#if hasChildren}
      <button
        class="pf-tree-chev"
        type="button"
        aria-label={expanded ? 'Collapse' : 'Expand'}
        onclick={() => (ctx.treeExpanded[ctx.treeKey(field.name, tnode.value)] = !expanded)}
      ><ChevronDown size={10} class={expanded ? '' : 'pf-chev-collapsed'} /></button>
    {:else}
      <span class="pf-tree-chev-spacer"></span>
    {/if}
    {#if tnode.icon}
      <span class="pf-tree-icon"><PluginIcon name={tnode.icon} size={11} /></span>
    {/if}
    <button
      class="pf-tree-label"
      class:pf-tree-label-group={tnode.group}
      class:pf-tree-label-selected={selected}
      type="button"
      disabled={tnode.group}
      onclick={() => {
        if (tnode.group) {
          // Group headers: clicking toggles expansion instead of selecting
          if (hasChildren) {
            ctx.treeExpanded[ctx.treeKey(field.name, tnode.value)] =
              !ctx.treeExpanded[ctx.treeKey(field.name, tnode.value)];
          }
          return;
        }
        if (multi) {
          const arr = sel as string[];
          ctx.values[field.name] = arr.includes(tnode.value)
            ? arr.filter(v => v !== tnode.value)
            : [...arr, tnode.value];
        } else {
          ctx.values[field.name] = tnode.value;
        }
        // Master/detail: fire change_action so plugin can rebuild siblings.
        // Skipped for multi-select (selection shape differs).
        if (!multi && field.change_action) {
          ctx.handleButtonAction(field.change_action, false, { value: tnode.value });
        }
      }}
    >
      {#if multi && !tnode.group}
        <span class="pf-tree-cb" class:checked={selected}>
          {#if selected}<Check size={9} />{/if}
        </span>
      {/if}
      <span class="pf-tree-label-text">
        <span>{tnode.label}</span>
        {#if tnode.description}
          <span class="pf-tree-desc">{tnode.description}</span>
        {/if}
      </span>
      {#if tnode.tag}
        <span class="pf-cfg-tag pf-cfg-tag-{tnode.tag_variant ?? 'neutral'} pf-tree-tag">{tnode.tag}</span>
      {/if}
    </button>
  </div>
  {#if hasChildren && expanded}
    {#each tnode.children ?? [] as child (child.value)}
      {@render treeNode(field, child, depth + 1, sel, multi)}
    {/each}
  {/if}
{/snippet}

<!-- ────────────────────────────────────────────────────────────────── -->

{#if (node.type as string) === 'field'}
  <!-- ── Plugin-emitted leaf field (single value, fires `action` on
       commit). Used by reflection-based UIs (e.g. bevy-brp). -->
  {@const n = node as any}
  {@const fk = (n.kind ?? 'readonly') as string}
  {@const ro = !!n.readonly}
  <div
    class="pf-field pf-field-leaf {(node as any).class ?? ''}"
    class:pf-field-compact={n.compact}
    class:pf-field-highlight={n.highlight}
    style={(node as any).style}
  >
    {#if n.label}
      <!-- svelte-ignore a11y_label_has_associated_control -->
      <label class="pf-label">{n.label}</label>
    {/if}

    {#if fk === 'readonly'}
      <span class="pf-readonly-value">{n.value ?? ''}</span>

    {:else if fk === 'number'}
      <NumberStepper
        value={typeof n.value === 'number' ? n.value : Number(n.value ?? 0)}
        min={n.min}
        max={n.max}
        step={typeof n.step === 'number' ? n.step : 1}
        readonly={ro}
        disabled={ctx.disabled}
        narrow={false}
        ariaLabel={n.label ?? ''}
        onchange={(v) => { if (!ro && n.action) ctx.firePluginAction(ctx.pluginName, n.action, JSON.stringify({ ...(n.payload ?? {}), value: v })); }}
      />

    {:else if fk === 'text'}
      <input
        class="pf-input"
        type="text"
        value={String(n.value ?? '')}
        readonly={ro}
        disabled={ctx.disabled}
        onchange={(e) => {
          if (ro || !n.action) return;
          const v = (e.currentTarget as HTMLInputElement).value;
          ctx.firePluginAction(ctx.pluginName, n.action, JSON.stringify({ ...(n.payload ?? {}), value: v }));
        }}
      />

    {:else if fk === 'checkbox' || fk === 'toggle'}
      <label class="pf-checkbox-row">
        <input
          type="checkbox"
          checked={!!n.value}
          disabled={ro || ctx.disabled}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLInputElement).checked;
            if (!ro && n.action) ctx.firePluginAction(ctx.pluginName, n.action, JSON.stringify({ ...(n.payload ?? {}), value: v }));
          }}
        />
      </label>

    {:else if fk === 'select'}
      {@const opts = (n.options as any[]) ?? []}
      <select
        class="pf-input pf-select-trigger"
        value={String(n.value ?? '')}
        disabled={ro || ctx.disabled}
        onchange={(e) => {
          const v = (e.currentTarget as HTMLSelectElement).value;
          if (!ro && n.action) ctx.firePluginAction(ctx.pluginName, n.action, JSON.stringify({ ...(n.payload ?? {}), value: v }));
        }}
      >
        {#each opts as o}
          {@const ov = typeof o === 'string' ? o : (o.value ?? o.label)}
          {@const ol = typeof o === 'string' ? o : (o.label ?? o.value)}
          <option value={ov}>{ol}</option>
        {/each}
      </select>

    {:else}
      <span class="pf-readonly-value">{String(n.value ?? '')}</span>
    {/if}

    {#if n.pill}
      <TypePill label={n.pill} kind={n.pill_kind ?? n.pill} tooltip={n.pill_tooltip} />
    {/if}
  </div>

{:else}
  <!-- ── Value-bearing field nodes ───────────────────────────────────── -->
  {@const n = node as any}
  <div
    class="pf-field {(node as any).class ?? ''}"
    class:pf-field-compact={n.compact}
    class:pf-field-highlight={n.highlight}
    style={(node as any).style}
  >
    <!-- Label — skipped for checkbox/toggle (have their own inline label). -->
    {#if node.type !== 'checkbox' && node.type !== 'toggle' && n.label}
      <label class="pf-label" for="pf-{n.name}">
        {n.label}
        {#if n.required}<span class="pf-required" aria-hidden="true">*</span>{/if}
      </label>
    {/if}

    {#if node.type === 'text' || node.type === 'password' || node.type === 'email' || node.type === 'url'}
      <input
        id="pf-{n.name}"
        class="pf-input"
        class:pf-input-error={!!ctx.validationErrors[n.name]}
        type={node.type}
        placeholder={n.placeholder ?? ''}
        readonly={n.readonly ?? false}
        disabled={ctx.resolvedDisabled(n)}
        oninput={() => ctx.notifyChange(n.name, ctx.values[n.name])}
        bind:value={ctx.values[n.name]}
      />

    {:else if node.type === 'textarea'}
      <textarea
        id="pf-{n.name}"
        class="pf-input pf-textarea"
        placeholder={n.placeholder ?? ''}
        rows={n.rows ?? 4}
        readonly={n.readonly ?? false}
        disabled={ctx.resolvedDisabled(n)}
        bind:value={ctx.values[n.name]}
      ></textarea>

    {:else if node.type === 'date' || node.type === 'datetime' || node.type === 'time'}
      <input
        id="pf-{n.name}"
        class="pf-input pf-input-datetime"
        type={node.type === 'datetime' ? 'datetime-local' : node.type}
        min={n.min}
        max={n.max}
        readonly={n.readonly ?? false}
        disabled={ctx.resolvedDisabled(n)}
        bind:value={ctx.values[n.name]}
      />

    {:else if node.type === 'number'}
      <NumberStepper
        id="pf-{n.name}"
        bind:value={ctx.values[n.name]}
        min={n.min}
        max={n.max}
        step={n.step ?? 1}
        readonly={n.readonly ?? false}
        disabled={ctx.resolvedDisabled(n)}
        narrow={false}
        ariaLabel={n.label ?? n.name}
        placeholder={n.placeholder}
      />

    {:else if node.type === 'range'}
      <div class="pf-range-row">
        <input
          id="pf-{n.name}"
          class="pf-range"
          type="range"
          min={n.min ?? 0}
          max={n.max ?? 100}
          step={n.step ?? 1}
          disabled={(n.readonly ?? false) || ctx.resolvedDisabled(n)}
          bind:value={ctx.values[n.name]}
        />
        {#if n.show_value !== false}
          <span class="pf-range-value">
            {ctx.fmtRange(n as FormFieldRange, ctx.values[n.name] as number)}
          </span>
        {/if}
      </div>

    {:else if node.type === 'checkbox'}
      <label class="pf-checkbox-row" for="pf-{n.name}">
        <input
          id="pf-{n.name}"
          type="checkbox"
          disabled={(n.readonly ?? false) || ctx.resolvedDisabled(n)}
          bind:checked={ctx.values[n.name]}
        />
        <span class="pf-checkbox-label">
          {n.label}
          {#if n.required}<span class="pf-required" aria-hidden="true">*</span>{/if}
        </span>
      </label>

    {:else if node.type === 'select'}
      {@const rawOpts     = (ctx.resolvedOptions(n) ?? n.options) as FormSelectOption[] | undefined}
      {@const ddItems     = ctx.wrapSelectChange(
                              ctx.buildSelectDropdownItems(rawOpts, n.name, false, ctx.values[n.name]),
                              (n as any).actions?.change,
                            )}
      {@const placeholder = (n as any).placeholder ?? '— select —'}
      {@const selectedLabel = (ctx.values[n.name] != null && ctx.values[n.name] !== '')
                                ? (ctx.selectLabelOf(rawOpts, ctx.values[n.name] as string) ?? String(ctx.values[n.name]))
                                : null}
      {@const isDisabled  = (n.readonly ?? false) || ctx.resolvedDisabled(n)}
      {@const itemCount   = ctx.selectItemCount(rawOpts)}
      <Dropdown
        position="fixed"
        direction="down"
        matchTriggerWidth
        items={ddItems}
        searchable={(n as any).searchable ?? itemCount > 12}
        searchPlaceholder="Filter…"
        emptyMessage={(n as any).empty_message ?? 'No options'}
      >
        {#snippet trigger({ open, toggle })}
          <button
            id="pf-{n.name}"
            class="pf-input pf-select-trigger"
            class:pf-select-trigger-empty={selectedLabel === null}
            onclick={toggle}
            disabled={isDisabled}
            type="button"
            aria-haspopup="listbox"
            aria-expanded={open}
          >
            <span class="pf-select-trigger-label">{selectedLabel ?? placeholder}</span>
            <ChevronDown size={11} />
          </button>
        {/snippet}
      </Dropdown>

    {:else if node.type === 'multiselect'}
      {@const rawOpts     = (ctx.resolvedOptions(n) ?? (n as any).options) as FormSelectOption[] | undefined}
      {@const cur         = (Array.isArray(ctx.values[n.name]) ? ctx.values[n.name] : []) as string[]}
      {@const ddItems     = ctx.buildSelectDropdownItems(rawOpts, n.name, true, cur)}
      {@const placeholder = (n as any).placeholder ?? '— select —'}
      {@const summary     = ctx.multiselectSummary(rawOpts, cur, placeholder)}
      {@const isDisabled  = (n.readonly ?? false) || ctx.resolvedDisabled(n)}
      {@const itemCount   = ctx.selectItemCount(rawOpts)}
      <Dropdown
        position="fixed"
        direction="down"
        matchTriggerWidth
        selectionMode="multiple"
        items={ddItems}
        searchable={(n as any).searchable ?? itemCount > 12}
        searchPlaceholder="Filter…"
        emptyMessage={(n as any).empty_message ?? 'No options'}
      >
        {#snippet trigger({ open, toggle })}
          <button
            id="pf-{n.name}"
            class="pf-input pf-select-trigger"
            class:pf-select-trigger-empty={cur.length === 0}
            onclick={toggle}
            disabled={isDisabled}
            type="button"
            aria-haspopup="listbox"
            aria-expanded={open}
          >
            <span class="pf-select-trigger-label">{summary}</span>
            <ChevronDown size={11} />
          </button>
        {/snippet}
      </Dropdown>

    {:else if node.type === 'radio'}
      {@const opts = ctx.normalizeOptions(ctx.resolvedOptions(n))}
      <RadioGroup
        value={ctx.values[n.name] as string}
        options={opts.map(o => ({
          value: o.value,
          label: o.label,
          description: o.description,
          disabled: o.disabled,
        }))}
        appearance="radio"
        direction={n.inline ? 'horizontal' : 'vertical'}
        disabled={(n.readonly ?? false) || ctx.resolvedDisabled(n)}
        onchange={(v) => { ctx.values[n.name] = v; }}
      />

    {:else if node.type === 'toggle'}
      <div class="pf-toggle-row">
        <Toggle
          checked={ctx.values[n.name] as boolean}
          disabled={(n.readonly ?? false) || ctx.resolvedDisabled(n)}
          size={(n.size as any) ?? 'md'}
          label={n.label}
          description={n.description ?? n.hint}
          onchange={(v) => { ctx.values[n.name] = v; }}
        />
        {#if n.required}<span class="pf-required" aria-hidden="true">*</span>{/if}
      </div>

    {:else if node.type === 'color'}
      <div class="pf-color-row">
        <input
          id="pf-{n.name}"
          class="pf-color-swatch"
          type="color"
          disabled={n.readonly ?? false}
          bind:value={ctx.values[n.name]}
        />
        <input
          class="pf-input pf-color-hex"
          type="text"
          placeholder="#000000"
          bind:value={ctx.values[n.name]}
        />
      </div>

    {:else if node.type === 'file'}
      {@const mode = (n.pick_mode ?? 'file') as 'file' | 'folder' | 'save'}
      <div class="pf-file-row">
        <input
          id="pf-{n.name}"
          class="pf-input pf-file-path"
          type="text"
          placeholder={n.placeholder ?? (mode === 'folder' ? 'No folder selected' : 'No file selected')}
          readonly={n.readonly ?? false}
          bind:value={ctx.values[n.name]}
        />
        <button
          class="pf-file-btn"
          type="button"
          disabled={n.readonly ?? false}
          onclick={() => { ctx.openFilePicker(n.name); }}
          use:tooltip={'Browse…'}
        >
          {#if mode === 'folder'}<FolderOpen size={12} />{:else}<FileIconLucide size={12} />{/if}
          Browse…
        </button>
        {#if ctx.values[n.name]}
          <button
            class="pf-file-clear"
            type="button"
            disabled={n.readonly ?? false}
            aria-label="Clear"
            onclick={() => { ctx.values[n.name] = ''; }}
          ><XIcon size={11} /></button>
        {/if}
      </div>

    {:else if node.type === 'autocomplete'}
      {@const fi = n as FormFieldAutocomplete}
      {@const results = (ctx.autoOpen[fi.id] ? ctx.filterAutocomplete(fi, ctx.values[n.name] ?? '') : [])}
      <div class="pf-auto" role="combobox" aria-expanded={!!ctx.autoOpen[fi.id]} aria-controls="pf-auto-listbox-{n.name}" aria-haspopup="listbox">
        <input
          id="pf-{n.name}"
          class="pf-input"
          type="text"
          placeholder={fi.placeholder ?? ''}
          readonly={n.readonly ?? false}
          autocomplete="off"
          bind:value={ctx.values[n.name]}
          onfocus={() => { ctx.onAutocompleteInput(fi); }}
          oninput={() => { ctx.onAutocompleteInput(fi); }}
          onblur={() => setTimeout(() => { ctx.autoOpen[fi.id] = false; }, 120)}
          onkeydown={(e) => {
            if (!ctx.autoOpen[fi.id]) return;
            const list = results;
            const cur  = ctx.autoActiveIdx[fi.id] ?? 0;
            if (e.key === 'ArrowDown') { e.preventDefault(); ctx.autoActiveIdx[fi.id] = Math.min(list.length - 1, cur + 1); }
            else if (e.key === 'ArrowUp') { e.preventDefault(); ctx.autoActiveIdx[fi.id] = Math.max(0, cur - 1); }
            else if (e.key === 'Enter' && list[cur]) { e.preventDefault(); ctx.pickAutocomplete(fi, list[cur].value); }
            else if (e.key === 'Escape') { ctx.autoOpen[fi.id] = false; }
          }}
        />
        {#if ctx.autoOpen[fi.id] && results.length > 0}
          <div class="pf-auto-menu" id="pf-auto-listbox-{n.name}" role="listbox">
            {#each results as opt, i (opt.value + ':' + i)}
              {#if opt.group && (i === 0 || results[i - 1]?.group !== opt.group)}
                <div class="pf-auto-group">{opt.group}</div>
              {/if}
              <button
                type="button"
                class="pf-auto-item"
                class:active={ctx.autoActiveIdx[fi.id] === i}
                role="option"
                aria-selected={ctx.autoActiveIdx[fi.id] === i}
                onmousedown={(e) => { e.preventDefault(); ctx.pickAutocomplete(fi, opt.value); }}
                onmouseenter={() => { ctx.autoActiveIdx[fi.id] = i; }}
              >
                <span class="pf-auto-label">{opt.label}</span>
                {#if opt.value !== opt.label}
                  <span class="pf-auto-value">{opt.value}</span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>

    {:else if node.type === 'tags'}
      {@const tagsArr = Array.isArray(ctx.values[n.name]) ? ctx.values[n.name] as string[] : []}
      <div class="pf-tags">
        {#each tagsArr as tag, i (tag + ':' + i)}
          <span class="pf-chip">
            <span>{tag}</span>
            {#if !(n.readonly ?? false)}
              <button
                class="pf-chip-x"
                type="button"
                aria-label="Remove"
                onclick={() => { ctx.values[n.name] = tagsArr.filter((_, j) => j !== i); }}
              ><XIcon size={9} /></button>
            {/if}
          </span>
        {/each}
        {#if !(n.readonly ?? false) && (!n.max || tagsArr.length < n.max)}
          <input
            class="pf-chip-input"
            type="text"
            placeholder={n.placeholder ?? (tagsArr.length === 0 ? 'Type and press Enter…' : '')}
            list={n.suggestions ? `pf-tagsrc-${n.id}` : undefined}
            onkeydown={(e) => {
              const target = e.currentTarget as HTMLInputElement;
              if ((e.key === 'Enter' || e.key === ',') && target.value.trim()) {
                e.preventDefault();
                const v = target.value.trim();
                if (n.suggestions && !n.suggestions.includes(v)) return;
                if (!tagsArr.includes(v)) ctx.values[n.name] = [...tagsArr, v];
                target.value = '';
              } else if (e.key === 'Backspace' && !target.value && tagsArr.length) {
                ctx.values[n.name] = tagsArr.slice(0, -1);
              }
            }}
          />
          {#if n.suggestions}
            <datalist id={`pf-tagsrc-${n.id}`}>
              {#each n.suggestions as sug}<option value={sug}></option>{/each}
            </datalist>
          {/if}
        {/if}
      </div>

    {:else if node.type === 'tree'}
      {@const multi = !!n.multi}
      {@const sel = multi ? (Array.isArray(ctx.values[n.name]) ? ctx.values[n.name] as string[] : []) : (ctx.values[n.name] as string)}
      <div class="pf-tree" class:pf-tree-bordered={n.bordered}
           style={n.bordered && n.max_height ? `max-height:${n.max_height}` : ''}
           role="tree">
        {#each (n.nodes ?? []) as root (root.value)}
          {@render treeNode(n, root, 0, sel, multi)}
        {/each}
      </div>

    {:else if node.type === 'table'}
      {@const rows = Array.isArray(ctx.values[n.name]) ? (ctx.values[n.name] as Record<string, any>[]) : []}
      {@const cols = (n.columns ?? []) as FormTableColumn[]}
      {@const cols_template = cols.map(c => c.width ?? '1fr').join(' ') + ' 28px'}
      {@const tableEmpty = rows.length === 0}
      <div class="pf-list" class:pf-list-empty={tableEmpty}>
        <div class="pf-list-header" style="grid-template-columns:{cols_template}">
          {#each cols as c}<span class="pf-list-th">{c.label}</span>{/each}
          <span></span>
        </div>
        {#if tableEmpty}
          <div class="pf-list-empty-state">No rows yet</div>
        {/if}
        {#each rows as row, ri (ri)}
          <div class="pf-list-row" style="grid-template-columns:{cols_template}" in:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}>
            {#each cols as c (c.key)}
              {#if c.type === 'checkbox'}
                <input class="pf-list-cb" type="checkbox" bind:checked={row[c.key]} />
              {:else if c.type === 'number'}
                <NumberStepper
                  bind:value={row[c.key]}
                  placeholder={c.placeholder ?? ''}
                  narrow={false}
                  ariaLabel={c.label}
                />
              {:else if c.type === 'select'}
                {@const copts = ctx.normalizeOptions(c.options)}
                <select class="pf-select pf-list-cell" bind:value={row[c.key]}>
                  {#each copts as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
                </select>
              {:else}
                <input class="pf-input pf-list-cell" type="text" placeholder={c.placeholder ?? ''} bind:value={row[c.key]} />
              {/if}
            {/each}
            {#if !(n.readonly ?? false) && (!n.min_rows || rows.length > n.min_rows)}
              <button
                class="pf-list-del"
                type="button"
                aria-label="Remove row"
                use:tooltip={'Remove'}
                onclick={() => { ctx.values[n.name] = rows.filter((_, j) => j !== ri); }}
              ><Trash2 size={11} /></button>
            {:else}
              <span></span>
            {/if}
          </div>
        {/each}
        {#if !(n.readonly ?? false) && (!n.max_rows || rows.length < n.max_rows)}
          <button
            class="pf-list-add"
            type="button"
            onclick={() => {
              const fresh: Record<string, unknown> = {};
              for (const c of cols) {
                fresh[c.key] = c.type === 'checkbox' ? false : c.type === 'number' ? 0 : '';
              }
              ctx.values[n.name] = [...rows, fresh];
            }}
          ><Plus size={12} /> {n.add_label ?? 'Add row'}</button>
        {/if}
      </div>

    {:else if node.type === 'kv_list'}
      {@const rows = ctx.kvRows[n.name] ?? []}
      {@const kvEmpty = rows.length === 0}
      <div class="pf-list pf-list-kv" class:pf-list-empty={kvEmpty}>
        {#if kvEmpty}
          <div class="pf-list-empty-state">No variables defined</div>
        {/if}
        {#each rows as row, i (i)}
          <div class="pf-list-row pf-list-row-kv" in:fly={{ y: -8, duration: animStore.dFast, easing: cubicOut }} out:fade={{ duration: animStore.dFast }}>
            <input
              class="pf-input pf-list-cell pf-list-cell-key"
              type="text"
              placeholder={n.key_placeholder ?? 'Key'}
              disabled={n.readonly ?? false}
              bind:value={row.key}
            />
            <span class="pf-list-eq">=</span>
            <input
              class="pf-input pf-list-cell pf-list-cell-val"
              type="text"
              placeholder={n.value_placeholder ?? 'Value'}
              disabled={n.readonly ?? false}
              bind:value={row.val}
            />
            {#if !(n.readonly ?? false)}
              <button
                class="pf-list-del"
                type="button"
                aria-label="Remove"
                use:tooltip={'Remove'}
                onclick={() => { ctx.kvRows[n.name] = rows.filter((_, j) => j !== i); }}
              ><Trash2 size={11} /></button>
            {:else}
              <span></span>
            {/if}
          </div>
        {/each}
        {#if !(n.readonly ?? false)}
          <button
            class="pf-list-add"
            type="button"
            onclick={() => { ctx.kvRows[n.name] = [...(ctx.kvRows[n.name] ?? []), { key: '', val: '' }]; }}
          ><Plus size={12} /> Add variable</button>
        {/if}
      </div>
    {/if}

    {#if ctx.validationErrors[n.name]}
      <span class="pf-validation-error">{ctx.validationErrors[n.name]}</span>
    {/if}
    {#if n.hint}
      <span class="pf-hint">{n.hint}</span>
    {/if}
    {#if n.pill}
      <TypePill label={n.pill} kind={n.pill_kind ?? n.pill} tooltip={n.pill_tooltip} />
    {/if}
  </div>
{/if}
