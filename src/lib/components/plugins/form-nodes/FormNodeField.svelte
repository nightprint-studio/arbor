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
    ChevronDown, Plus, Trash2, X as XIcon, Check,
    File as FileIconLucide, FolderOpen,
  } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  import NumberStepper     from '$lib/components/shared/ui/NumberStepper.svelte';
  import Input             from '$lib/components/shared/ui/Input.svelte';
  import Dropdown          from '$lib/components/shared/ui/Dropdown.svelte';
  import RadioGroup        from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle            from '$lib/components/shared/ui/Toggle.svelte';
  import TypePill          from '$lib/components/shared/internal/TypePill.svelte';
  import BranchSelect      from '$lib/components/shared/internal/BranchSelect.svelte';
  import FormNodeInlineEdit from './FormNodeInlineEdit.svelte';
  import { PLUGIN_ICONS }  from '$lib/utils/plugin-icons';

  import type {
    FormNode, FormFieldRange,
    FormTableColumn, FormFieldAutocomplete, FormSelectOption,
  } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';
  import { toArr } from './helpers';

  interface Props {
    node: FormNode;
    ctx:  FormNodeCtx;
  }
  let { node, ctx }: Props = $props();
</script>

<!-- The `tree` node has its own sub-renderer (FormNodeTree) — routed before
     this catch-all in FormNodeRenderer. -->

<!-- ────────────────────────────────────────────────────────────────── -->

