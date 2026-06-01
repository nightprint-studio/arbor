<!--
  FormNodeLayout — renders all "structural" / "content" node types that
  don't carry an editable value of their own:
    container, row, section, copy_link, icon, separator, paragraph, alert,
    code, label, divider, info_card, chip_bar, form_field, tabs,
    tree_layout, wizard, card_row, cfg_list, switch, breadcrumb,
    url_block, monogram, state_block, step_indicator, status_list,
    copy_button, experimental_badge, section_header, filter_button,
    panel_shell, bottom_panel_header, tooltip, color_swatch,
    kbd, type_pill, encoding_pill, avatar, brand_icon, brand_tile,
    provider_user_badge.

  Receives:
    · node       — the FormNode to render
    · ctx        — shared FormNodeCtx with state proxies + helpers
    · renderNode — recursive snippet from the dispatcher
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import {
    PanelLeftClose, PanelLeftOpen, ChevronRight,
    Plus, Pencil, Trash2, Copy, MoreHorizontal,
  } from 'lucide-svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import PluginIcon from '$lib/components/plugins/PluginIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { highlightCode } from '$lib/utils/highlight';

  import Alert     from '$lib/components/shared/ui/Alert.svelte';
  import Callout   from '$lib/components/shared/ui/Callout.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import ExperimentalBadge from '$lib/components/shared/ui/ExperimentalBadge.svelte';
  import SectionHeader     from '$lib/components/shared/ui/SectionHeader.svelte';
  import FilterButton      from '$lib/components/shared/ui/FilterButton.svelte';
  import PanelShell        from '$lib/components/shared/ui/PanelShell.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import InfoCard  from '$lib/components/shared/ui/InfoCard.svelte';
  import ChipBar   from '$lib/components/shared/ui/ChipBar.svelte';
  import Tabs      from '$lib/components/shared/ui/Tabs.svelte';
  import type { TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import StepIndicator from '$lib/components/shared/ui/StepIndicator.svelte';
  import type { Step as StepIndicatorStep } from '$lib/components/shared/ui/StepIndicator.svelte';
  import Breadcrumb from '$lib/components/shared/ui/Breadcrumb.svelte';
  import type { BreadcrumbSegment } from '$lib/components/shared/ui/Breadcrumb.svelte';
  import UrlBlock     from '$lib/components/shared/ui/UrlBlock.svelte';
  import Monogram     from '$lib/components/shared/ui/Monogram.svelte';
  import StateBlock   from '$lib/components/shared/ui/StateBlock.svelte';
  import StatusList   from '$lib/components/shared/ui/StatusList.svelte';
  import type { StatusItem, StatusChip, Severity as StatusSeverity } from '$lib/components/shared/ui/StatusList.svelte';
  import ColorSwatch  from '$lib/components/shared/ui/ColorSwatch.svelte';
  import Kbd          from '$lib/components/shared/internal/Kbd.svelte';
  import TypePill     from '$lib/components/shared/internal/TypePill.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import Avatar       from '$lib/components/shared/internal/Avatar.svelte';
  import BrandIcon    from '$lib/components/shared/internal/BrandIcon.svelte';
  import BrandTile    from '$lib/components/shared/internal/BrandTile.svelte';
  import ProviderUserBadge from '$lib/components/shared/internal/ProviderUserBadge.svelte';
  import Spinner      from '$lib/components/shared/ui/Spinner.svelte';
  import { AlertCircle, CheckCircle2, Info as InfoIcon } from 'lucide-svelte';

  import type { FormNode } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';

  interface Props {
    node:       FormNode;
    ctx:        FormNodeCtx;
    renderNode: Snippet<[FormNode]>;
  }
  let { node, ctx, renderNode }: Props = $props();

  // ── tree_layout: resizable nav width helpers ─────────────────────────────
  function parseTlPx(v: unknown, fallback: number): number {
    if (typeof v === 'number' && Number.isFinite(v)) return v;
    if (typeof v === 'string') {
      const m = v.match(/^\s*(-?\d+(?:\.\d+)?)\s*px\s*$/i);
      if (m) return Number(m[1]);
      const raw = Number(v);
      if (Number.isFinite(raw)) return raw;
    }
    return fallback;
  }
  let navResizeDragging = $state(false);
  function startNavResize(e: MouseEvent, id: string, minPx: number, maxPx: number) {
    if (!id) return;
    e.preventDefault();
    navResizeDragging = true;
    const aside = (e.currentTarget as HTMLElement).closest('.pf-tl-nav') as HTMLElement | null;
    const startX = e.clientX;
    const startW = ctx.treeLayoutNavWidth[id] ?? aside?.getBoundingClientRect().width ?? 240;
    let pending  = startW;

    const onMove = (ev: MouseEvent) => {
      const next = Math.max(minPx, Math.min(maxPx, startW + (ev.clientX - startX)));
      pending = next;
      // Mutate the CSS var directly for zero-cost drag feedback.
      if (aside) aside.style.setProperty('--pf-tl-nav-w', `${next}px`);
    };
    const onUp = () => {
      navResizeDragging = false;
      ctx.setTreeLayoutNavWidth(id, pending);
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup',   onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup',   onUp);
  }
  function onNavResizeKey(e: KeyboardEvent, id: string, minPx: number, maxPx: number) {
    if (!id) return;
    const fwd  = e.key === 'ArrowRight';
    const back = e.key === 'ArrowLeft';
    if (!fwd && !back) return;
    e.preventDefault();
    const step  = e.shiftKey ? 32 : 8;
    const cur   = ctx.treeLayoutNavWidth[id] ?? 240;
    const next  = Math.max(minPx, Math.min(maxPx, cur + (fwd ? step : -step)));
    ctx.setTreeLayoutNavWidth(id, next);
  }
</script>

{#if node.type === 'container'}
  <div class="pf-container {(node as any).class ?? ''}" style={ctx.containerStyle(node as any)}>
    {#each (node as any).children as child (child.id)}
      {@render renderNode(child)}
    {/each}
  </div>

{:else if node.type === 'row'}
  <div class="pf-row {(node as any).class ?? ''}" style={ctx.rowStyle(node as any)}>
    {#each (node as any).children as child (child.id)}
      {@render renderNode(child)}
    {/each}
  </div>

<!-- ── section ──────────────────────────────────────────────────────── -->
{:else if node.type === 'section'}
  {#if (node as any).card}
    <!-- Card-style section (sidebar forms).
         Supports:
           · `count`          — numeric badge in the header
           · `add_action`     — quick "+" icon button (legacy)
           · `header_actions` — [{icon, tooltip, action, extra, disabled,
                                  variant}] arbitrary header buttons
           · `collapsible`    — toggle-open header; persists state in
                                 ctx.collapsedMap keyed by node.id
           · `variant="component"` — IntelliJ-style data card with
                                 status dot, dim namespace prefix on the
                                 title and dense 2-column body
           · `dense`          — 2-column auto grid body
           · `status_dot`     — { tone, tooltip }
           · `subtitle`       — small dim caption under the title -->
    {@const cn = node as any}
    {@const isCollapsed = cn.collapsible && ctx.collapsedMap[node.id!]}
    {@const isComponent = cn.variant === 'component'}
    {@const isDense     = !!cn.dense}
    {@const ttl         = (node as any).title ?? ''}
    {@const lastSep     = isComponent ? ttl.lastIndexOf('::') : -1}
    {@const tlNs        = lastSep > 0 ? ttl.slice(0, lastSep + 2) : ''}
    {@const tlName      = lastSep > 0 ? ttl.slice(lastSep + 2)    : ttl}
    {@const dot         = cn.status_dot}
    {@const hasHeader   = !!(ttl || cn.collapsible || cn.count !== undefined || cn.add_action || (Array.isArray(cn.header_actions) && cn.header_actions.length > 0))}
    <div
      class="pf-card {(node as any).class ?? ''}"
      class:pf-card-collapsed={isCollapsed}
      class:pf-card-component={isComponent}
      class:pf-card-dense={isDense}
      class:pf-card-headless={!hasHeader}
      style={(node as any).style}
    >
      {#snippet cardTitleInner()}
        {#if cn.collapsible}
          <ChevronRight
            size={13}
            class="pf-chevron {isCollapsed ? '' : 'pf-chevron-open'}"
          />
        {/if}
        {#if isComponent && dot}
          <span
            class="pf-status-dot"
            data-tone={dot.tone ?? 'muted'}
            title={dot.tooltip ?? undefined}
          ></span>
        {/if}
        {#if isComponent && tlNs}
          <span class="pf-card-title-text">
            <span class="pf-card-title-ns">{tlNs}</span><span class="pf-card-title-name">{tlName}</span>
          </span>
        {:else}
          <span class="pf-card-title-text">{ttl}</span>
        {/if}
        <div class="pf-card-title-actions">
          {#if cn.count !== undefined}
            <span class="pf-counter">{cn.count}</span>
          {/if}
          {#if Array.isArray(cn.header_actions)}
            {#each cn.header_actions as act, k (act.action + ':' + k)}
              {@const HIcon = act.icon ? PLUGIN_ICONS[act.icon] : null}
              <button
                class="pf-ghost-icon"
                class:pf-ghost-icon-danger={act.variant === 'danger'}
                type="button"
                disabled={!!act.disabled}
                use:tooltip={act.tooltip ?? ''}
                onclick={(e) => { e.stopPropagation(); ctx.handleButtonAction(act.action, false, act.extra); }}
              >
                {#if HIcon}<HIcon size={12} />{:else}<MoreHorizontal size={12} />{/if}
              </button>
            {/each}
          {/if}
          {#if cn.add_action}
            <button
              class="pf-ghost-icon"
              type="button"
              onclick={(e) => { e.stopPropagation(); ctx.handleButtonAction(cn.add_action, false); }}>
              <Plus size={11} />
            </button>
          {/if}
        </div>
      {/snippet}
      {#if hasHeader}
        {#if cn.collapsible}
          <div
            class="pf-card-title pf-card-title-clickable"
            role="button"
            tabindex="0"
            onclick={() => { ctx.collapsedMap[node.id!] = !ctx.collapsedMap[node.id!]; }}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); ctx.collapsedMap[node.id!] = !ctx.collapsedMap[node.id!]; } }}
          >
            {@render cardTitleInner()}
          </div>
        {:else}
          <div class="pf-card-title">
            {@render cardTitleInner()}
          </div>
        {/if}
      {/if}
      {#if !isCollapsed}
        <div
          class="pf-card-body"
          class:pf-card-body-dense={isDense}
          transition:slide={{ duration: animStore.dPanel, easing: cubicOut }}
        >
          {#if cn.subtitle && isComponent}
            <div class="pf-card-subtitle">{cn.subtitle}</div>
          {/if}
          {#each ctx.sectionBody(node) as child (child.id)}
            {@render renderNode(child)}
          {/each}
        </div>
      {/if}
    </div>
  {:else}
    {@const isCollapsed = (node as any).collapsible && ctx.collapsedMap[node.id!]}
    <div class="pf-section {(node as any).class ?? ''}" style={(node as any).style}>
      {#if (node as any).title !== undefined || (node as any).collapsible}
        <button
          class="pf-section-header"
          class:pf-section-collapsible={(node as any).collapsible}
          onclick={() => { if ((node as any).collapsible) ctx.collapsedMap[node.id!] = !ctx.collapsedMap[node.id!]; }}
          type="button"
          aria-expanded={(node as any).collapsible ? !isCollapsed : undefined}
        >
          {#if (node as any).collapsible}
            <ChevronRight
              size={13}
              class="pf-chevron {isCollapsed ? '' : 'pf-chevron-open'}"
            />
          {/if}
          {#if (node as any).title}
            <span class="pf-section-title">{(node as any).title}</span>
          {/if}
        </button>
      {/if}
      {#if !isCollapsed}
        <div transition:slide={{ duration: animStore.dPanel, easing: cubicOut }}>
          {#if (node as any).description}
            <p class="pf-section-desc">{(node as any).description}</p>
          {/if}
          <div class="pf-section-body">
            {#each ctx.sectionBody(node) as child (child.id)}
              {@render renderNode(child)}
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}

<!-- ── copy_link ─────────────────────────────────────────────────────── -->
{:else if (node.type as string) === 'copy_link'}
  {@const cln = node as any}
  <button
    type="button"
    class="pf-copy-link {cln.font === 'mono' ? 'pf-copy-link-mono' : ''} {(node as any).class ?? ''}"
    style={(node as any).style}
    use:tooltip={cln.tooltip ?? 'Click to copy'}
    onclick={() => copyToClipboard(cln.text ?? '', { successToast: cln.toast ?? 'Copied to clipboard', errorToast: true })}
  >
    <span class="pf-copy-link-text">{cln.text ?? ''}</span>
    <Copy size={11} class="pf-copy-link-glyph" />
  </button>

<!-- ── icon ──────────────────────────────────────────────────────────── -->
{:else if (node.type as string) === 'icon'}
  {@const IN = (node as any).icon ? PLUGIN_ICONS[(node as any).icon] : null}
  {@const vn = (node as any).variant ?? 'default'}
  {#if IN}
    <span class="pf-status-icon pf-status-icon-{vn} {(node as any).class ?? ''}"
          style={(node as any).style}
          use:tooltip={(node as any).tooltip ?? ''}>
      <IN size={(node as any).size ?? 14} />
    </span>
  {/if}

<!-- ── separator ─────────────────────────────────────────────────────── -->
{:else if node.type === 'separator'}
  <div class="pf-separator {(node as any).class ?? ''}" style={(node as any).style} role="separator">
    {#if (node as any).label}<span class="pf-separator-label">{(node as any).label}</span>{/if}
  </div>

<!-- ── paragraph ─────────────────────────────────────────────────────── -->
{:else if node.type === 'paragraph'}
  <!-- `content` is the documented field but many plugins (and the sidebar
       API) write `text` — accept both. -->
  <p
    class="pf-paragraph pf-paragraph-{(node as any).variant ?? 'normal'} {(node as any).class ?? ''}"
    style={(node as any).style}
  >{(node as any).content ?? (node as any).text ?? ''}</p>

<!-- ── alert (banner: Alert; inline: Callout) ────────────────────────── -->
{:else if node.type === 'alert'}
  {@const an = node as any}
  {@const av = (an.variant ?? 'info') as 'info' | 'warning' | 'error' | 'success'}
  <div class={(node as any).class} style={(node as any).style}>
    {#if an.style === 'inline'}
      {@const cv = av === 'error' ? 'danger' : av === 'success' ? 'tip' : av}
      <Callout variant={cv}>{an.text ?? ''}</Callout>
    {:else}
      <Alert variant={av} text={an.text} />
    {/if}
  </div>

<!-- ── code ──────────────────────────────────────────────────────────── -->
{:else if node.type === 'code'}
  {@const cdn = node as any}
  {@const lang = cdn.language as string | undefined}
  <div class="pf-code-wrap {(node as any).class ?? ''}" style={(node as any).style}>
    {#if lang}
      <pre class="pf-code language-{lang}"><code class="language-{lang}">{@html highlightCode(cdn.text ?? '', lang)}</code></pre>
    {:else}
      <pre class="pf-code"><code>{cdn.text}</code></pre>
    {/if}
    {#if cdn.copy}
      <button
        type="button"
        class="pf-code-copy"
        use:tooltip={'Copy to clipboard'}
        aria-label="Copy"
        onclick={() => copyToClipboard(cdn.text ?? '', { successToast: cdn.toast ?? 'Copied to clipboard', errorToast: true })}
      ><Copy size={12} /></button>
    {/if}
  </div>

<!-- ── label ─────────────────────────────────────────────────────────── -->
{:else if node.type === 'label'}
  {@const n = node as any}
  <p class="pf-paragraph pf-paragraph-{n.variant ?? 'normal'} {(node as any).class ?? ''}" style={(node as any).style}>
    {n.text}
  </p>

<!-- ── divider ───────────────────────────────────────────────────────── -->
{:else if node.type === 'divider'}
  <hr class="pf-divider {(node as any).class ?? ''}" style={(node as any).style} />

<!-- ── info_card ─────────────────────────────────────────────────────── -->
{:else if node.type === 'info_card'}
  {@const ic = node as any}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <InfoCard
      title={ic.title}
      subtitle={ic.subtitle}
      icon={ic.icon}
      monogram={ic.monogram}
      accentColor={ic.accent}
      status={ic.status}
      badges={ic.badges ?? []}
      meta={ic.meta ?? []}
      variant={ic.variant ?? 'elevated'}
      bordered={ic.bordered ?? true}
      actions={(ic.actions ?? []).map((a: any) => ({
        icon: a.icon,
        label: a.label,
        tooltip: a.tooltip,
        variant: a.variant,
        disabled: a.disabled,
        onClick: () => ctx.handleButtonAction(a.action, false, a.extra),
      }))}
    />
  </div>

<!-- ── chip_bar ──────────────────────────────────────────────────────── -->
{:else if node.type === 'chip_bar'}
  {@const cb = node as any}
  <div class="pf-chipbar {(node as any).class ?? ''}" style={(node as any).style}>
    <ChipBar
      items={cb.items as any[]}
      selected={ctx.values[cb.name] as any}
      multi={!!cb.multi}
      size={cb.size ?? 'md'}
      onSelect={(sel) => {
        ctx.values[cb.name] = sel as any;
        ctx.notifyChange(cb.name, sel);
        if (cb.action) ctx.handleButtonAction(cb.action, false, { name: cb.name, value: sel });
      }}
    />
  </div>

<!-- ── breadcrumb ────────────────────────────────────────────────────── -->
{:else if node.type === 'breadcrumb'}
  {@const bc = node as any}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <Breadcrumb
      segments={(bc.segments ?? []) as BreadcrumbSegment<any>[]}
      max={bc.max ?? 6}
      editable={!!bc.editable}
      editValue={bc.edit_value ?? ''}
      editPlaceholder={bc.edit_placeholder ?? 'Type a path (e.g. x/y/z)'}
      onSelect={(value, segment, index) => {
        if (bc.action) ctx.handleButtonAction(bc.action, false, {
          value, index, label: segment.label,
        });
      }}
      onCommit={(path) => {
        if (bc.commit_action) ctx.handleButtonAction(bc.commit_action, false, { path });
      }}
    />
  </div>

<!-- ── url_block ─────────────────────────────────────────────────────── -->
{:else if node.type === 'url_block'}
  {@const ub = node as any}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <UrlBlock label={ub.label} value={ub.value ?? ''} copyable={!!ub.copyable} />
  </div>

<!-- ── monogram ──────────────────────────────────────────────────────── -->
{:else if node.type === 'monogram'}
  {@const mg = node as any}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <Monogram
      name={mg.name ?? '?'}
      initials={mg.initials}
      color={mg.color ?? 'var(--accent)'}
      size={mg.size ?? 18}
      variant={mg.variant ?? 'square'}
      disabled={!!mg.disabled}
      title={mg.tooltip}
      fg={mg.fg}
    />
  </span>

<!-- ── state_block ───────────────────────────────────────────────────── -->
{:else if node.type === 'state_block'}
  {@const sb = node as any}
  {@const sbTone = (sb.tone ?? 'neutral') as 'loading' | 'error' | 'success' | 'info' | 'neutral'}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <StateBlock tone={sbTone} label={sb.label} fill={sb.fill !== false}>
      {#snippet spinner()}
        {#if sbTone === 'loading' && sb.spinner !== false}
          <Spinner size={16} />
        {/if}
      {/snippet}
      {#snippet icon()}
        {#if sb.icon}
          <PluginIcon name={sb.icon} size={16} />
        {:else if sbTone === 'error'}
          <AlertCircle size={16} />
        {:else if sbTone === 'success'}
          <CheckCircle2 size={16} />
        {:else if sbTone === 'info'}
          <InfoIcon size={16} />
        {/if}
      {/snippet}
    </StateBlock>
  </div>

<!-- ── step_indicator (visual-only; sibling of the `wizard` container) ─ -->
{:else if node.type === 'step_indicator'}
  {@const si = node as any}
  {@const siSteps = (si.steps ?? []).map((s: any) => ({
    id:    s.id,
    label: s.label,
    icon:  s.icon ? (PLUGIN_ICONS as any)[s.icon] : undefined,
  })) as StepIndicatorStep[]}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <StepIndicator
      steps={siSteps}
      current={si.current ?? siSteps[0]?.id ?? ''}
      layout={si.layout ?? 'horizontal'}
      size={si.size ?? 'md'}
      variant={si.variant ?? 'flat'}
      separator={si.separator}
      collapseLabels={!!si.collapse_labels}
      onStepClick={si.action ? (id, index) => ctx.handleButtonAction(si.action, false, { id, index }) : undefined}
    />
  </div>

<!-- ── status_list ───────────────────────────────────────────────────── -->
{:else if node.type === 'status_list'}
  {@const sl = node as any}
  {@const slItems = (sl.items ?? []).map((it: any) => ({
    id:    String(it.id ?? ''),
    label: String(it.label ?? ''),
    chips: (it.chips ?? []).map((c: any) => ({
      severity: c.severity as StatusSeverity,
      text:     String(c.text ?? ''),
      // Icon: plugin sends a Lucide name string; we map it via PLUGIN_ICONS
      // so unknown names degrade to "no icon" silently rather than crash.
      icon:     c.icon ? (PLUGIN_ICONS as any)[c.icon] : undefined,
    })),
  })) as StatusItem[]}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <StatusList
      items={slItems}
      totalCount={sl.total_count}
      scanning={!!sl.scanning}
      scanningLabel={sl.scanning_label}
      cleanLabel={sl.clean_label}
      noun={sl.noun}
      footnote={sl.footnote}
      maxListHeight={sl.max_list_height ?? 160}
    />
  </div>

<!-- ── copy_button ───────────────────────────────────────────────────── -->
{:else if node.type === 'copy_button'}
  {@const cb = node as any}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <CopyButton
      value={cb.value ?? ''}
      variant={cb.variant ?? 'icon'}
      label={cb.label}
      copiedLabel={cb.copied_label}
      title={cb.tooltip ?? 'Copy to clipboard'}
      toastSuccess={cb.toast_success}
      showErrorToast={cb.show_error_toast !== false}
    />
  </span>

<!-- ── experimental_badge ────────────────────────────────────────────── -->
{:else if node.type === 'experimental_badge'}
  {@const eb = node as any}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <ExperimentalBadge
      title={eb.title}
      description={eb.description}
      size={eb.size ?? 'md'}
      label={eb.label}
    />
  </span>

<!-- ── section_header ────────────────────────────────────────────────── -->
{:else if node.type === 'section_header'}
  {@const sh = node as any}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <SectionHeader title={sh.title ?? ''} description={sh.description} />
  </div>

<!-- ── color_swatch ──────────────────────────────────────────────────── -->
{:else if node.type === 'color_swatch'}
  {@const cs = node as any}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <ColorSwatch
      color={cs.color ?? ''}
      label={cs.label}
      caption={cs.caption}
      noCaption={!!cs.no_caption}
      chipSize={cs.chip_size}
      tooltip={cs.tooltip}
      glyph={cs.glyph}
    />
  </span>

<!-- ── kbd ──────────────────────────────────────────────────────────── -->
{:else if node.type === 'kbd'}
  {@const kb = node as any}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <Kbd
      action={kb.action}
      binding={kb.binding}
      label={kb.label}
      keys={kb.keys}
      size={kb.size ?? 'md'}
      tone={kb.tone ?? 'default'}
      variant={kb.variant ?? 'box'}
    />
  </span>

<!-- ── type_pill ────────────────────────────────────────────────────── -->
{:else if node.type === 'type_pill'}
  {@const tp = node as any}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <TypePill
      label={tp.label}
      kind={tp.kind}
      tone={tp.tone}
      tooltip={tp.tooltip}
    />
  </span>

<!-- ── encoding_pill ────────────────────────────────────────────────── -->
{:else if node.type === 'encoding_pill'}
  {@const ep = node as any}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <EncodingPill
      encoding={ep.encoding ?? ''}
      overridden={!!ep.overridden}
      compact={!!ep.compact}
    />
  </span>

<!-- ── avatar ───────────────────────────────────────────────────────── -->
{:else if node.type === 'avatar'}
  {@const av = node as any}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <Avatar
      name={av.name ?? ''}
      email={av.email ?? ''}
      size={av.size ?? 24}
    />
  </span>

<!-- ── brand_icon (monochrome, currentColor) ─────────────────────────── -->
{:else if node.type === 'brand_icon'}
  {@const bi = node as any}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <BrandIcon
      brand={bi.brand}
      size={bi.size ?? 20}
      title={bi.title}
    />
  </span>

<!-- ── brand_tile (brand-coloured square) ───────────────────────────── -->
{:else if node.type === 'brand_tile'}
  {@const bt = node as any}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <BrandTile
      brand={bt.brand}
      size={bt.size ?? 20}
      tileSize={bt.tile_size}
      disabled={!!bt.disabled}
      title={bt.title}
    />
  </span>

<!-- ── provider_user_badge (avatar + name + secondary, click-to-copy) ─ -->
{:else if node.type === 'provider_user_badge'}
  {@const pb = node as any}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <ProviderUserBadge
      name={pb.name ?? ''}
      secondary={pb.secondary}
      avatarUrl={pb.avatar_url}
      copyable={pb.copyable !== false}
    />
  </div>

<!-- ── bottom_panel_header (header bar only; pair with siblings) ─────── -->
{:else if node.type === 'bottom_panel_header'}
  {@const bp     = node as any}
  {@const BpIcon = bp.icon ? PLUGIN_ICONS[bp.icon] : null}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <BottomPanelHeader
      title={bp.title}
      count={bp.count ?? null}
      hideClose={!bp.close_action}
      onClose={bp.close_action
        ? () => ctx.handleButtonAction(bp.close_action, false)
        : undefined}
    >
      {#if BpIcon}
        {#snippet icon()}<BpIcon size={13} />{/snippet}
      {/if}
      {#if Array.isArray(bp.actions) && bp.actions.length > 0}
        {#snippet actions()}
          {#each bp.actions as actNode (actNode.id)}
            {@render renderNode(actNode)}
          {/each}
        {/snippet}
      {/if}
      {#each (bp.children ?? []) as child (child.id)}
        {@render renderNode(child)}
      {/each}
    </BottomPanelHeader>
  </div>

<!-- ── panel_shell (PanelShell chrome around children) ───────────────── -->
{:else if node.type === 'panel_shell'}
  {@const ps     = node as any}
  {@const PsIcon = ps.icon ? PLUGIN_ICONS[ps.icon] : null}
  {@const psClass = (ps.variant === 'plugin' ? 'plugin-panel-shell ' : '') + ((node as any).class ?? '')}
  <PanelShell
    title={ps.title ?? ''}
    count={ps.count ?? null}
    scrollable={ps.scrollable !== false}
    hideHeader={!!ps.hide_header}
    class={psClass}
  >
    {#if PsIcon}
      {#snippet icon()}<PsIcon size={13} />{/snippet}
    {/if}
    {#if Array.isArray(ps.actions) && ps.actions.length > 0}
      {#snippet actions()}
        {#each ps.actions as actNode (actNode.id)}
          {@render renderNode(actNode)}
        {/each}
      {/snippet}
    {/if}
    {#if Array.isArray(ps.toolbar) && ps.toolbar.length > 0}
      {#snippet toolbar()}
        {#each ps.toolbar as tbNode (tbNode.id)}
          {@render renderNode(tbNode)}
        {/each}
      {/snippet}
    {/if}
    {#if Array.isArray(ps.footer) && ps.footer.length > 0}
      {#snippet footer()}
        {#each ps.footer as ftNode (ftNode.id)}
          {@render renderNode(ftNode)}
        {/each}
      {/snippet}
    {/if}
    {#each (ps.children ?? []) as child (child.id)}
      {@render renderNode(child)}
    {/each}
  </PanelShell>

<!-- ── tooltip (wrap children with the host's singleton tooltip) ─────── -->
{:else if node.type === 'tooltip'}
  {@const tt    = node as any}
  {@const ttOpts = {
    content:     String(tt.content ?? ''),
    description: tt.description,
    shortcut:    tt.shortcut,
    placement:   tt.placement ?? 'auto',
    delay:       tt.delay,
    offset:      tt.offset,
    maxWidth:    tt.max_width,
    maxHeight:   tt.max_height,
    markdown:    tt.markdown,
  }}
  {#if tt.display === 'block'}
    <div class="pf-tooltip-wrap pf-tooltip-wrap-block {(node as any).class ?? ''}"
         style={(node as any).style}
         use:tooltip={ttOpts}>
      {#each (tt.children ?? []) as child (child.id)}
        {@render renderNode(child)}
      {/each}
    </div>
  {:else}
    <span class="pf-tooltip-wrap {(node as any).class ?? ''}"
          style={(node as any).style}
          use:tooltip={ttOpts}>
      {#each (tt.children ?? []) as child (child.id)}
        {@render renderNode(child)}
      {/each}
    </span>
  {/if}

<!-- ── filter_button (action-only chip; active flag is patched in) ───── -->
{:else if node.type === 'filter_button'}
  {@const fb = node as any}
  {@const FbIcon = fb.icon ? PLUGIN_ICONS[fb.icon] : null}
  <span class={(node as any).class ?? ''} style={(node as any).style}>
    <FilterButton
      label={fb.label ?? ''}
      icon={FbIcon ?? undefined}
      count={fb.count ?? 0}
      active={fb.active}
      onClick={() => ctx.handleButtonAction(fb.action, false, fb.extra)}
    />
  </span>

<!-- ── form_field ────────────────────────────────────────────────────── -->
{:else if node.type === 'form_field'}
  {@const ff     = node as any}
  {@const FfIcon = ff.icon ? PLUGIN_ICONS[ff.icon] : null}
  <div class={(node as any).class ?? ''} style={(node as any).style}>
    <FormField
      label={ff.label}
      optionalText={ff.optional_text}
      required={!!ff.required}
      description={ff.description}
      hint={ff.hint}
      error={ff.error ?? null}
      for={ff.for}
    >
      {#if FfIcon}
        {#snippet icon()}<FfIcon size={11} />{/snippet}
      {/if}
      {#if Array.isArray(ff.actions) && ff.actions.length > 0}
        {#snippet actions()}
          {#each ff.actions as actNode (actNode.id)}
            {@render renderNode(actNode)}
          {/each}
        {/snippet}
      {/if}
      {#each (ff.children ?? []) as child (child.id)}
        {@render renderNode(child)}
      {/each}
    </FormField>
  </div>

<!-- ── switch ────────────────────────────────────────────────────────── -->
{:else if node.type === 'switch'}
  {@const s = node as any}
  {@const branch = s.cases?.[String(ctx.values[s.field])] ?? s.default ?? []}
  {#each branch as child (child.id)}
    {@render renderNode(child)}
  {/each}

<!-- ── tabs ──────────────────────────────────────────────────────────── -->
{:else if node.type === 'tabs'}
  {@const tn = node as any}
  {@const tabItems = ((tn.tabs ?? []) as Array<{ id: string; label: string; icon?: string }>).map((t): TabItem => ({
    id:       t.id,
    label:    t.label,
    icon:     t.icon ? PLUGIN_ICONS[t.icon] : undefined,
    iconSize: 12,
  }))}
  <div class="pf-tabs {(node as any).class ?? ''}" style={(node as any).style}>
    <Tabs
      items={tabItems}
      value={ctx.activeTabMap[node.id!] ?? null}
      variant="underline"
      size="md"
      onSelect={(id) => { ctx.activeTabMap[node.id!] = id; }}
    />
    {#each tn.tabs as tab (tab.id)}
      <div
        class="pf-tabpanel"
        class:pf-tabpanel-hidden={ctx.activeTabMap[node.id!] !== tab.id}
        class:pf-tabpanel-flush={!!(tab as any).flush}
        role="tabpanel"
      >
        {#each tab.children ?? [] as child (child.id)}
          {@render renderNode(child)}
        {/each}
      </div>
    {/each}
  </div>

<!-- ── tree_layout (2-col nav + content) ─────────────────────────────── -->
{:else if node.type === 'tree_layout'}
  {@const tl = node as any}
  {@const tlId = node.id ?? ''}
  {@const collapsible = !!tl.nav_collapsible}
  {@const isCollapsed = collapsible && !!ctx.treeLayoutCollapsed[tlId]}
  {@const resizable = !!tl.nav_resizable && !isCollapsed}
  {@const navMinPx = parseTlPx(tl.nav_min_width, 160)}
  {@const navMaxPx = parseTlPx(tl.nav_max_width, 480)}
  {@const navWPx = tlId && ctx.treeLayoutNavWidth[tlId] != null
                     ? ctx.treeLayoutNavWidth[tlId]
                     : null}
  {@const navWidthCss = resizable
                          ? `${navWPx ?? parseTlPx(tl.nav_width, 240)}px`
                          : (tl.nav_width ?? '240px')}
  <div class="pf-tree-layout {(node as any).class ?? ''}"
       class:pf-tl-collapsed={isCollapsed}
       style={(node as any).style}>
    <aside
      class="pf-tl-nav"
      class:pf-tl-nav-collapsed={isCollapsed}
      class:pf-tl-nav-resizable={resizable}
      style="--pf-tl-nav-w:{navWidthCss};--pf-tl-anim:{animStore.dPanel}ms;"
    >
      {#if collapsible}
        <button type="button" class="pf-tl-toggle pf-tl-toggle-float"
                use:tooltip={isCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
                aria-pressed={!isCollapsed}
                onclick={() => ctx.toggleTreeLayoutCollapsed(tlId)}>
          {#if isCollapsed}
            <PanelLeftOpen size={14} />
          {:else}
            <PanelLeftClose size={14} />
          {/if}
        </button>
      {/if}
      <div class="pf-tl-nav-body">
        {#each tl.nav_children ?? [] as child (child.id)}
          {@render renderNode(child)}
        {/each}
      </div>
      {#if tl.nav_footer_children && tl.nav_footer_children.length > 0}
        <div class="pf-tl-nav-footer">
          {#each tl.nav_footer_children as child (child.id)}
            {@render renderNode(child)}
          {/each}
        </div>
      {/if}
      {#if resizable}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="pf-tl-nav-resize-handle"
          role="slider"
          aria-orientation="vertical"
          aria-label="Resize sidebar"
          aria-valuenow={navWPx ?? parseTlPx(tl.nav_width, 240)}
          aria-valuemin={navMinPx}
          aria-valuemax={navMaxPx}
          tabindex="0"
          onmousedown={(e) => startNavResize(e, tlId, navMinPx, navMaxPx)}
          onkeydown={(e) => onNavResizeKey(e, tlId, navMinPx, navMaxPx)}
        ></div>
      {/if}
    </aside>
    <div class="pf-tl-content">
      {#each tl.content_children ?? [] as child (child.id)}
        {@render renderNode(child)}
      {/each}
    </div>
  </div>

<!-- ── wizard ────────────────────────────────────────────────────────── -->
{:else if node.type === 'wizard'}
  {@const wn = node as any}
  {@const curIdx = ctx.wizardStepIndex(wn)}
  {@const wizardSteps = (wn.steps as any[]).map(s => ({
    id:    s.id,
    label: s.label,
    icon:  s.icon ? PLUGIN_ICONS[s.icon] : undefined,
  })) satisfies StepIndicatorStep[]}
  <div class="pf-wizard {(node as any).class ?? ''}" style={(node as any).style}>
    <div class="pf-wizard-rail">
      <StepIndicator steps={wizardSteps} current={wn.steps[curIdx]?.id ?? wn.steps[0]?.id ?? ''} size="sm" />
    </div>
    {#each wn.steps as step, i (step.id)}
      <div
        class="pf-wizard-panel"
        class:pf-wizard-panel-hidden={i !== curIdx}
      >
        {#if step.description}
          <p class="pf-section-desc">{step.description}</p>
        {/if}
        {#each step.children ?? [] as child (child.id)}
          {@render renderNode(child)}
        {/each}
      </div>
    {/each}
  </div>

<!-- ── card_row ──────────────────────────────────────────────────────── -->
{:else if node.type === 'card_row'}
  {@const n = node as any}
  <div class="pf-card-row {(node as any).class ?? ''}" style={(node as any).style}>
    {#if n.label || n.description}
      <div class="pf-row-label">
        {#if n.label}<div class="pf-row-label-title">{n.label}</div>{/if}
        {#if n.description}<div class="pf-row-label-desc">{n.description}</div>{/if}
      </div>
    {/if}
    <div class="pf-row-ctrl">
      {#each n.children ?? [] as child (child.id)}
        {@render renderNode(child)}
      {/each}
    </div>
  </div>

<!-- ── cfg_list ──────────────────────────────────────────────────────── -->
{:else if node.type === 'cfg_list'}
  {@const n = node as any}
  <div class="pf-cfg-list {(node as any).class ?? ''}" style={(node as any).style}>
    {#each n.items ?? [] as item (item.id)}
      {@const CfgIcon = item.icon ? PLUGIN_ICONS[item.icon] : null}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div class="pf-cfg-item"
           class:pf-cfg-active={item.active}
           class:pf-cfg-clickable={!!item.select_action}
           role={item.select_action ? 'button' : undefined}
           tabindex={item.select_action ? 0 : undefined}
           onclick={item.select_action
             ? () => ctx.handleButtonAction(item.select_action, false, { id: item.id })
             : undefined}
           onkeydown={item.select_action
             ? (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); ctx.handleButtonAction(item.select_action, false, { id: item.id }); } }
             : undefined}>
        {#if CfgIcon}
          <CfgIcon size={12} class="pf-cfg-icon" />
        {:else}
          <div class="pf-cfg-dot"></div>
        {/if}
        <span class="pf-cfg-name">{item.label}</span>
        {#each item.tags ?? [] as tag}
          <span class="pf-cfg-tag pf-cfg-tag-{tag.variant ?? 'neutral'}">{tag.text}</span>
        {/each}
        <div class="pf-cfg-actions">
          {#if item.edit_action}
            <button class="pf-icon-btn" type="button" use:tooltip={'Edit'}
              onclick={(e) => { e.stopPropagation(); ctx.handleButtonAction(item.edit_action, false, { id: item.id }); }}>
              <Pencil size={11} />
            </button>
          {/if}
          {#if item.delete_action}
            <button class="pf-icon-btn pf-icon-btn-danger" type="button" use:tooltip={'Delete'}
              onclick={(e) => { e.stopPropagation(); ctx.handleButtonAction(item.delete_action, false, { id: item.id }); }}>
              <Trash2 size={11} />
            </button>
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<!-- All CSS lives in FormNodeRenderer.svelte (the dispatcher) wrapped in
     `:global(...)` so it applies to markup rendered by sub-renderers too. -->
