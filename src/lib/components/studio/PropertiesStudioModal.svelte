<!--
  PropertiesStudioModal — thin `.properties` wrapper around `<Studio>`.

  Owns only the `.properties`-specific bits: the `$value` self-marker
  (a key that is both a leaf and a prefix), every-key-is-a-ref cross-refs
  (no id/name heuristic), no native typing on the wire (schema-aware
  narrowing coerces back to the typed primitive but the on-disk value is
  the string form), the dotted-key parse-warning banner + Errors copy,
  and the `null_handling = ask_user` bulk policy. Everything else is the
  generic `<Studio>`.

  FROZEN F4/F5: every leaf is a string; the `null` kind exists only for
  bulk-edit parity. Every flat dotted key is a cross-ref target.
-->
<script lang="ts">
  import { tooltip } from '$lib/actions/tooltip';
  import Alert from '../shared/ui/Alert.svelte';
  import Studio from './Studio.svelte';
  import type { StudioConfig, StudioTreeNode, StudioCtx, StudioCommitResult } from './studio-config';
  import type { StudioKindTone } from './StudioKindBadge.svelte';
  import { propertiesStudioStore, type PropertiesNodeKind } from '$lib/stores/studio/properties-studio.svelte';
  import { studioBackend, type StudioPrimitiveValue } from '$lib/ipc/studio/studio-format';
  import { typeAtPath as walkTypeAtPath, flattenedStructFields } from '$lib/utils/studio-schema';

  const PROPS_BE = studioBackend<PropertiesNodeKind>('properties');
  type TNode = StudioTreeNode<PropertiesNodeKind>;

  function kindBadge(k: PropertiesNodeKind): string {
    switch (k) {
      case 'object':  return '{}';
      case 'array':   return '[]';
      case 'string':  return '“';
      case 'null':    return '∅';
    }
  }
  function kindTone(k: PropertiesNodeKind): StudioKindTone {
    switch (k) {
      case 'object':
      case 'array':   return 'type';
      case 'string':  return 'string';
      case 'null':    return 'muted';
    }
  }
  function isContainerKind(k: PropertiesNodeKind): boolean { return k === 'object' || k === 'array'; }
  function isEditablePrimitive(k: PropertiesNodeKind): boolean { return k === 'string'; }
  function isPromotableNull(k: PropertiesNodeKind): boolean { return k === 'null'; }

  async function commit(node: TNode, draft: string, ctx: StudioCtx<PropertiesNodeKind, TNode>): Promise<StudioCommitResult> {
    // Schema-aware narrowing — `.properties` has no native typing; pass
    // the typed primitive so the tree projection updates, the on-disk
    // value stays the string form.
    const hint = ctx.schema() ? ctx.primitiveHintAt(node.path) : null;
    const wantFloat = hint === 'f32' || hint === 'f64' || hint === 'number';
    const wantInt   = hint === 'integer'
      || (hint != null && (hint.startsWith('i') || hint.startsWith('u'))
          && hint !== 'isize' && hint !== 'usize')
      || hint === 'isize' || hint === 'usize';
    const wantBool  = hint === 'bool' || hint === 'boolean';

    let value: StudioPrimitiveValue;
    try {
      if (wantBool) {
        const t = draft.trim().toLowerCase();
        if (t !== 'true' && t !== 'false') throw new Error('schema: expected boolean');
        value = { type: 'bool', value: t === 'true' };
      } else if (wantInt) {
        const n = Number(draft.trim());
        if (!Number.isFinite(n) || !Number.isInteger(n)) throw new Error('schema: expected integer');
        value = { type: 'int', value: Math.trunc(n) };
      } else if (wantFloat) {
        const n = Number(draft.trim());
        if (!Number.isFinite(n)) throw new Error('schema: expected number');
        value = { type: 'float', value: n };
      } else {
        value = { type: 'string', value: draft };
      }
    } catch (e: any) {
      return { error: e?.message ?? String(e) };
    }
    try {
      await propertiesStudioStore.mutatePrimitive(node.path, value);
      await ctx.refresh(node, /* structural */ false);
      return {};
    } catch (e: any) { return { error: e?.message ?? String(e) }; }
  }

  function copyPathText(path: string[]): string {
    const segs = path.filter(s => s !== '$value');
    return segs.length === 0 ? '$' : '$.' + segs.join('.');
  }

  const config: StudioConfig<PropertiesNodeKind, TNode> = {
    formatId: 'properties',
    backend: PROPS_BE,
    formatLabel: '.properties',
    ariaLabel: '.properties Studio',
    defaultTitle: 'Properties Studio',
    loadingLabel: 'Opening .properties document…',
    rightPaneKey: 'arbor:properties-studio:right-pane',
    queryHistoryKey: 'arbor:properties-studio:query-history',
    queryPlaceholder: 'Query — server.port, $..host, $.servers[0], …',
    saveExtensions: ['properties'],
    saveDefaultName: 'application.properties',
    separator: '=',
    nullPolicy: 'ask_user',
    indentTooltip: 'Indent — informational; .properties has no nested indentation',
    formatTooltip: 'Format — no-op for .properties (every byte already preserved)',
    schemaPickerTitle: 'Pick JSON Schema file',
    schemaPickerButton: 'Pick schema file',
    schemaPickerExts: ['json', 'schema.json'],
    schemaRailTooltipEmpty: 'Schema — bind a JSON Schema file',
    schemaRailLabel: 'Schema',
    schemaCssPrefix: 'ps',
    copyValueLabel: 'Copy value',
    pasteLabel: 'Paste over value…',
    kindBadgeStyle: 'italic-null',

    store: propertiesStudioStore,
    closeDoc: () => propertiesStudioStore.closeDoc(),
    openDoc: (opts) => propertiesStudioStore.openDoc(opts),
    undo: () => propertiesStudioStore.undo(),
    redo: () => propertiesStudioStore.redo(),
    setText: (t) => propertiesStudioStore.setText(t),
    save: (opts) => propertiesStudioStore.save(opts),
    applyExternalMutate: (state) => propertiesStudioStore.applyExternalMutate(state),

    mutatePrimitive: (p, v) => propertiesStudioStore.mutatePrimitive(p, v),
    removeAt: (p) => propertiesStudioStore.removeAt(p),
    insertField: (p, k, s) => propertiesStudioStore.insertField(p, k, s),
    insertItem: (p, s) => propertiesStudioStore.insertItem(p, s),
    duplicateAt: (p) => propertiesStudioStore.duplicateAt(p),
    moveItem: (p, d) => propertiesStudioStore.moveItem(p, d),
    replaceAt: (p, s) => propertiesStudioStore.replaceAt(p, s),
    newFieldSnippet: () => '',
    newItemSnippet: () => '',

    kindBadge,
    kindTone,
    isBoolKind: () => false,
    isContainerKind,
    isEditablePrimitive,
    isPromotableNull,

    sortChildren: (_k, kids) => kids,

    computeSeed: (n, valueText) => {
      let seed = valueText ?? n.preview;
      if (n.kind === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
        try { seed = JSON.parse(seed) as string; }
        catch { seed = seed.slice(1, -1); }
      }
      if (n.kind === 'null') seed = '';
      return seed;
    },
    commit,

    getSchemaHint: () => propertiesStudioStore.schemaHint,
    walkType: walkTypeAtPath,
    flattenedFields: flattenedStructFields,

    // Every flat key is a cross-ref target; every value a potential ref;
    // `.properties` has no quotes so `unquotedString` is identity.
    unquotedString:        (preview) => preview || null,
    isDefinitionFieldName: () => true,
    isReferenceFieldName:  () => true,

    currentVariantTag: (n) => (n.kind === 'string' ? (n.preview ?? '') : ''),
    extractRenameValue: (n) => n.preview || null,
    isDefinitionNode: (n) => n.kind === 'string' && n.preview.length > 0,
    definitionValue: (n) => (n.kind === 'string' && n.preview ? n.preview : null),

    copyPathText,
  };