{#if (node.type as string) === 'field'}
  <!-- ── Plugin-emitted leaf field (single value, fires `action` on
       commit). Used by reflection-based UIs (e.g. bevy-brp). -->
  {@const n = node as any}
  {@const fk = (n.kind ?? 'readonly') as string}
  {@const ro = !!n.readonly}
  <!-- Commit slot: `dispatch` (object) goes scoped `{node_id,slot,value}`;
       legacy `action` (string) keeps the `{...payload, value}` shape. -->
  {@const leafFire = (v: unknown) => {
    if (ro) return;
    if (n.dispatch) ctx.handleScopedDispatch(n.id, 'change', n.dispatch, v, { stateKeys: n.scope_state });
    else if (n.action) ctx.firePluginAction(ctx.pluginName, n.action, JSON.stringify({ ...(n.payload ?? {}), value: v }));
  }}
  <div
    class="pf-field pf-field-leaf {(node as any).class ?? ''}"
    class:pf-field-compact={n.compact}
    class:pf-field-highlight={n.highlight}
    style={(node as any).style}
  >
    {#if n.label}
      <!-- Same rule as the value-bearing branch below: in a compact row the tooltip carries
           the hint, or the label when the fixed-width column clipped it. -->
      <!-- svelte-ignore a11y_label_has_associated_control -->
      <label class="pf-label" use:tooltip={n.compact ? (n.hint ?? n.label ?? '') : ''}>
        {n.label}
      </label>
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
        onchange={(v) => leafFire(v)}
      />

    {:else if fk === 'text'}
      <input
        class="pf-input"
        type="text"
        value={String(n.value ?? '')}
        readonly={ro}
        disabled={ctx.disabled}
        onchange={(e) => leafFire((e.currentTarget as HTMLInputElement).value)}
      />

    {:else if fk === 'checkbox' || fk === 'toggle'}
      <label class="pf-checkbox-row">
        <input
          type="checkbox"
          checked={!!n.value}
          disabled={ro || ctx.disabled}
          onchange={(e) => leafFire((e.currentTarget as HTMLInputElement).checked)}
        />
      </label>

    {:else if fk === 'select'}
      {@const opts = toArr<any>(n.options)}
      <select
        class="pf-input pf-select-trigger"
        value={String(n.value ?? '')}
        disabled={ro || ctx.disabled}
        onchange={(e) => leafFire((e.currentTarget as HTMLSelectElement).value)}
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
      <!-- In a compact row the tooltip carries the hint, and the label when there is no
           hint — the column is fixed-width, so a long name is clipped and needs somewhere to
           be read in full. The hint moves here rather than staying a line below because
           `compact` means "this row is one line", and a sentence under every control in a
           ten-parameter material is the panel that made compact worth having. -->
      <label
        class="pf-label"
        for="pf-{n.name}"
        use:tooltip={n.compact ? (n.hint ?? n.label ?? '') : ''}
      >
        {n.label}
        {#if n.required}<span class="pf-required" aria-hidden="true">*</span>{/if}
      </label>
    {/if}

    {#if node.type === 'text' || node.type === 'password' || node.type === 'email' || node.type === 'url'}
      {@const IconL = n.icon ? PLUGIN_ICONS[n.icon] : null}
      {@const IconR = n.icon_end ? PLUGIN_ICONS[n.icon_end] : null}
      {@const sz    = (n.size as 'sm' | 'md' | 'lg' | undefined) ?? 'md'}
      {@const ipx   = sz === 'sm' ? 12 : sz === 'lg' ? 16 : 14}
      {#snippet textIconStart()}{#if IconL}<IconL size={ipx} />{/if}{/snippet}
      {#snippet textIconEnd()}{#if IconR}<IconR size={ipx} />{/if}{/snippet}
      <Input
        id="pf-{n.name}"
        type={node.type}
        size={sz}
        prefix={n.prefix}
        suffix={n.suffix}
        iconStart={IconL ? textIconStart : undefined}
        iconEnd={IconR ? textIconEnd : undefined}
        placeholder={n.placeholder ?? ''}
        readonly={n.readonly ?? false}
        disabled={ctx.resolvedDisabled(n)}
        clearable={n.clearable ?? false}
        error={ctx.validationErrors[n.name] ?? null}
        oninput={(v) => {
          ctx.notifyChange(n.name, v);
          if (n.actions?.change) ctx.fireFieldChangeDebounced(n, v, n.debounce_ms ?? 250);
        }}
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
        oninput={() => {
          const v = ctx.values[n.name];
          ctx.notifyChange(n.name, v);
          if (n.actions?.change) ctx.fireFieldChangeDebounced(n, v, n.debounce_ms ?? 250);
        }}
        bind:value={ctx.values[n.name]}
      ></textarea>

    {:else if node.type === 'inline_edit'}
      <FormNodeInlineEdit
        value={String(ctx.values[n.name] ?? '')}
        placeholder={n.placeholder}
        size={n.size ?? 'sm'}
        maxlength={n.maxlength}
        requireValue={n.require_value ?? true}
        readonly={(n.readonly ?? false) || ctx.resolvedDisabled(n)}
        displayPlaceholder={n.display_placeholder ?? n.placeholder ?? '—'}
        onCommit={(v) => { ctx.values[n.name] = v; ctx.notifyChange(n.name, v); }}
      />

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
      {@const IconLn = n.icon ? PLUGIN_ICONS[n.icon] : null}
      {@const IconRn = n.icon_end ? PLUGIN_ICONS[n.icon_end] : null}
      {@const szn    = (n.size as 'sm' | 'md' | 'lg' | undefined) ?? 'md'}
      {@const ipxn   = szn === 'sm' ? 12 : szn === 'lg' ? 16 : 14}
      {#snippet numIconStart()}{#if IconLn}<IconLn size={ipxn} />{/if}{/snippet}
      {#snippet numIconEnd()}{#if IconRn}<IconRn size={ipxn} />{/if}{/snippet}
      <NumberStepper
        id="pf-{n.name}"
        bind:value={ctx.values[n.name]}
        min={n.min}
        max={n.max}
        step={n.step ?? 1}
        readonly={n.readonly ?? false}
        disabled={ctx.resolvedDisabled(n)}
        narrow={false}
        size={szn}
        prefix={n.prefix}
        suffix={n.suffix}
        iconStart={IconLn ? numIconStart : undefined}
        iconEnd={IconRn ? numIconEnd : undefined}
        ariaLabel={n.label ?? n.name}
        placeholder={n.placeholder}
        oninput={(v) => {
          ctx.notifyChange(n.name, v);
          if (n.actions?.change) ctx.fireFieldChangeDebounced(n, v, n.debounce_ms ?? 250);
        }}
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
          oninput={() => {
            const v = ctx.values[n.name];
            ctx.notifyChange(n.name, v);
            if (n.actions?.change) ctx.fireFieldChangeDebounced(n, v, n.debounce_ms ?? 250);
          }}
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
          checked={!!ctx.values[n.name]}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLInputElement).checked;
            ctx.values[n.name] = v;
            ctx.fireFieldChange(node, v);
          }}
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
                              n,
                            )}
      {@const placeholder = (n as any).placeholder ?? '— select —'}
      {@const hasValue    = ctx.values[n.name] != null && ctx.values[n.name] !== ''}
      {@const selectedLabel = hasValue
                                ? (ctx.selectLabelOf(rawOpts, ctx.values[n.name] as string) ?? String(ctx.values[n.name]))
                                : null}
      {@const isDisabled  = (n.readonly ?? false) || ctx.resolvedDisabled(n)}
      {@const itemCount   = ctx.selectItemCount(rawOpts)}
      {@const showClear   = !!n.clearable && hasValue && !isDisabled}
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
          <div class="pf-select-trigger-wrap" class:pf-select-trigger-wrap-clearable={showClear}>
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
            {#if showClear}
              <button
                class="pf-select-clear"
                type="button"
                tabindex={-1}
                aria-label="Clear selection"
                use:tooltip={'Clear'}
                onclick={(e) => {
                  e.stopPropagation();
                  ctx.values[n.name] = '';
                  ctx.notifyChange(n.name, '');
                  ctx.fireFieldChange(node, '');
                }}
              ><XIcon size={11} /></button>
            {/if}
          </div>
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
      {@const showClear   = !!n.clearable && cur.length > 0 && !isDisabled}
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
          <div class="pf-select-trigger-wrap" class:pf-select-trigger-wrap-clearable={showClear}>
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
            {#if showClear}
              <button
                class="pf-select-clear"
                type="button"
                tabindex={-1}
                aria-label="Clear selection"
                use:tooltip={'Clear'}
                onclick={(e) => {
                  e.stopPropagation();
                  ctx.values[n.name] = [];
                  ctx.notifyChange(n.name, []);
                }}
              ><XIcon size={11} /></button>
            {/if}
          </div>
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
        appearance={(n as any).appearance ?? 'radio'}
        size={(n as any).size ?? 'md'}
        direction={n.inline ? 'horizontal' : 'vertical'}
        disabled={(n.readonly ?? false) || ctx.resolvedDisabled(n)}
        onchange={(v) => {
          ctx.values[n.name] = v;
          ctx.fireFieldChange(node, v);
        }}
      />

    {:else if node.type === 'toggle'}
      <div class="pf-toggle-row">
        <Toggle
          checked={ctx.values[n.name] as boolean}
          disabled={(n.readonly ?? false) || ctx.resolvedDisabled(n)}
          size={(n.size as any) ?? 'md'}
          label={n.label}
          description={n.description ?? n.hint}
          onchange={(v) => {
            ctx.values[n.name] = v;
            ctx.fireFieldChange(node, v);
          }}
        />
        {#if n.required}<span class="pf-required" aria-hidden="true">*</span>{/if}
      </div>

    {:else if node.type === 'color'}
      <!-- `show_hex = false` leaves the swatch alone, which then takes the whole
           control column — the shape a dense colour row wants (mirrors the
           `show_value` opt-out on `range`). Both inputs fire `actions.change`:
           without it a colour was the one editable field whose value never left
           the panel. -->
      <div class="pf-color-row" class:pf-color-swatch-only={n.show_hex === false}>
        <input
          id="pf-{n.name}"
          class="pf-color-swatch"
          type="color"
          disabled={(n.readonly ?? false) || ctx.resolvedDisabled(n)}
          oninput={() => {
            const v = ctx.values[n.name];
            ctx.notifyChange(n.name, v);
            if (n.actions?.change) ctx.fireFieldChangeDebounced(n, v, n.debounce_ms ?? 250);
          }}
          bind:value={ctx.values[n.name]}
        />
        {#if n.show_hex !== false}
          <input
            class="pf-input pf-color-hex"
            type="text"
            placeholder="#000000"
            disabled={ctx.resolvedDisabled(n)}
            oninput={() => {
              const v = ctx.values[n.name];
              ctx.notifyChange(n.name, v);
              if (n.actions?.change) ctx.fireFieldChangeDebounced(n, v, n.debounce_ms ?? 250);
            }}
            bind:value={ctx.values[n.name]}
          />
        {/if}
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

    {:else if node.type === 'branch_select'}
      <BranchSelect
        bind:value={ctx.values[n.name]}
        branches={Array.isArray(n.branches) ? n.branches : []}
        loading={!!n.loading}
        disabled={(n.readonly ?? false) || ctx.resolvedDisabled(n)}
        placeholder={n.placeholder ?? '— pick a branch —'}
        searchThreshold={typeof n.search_threshold === 'number' ? n.search_threshold : 12}
      />

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
              {#each toArr<string>(n.suggestions) as sug}<option value={sug}></option>{/each}
            </datalist>
          {/if}
        {/if}
      </div>

    {:else if node.type === 'table'}
      {@const rows = Array.isArray(ctx.values[n.name]) ? (ctx.values[n.name] as Record<string, any>[]) : []}
      {@const cols = toArr<FormTableColumn>(n.columns)}
      {@const tableReadonly = n.readonly ?? false}
      {@const rowActions = Array.isArray(n.row_actions) ? n.row_actions : []}
      {@const showDeleteSlot = !n.hide_delete && !tableReadonly}
      {@const trailCount = rowActions.length + (showDeleteSlot ? 1 : 0)}
      {@const trailWidth = trailCount > 0 ? `${trailCount * 24 + Math.max(0, trailCount - 1) * 2 + 4}px` : ''}
      {@const cols_template = cols.map(c => c.width ?? '1fr').join(' ') + (trailWidth ? ' ' + trailWidth : '')}
      {@const tableEmpty = rows.length === 0}
      {@const wantsScroll = !!(n.max_height || n.sticky_header)}
      <div class="pf-list" class:pf-list-empty={tableEmpty}>
        <div
          class="pf-list-scrollable"
          class:pf-list-scrollable-active={wantsScroll}
          style={n.max_height ? `--pf-list-max-h: ${n.max_height}` : ''}
        >
          <div
            class="pf-list-header"
            class:pf-list-header-sticky={n.sticky_header}
            style="grid-template-columns:{cols_template}"
          >
            {#each cols as c (c.key)}
              <span class="pf-list-th" style={c.align ? `text-align:${c.align}` : ''}>{c.label}</span>
            {/each}
            {#if trailCount > 0}<span></span>{/if}
          </div>
          {#if tableEmpty}
            <div class="pf-list-empty-state">No rows yet</div>
          {/if}
          {#each rows as row, ri (ri)}
            <div class="pf-list-row" style="grid-template-columns:{cols_template}" in:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}>
              {#each cols as c (c.key)}
                {@const cellReadonly = c.readonly || tableReadonly}
                {@const cellAlign = c.align ?? (c.type === 'checkbox' ? 'center' : c.type === 'number' ? 'right' : 'left')}
                {#if cellReadonly}
                  {#if c.type === 'checkbox'}
                    <span class="pf-list-readonly pf-list-readonly-bool" style="text-align:{cellAlign}">
                      {#if row[c.key]}<Check size={12} />{/if}
                    </span>
                  {:else if c.type === 'select'}
                    {@const lbl = ctx.normalizeOptions(c.options).find(o => o.value === row[c.key])?.label ?? (row[c.key] == null ? '' : String(row[c.key]))}
                    <span class="pf-list-readonly" style="text-align:{cellAlign}">{lbl}</span>
                  {:else}
                    {@const v = row[c.key]}
                    <span class="pf-list-readonly" class:pf-list-readonly-num={c.type === 'number'} style="text-align:{cellAlign}">
                      {v == null ? '' : String(v)}
                    </span>
                  {/if}
                {:else if c.type === 'checkbox'}
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
              {#if trailCount > 0}
                <div class="pf-list-actions">
                  {#each rowActions as ra, ai (ra.id ?? ai)}
                    {@const RaIcon = (ra.icon && PLUGIN_ICONS[ra.icon]) || PLUGIN_ICONS.Circle}
                    {@const raAriaLabel = ra.label ?? ra.id ?? 'Action'}
                    <button
                      class="pf-row-action"
                      class:pf-row-action-danger={ra.danger}
                      type="button"
                      disabled={ra.disabled || tableReadonly}
                      aria-label={raAriaLabel}
                      use:tooltip={ra.label ?? ''}
                      onclick={() => {
                        const action_id = ra.id ?? `__action_${ai}`;
                        const payload = { row_index: ri, row, action_id };
                        if (ra.dispatch) {
                          ctx.handleScopedDispatch(n.id, 'row_action', ra.dispatch, payload, { stateKeys: n.scope_state });
                        } else if (typeof ra.action === 'string') {
                          ctx.firePluginAction(ctx.pluginName, ra.action, JSON.stringify(payload));
                        }
                      }}
                    >
                      <RaIcon size={12} />
                    </button>
                  {/each}
                  {#if showDeleteSlot}
                    {#if (!n.min_rows || rows.length > n.min_rows)}
                      <button
                        class="pf-list-del"
                        type="button"
                        aria-label="Remove row"
                        use:tooltip={'Remove'}
                        onclick={() => { ctx.values[n.name] = rows.filter((_, j) => j !== ri); }}
                      ><Trash2 size={11} /></button>
                    {:else}
                      <span class="pf-row-action-spacer"></span>
                    {/if}
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
        {#if !tableReadonly && !n.hide_add && (!n.max_rows || rows.length < n.max_rows)}
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
    <!-- Not in a compact row: there it is the label's tooltip, above. -->
    {#if n.hint && !n.compact}
      <span class="pf-hint">{n.hint}</span>
    {/if}
    {#if n.pill}
      <TypePill label={n.pill} kind={n.pill_kind ?? n.pill} tooltip={n.pill_tooltip} />
    {/if}
  </div>
{/if}