</script>

<Studio {config}>
  {#snippet headerIcon()}
    <svg viewBox="0 0 24 24" width="18" height="18" xmlns="http://www.w3.org/2000/svg">
      <rect x="3" y="3" width="18" height="18" rx="2" fill="currentColor" opacity="0.18" />
      <path d="M6 9h4M11 9h7M6 13h3M10 13h8M6 17h5M12 17h6"
            stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
    </svg>
  {/snippet}

  {#snippet rowKeyOverride({ node }: { node: TNode })}
    {#if node.key === '$value'}
      <span class="ps-row-key ps-row-key-self"
            use:tooltip={'Value at the parent prefix — `.properties` allows a key to be both a leaf and a sub-key prefix.'}>(self)</span>
    {:else}
      <span class="ps-row-key" class:ps-row-key-index={/^\d+$/.test(node.key)}>{node.key}</span>
    {/if}
  {/snippet}

  {#snippet footerPathSlot({ path }: { path: string[] })}
    {@const segs = path.filter(s => s !== '$value')}
    {@const isSelf = path.includes('$value')}
    <span class="ps-footer-path-pill" use:tooltip={isSelf
      ? `Selected node path — points at the value carried by the prefix \`${segs.join('.')}\` itself (next to its sub-keys).`
      : 'Selected node path'}>
      {segs.length === 0 ? '$' : '$.' + segs.join('.')}{#if isSelf}<span class="ps-footer-path-self"> · self</span>{/if}
    </span>
  {/snippet}

  {#snippet bannersExtras()}
    {#if propertiesStudioStore.parseError}
      <div class="ps-banner-wrap"><Alert variant="warning" compact text={propertiesStudioStore.parseError} /></div>
    {/if}
  {/snippet}

  {#snippet errorsBody({ parseError }: { parseError: string })}
    <div class="ps-errors-wrap">
      <Alert variant="warning" title="Parse warning">
        <pre class="ps-errors-body">{parseError}</pre>
        <p class="ps-errors-hint">
          Dotted-key conflicts happen when the same prefix is used as
          both a leaf and a container (e.g. <code>foo=string</code> and
          <code>foo.sub=value</code>). The tree falls back to a flat
          view so every key stays editable. Resolve by renaming one of
          the colliding keys.
        </p>
      </Alert>
    </div>
  {/snippet}

  {#snippet bindingsEmpty()}
    <p class="ps-bindings-empty">
      Every flat dotted key is a cross-ref target; every value is a
      potential reference. The sidecar lists every key whose value
      matches another file's key.
    </p>
  {/snippet}

  {#snippet schemaIntro()}
    <p class="ps-schema-hint">
      Pick a JSON Schema file (<code>*.schema.json</code> or
      <code>*.json</code> with a <code>$schema</code> keyword) to
      decorate this <code>.properties</code> document. Properties
      Studio surfaces every <code>$defs</code> entry as a root
      candidate.
    </p>
  {/snippet}
</Studio>

<style>
  .ps-row-key { color: var(--text-primary); font-family: var(--font-code); font-size: var(--font-size-xs); white-space: nowrap; }
  .ps-row-key-index { color: var(--text-muted); font-style: italic; }
  .ps-row-key-self { color: var(--accent); font-style: italic; font-size: var(--font-size-2xs); opacity: 0.85; }
  .ps-footer-path-pill {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); background: var(--bg-overlay);
    padding: 2px 6px; border-radius: 999px; max-width: 280px; overflow: hidden;
    text-overflow: ellipsis; white-space: nowrap;
  }
  .ps-footer-path-self { color: var(--accent); font-style: italic; margin-left: 1px; }
  .ps-banner-wrap { padding: 6px 12px 0 12px; }
  .ps-errors-wrap { padding: 16px; height: 100%; overflow: auto; }
  .ps-errors-body {
    background: var(--bg-overlay); color: var(--text-primary); padding: 10px; border-radius: 4px;
    font-family: var(--font-code); font-size: var(--font-size-xs); margin: 6px 0 0; overflow: auto; white-space: pre-wrap;
  }
  .ps-errors-hint { color: var(--text-muted); font-size: var(--font-size-xs); margin: 6px 0 0; line-height: 1.5; }
  .ps-errors-hint code, .ps-bindings-empty code, .ps-schema-hint code {
    font-family: var(--font-code); font-size: var(--font-size-xs); padding: 1px 4px; border-radius: 3px;
    background: var(--bg-overlay); color: var(--text-primary);
  }
  .ps-bindings-empty { color: var(--text-muted); font-size: var(--font-size-xs); padding: 12px; margin: 0; line-height: 1.5; }
  .ps-schema-hint { color: var(--text-secondary); font-size: var(--font-size-xs); line-height: 1.5; margin: 0; }
</style>
