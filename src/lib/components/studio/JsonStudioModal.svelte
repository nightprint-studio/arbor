<!--
  JsonStudioModal — thin JSON wrapper around the generic `<Studio>`.

  Owns only the JSON-specific bits: the JSONC banner + stream-mode banner
  + 4-way JSONC save prompt, schema-aware primitive narrowing in
  `commit`, stream-mode editing gate, the `tinted` kind-badge style, and
  the JSON Schema sidecar copy. Everything else (tree, query, diff,
  inspector, cross-refs, rename + bulk, save flow, undo/redo, keys) lives
  in `<Studio>` + the shared composables.
-->
<script lang="ts">
  import { FileJson } from 'lucide-svelte';
  import Modal from '../shared/Modal.svelte';
  import Alert from '../shared/ui/Alert.svelte';
  import Studio from './Studio.svelte';
  import { INDENT_OPTIONS_WITH_8 } from './studio-footer-types';
  import { fmtBytes as fsFmtBytes } from './helpers';
  import type { StudioConfig, StudioTreeNode, StudioCtx, StudioCommitResult } from './studio-config';
  import type { StudioKindTone } from './StudioKindBadge.svelte';
  import { jsonStudioStore, type JsonNodeKind } from '$lib/stores/studio/json-studio.svelte';
  import { studioBackend, type StudioPrimitiveValue } from '$lib/ipc/studio/studio-format';
  import { typeAtPath as walkTypeAtPath, flattenedStructFields } from '$lib/utils/studio-schema';

  const JSON_BE = studioBackend<JsonNodeKind>('json');
  type TNode = StudioTreeNode<JsonNodeKind>;

  let studio: Studio<JsonNodeKind, TNode> | undefined = $state();

  function kindBadge(k: JsonNodeKind): string {
    switch (k) {
      case 'object': return '{}';
      case 'array':  return '[]';
      case 'string': return '“';
      case 'number': return '#';
      case 'bool':   return '✓';
      case 'null':   return '∅';
    }
  }
  function kindTone(k: JsonNodeKind): StudioKindTone {
    switch (k) {
      case 'object':
      case 'array':  return 'type';
      case 'string': return 'string';
      case 'number': return 'number';
      case 'bool':   return 'keyword';
      case 'null':   return 'muted';
    }
  }
  function isContainerKind(k: JsonNodeKind): boolean { return k === 'object' || k === 'array'; }
  function isEditablePrimitive(k: JsonNodeKind): boolean {
    return k === 'string' || k === 'number' || k === 'bool';
  }

  async function commit(node: TNode, draft: string, ctx: StudioCtx<JsonNodeKind, TNode>): Promise<StudioCommitResult> {
    const hint = ctx.schema() ? ctx.primitiveHintAt(node.path) : null;
    const wantInt    = hint === 'integer';
    const wantNum    = hint === 'number';
    const wantBool   = hint === 'boolean';
    const wantString = hint === 'string';

    let value: StudioPrimitiveValue;
    try {
      switch (node.kind) {
        case 'string':
          if (wantBool) {
            const t = draft.trim().toLowerCase();
            if (t !== 'true' && t !== 'false') throw new Error('schema: expected boolean');
            value = { type: 'bool', value: t === 'true' };
            break;
          }
          if (wantInt) {
            const n = Number(draft.trim());
            if (!Number.isFinite(n) || !Number.isInteger(n)) throw new Error('schema: expected integer');
            value = { type: 'int', value: Math.trunc(n) };
            break;
          }
          if (wantNum) {
            const n = Number(draft.trim());
            if (!Number.isFinite(n)) throw new Error('schema: expected number');
            value = { type: 'float', value: n };
            break;
          }
          value = { type: 'string', value: draft };
          break;
        case 'bool': {
          const t = draft.trim().toLowerCase();
          if (t !== 'true' && t !== 'false') throw new Error('expected "true" or "false"');
          value = { type: 'bool', value: t === 'true' };
          break;
        }
        case 'number': {
          const s = draft.trim();
          const n = Number(s);
          if (!Number.isFinite(n)) throw new Error('not a number');
          if (wantInt) {
            if (!Number.isInteger(n)) throw new Error('schema: expected integer');
            value = { type: 'int', value: Math.trunc(n) };
            break;
          }
          if (wantString) { value = { type: 'string', value: draft }; break; }
          const looksFloat = /[.eE]/.test(s);
          value = looksFloat || wantNum ? { type: 'float', value: n } : { type: 'int', value: Math.trunc(n) };
          break;
        }
        default: return {};
      }
    } catch (e: any) {
      return { error: e?.message ?? String(e) };
    }
    try {
      await jsonStudioStore.mutatePrimitive(node.path, value);
      await ctx.refresh(node, /* structural */ false);
      return {};
    } catch (e: any) { return { error: e?.message ?? String(e) }; }
  }

  // ── JSONC save gate ─────────────────────────────────────────────────
  let jsoncSavePromptOpen = $state(false);

  async function onSaveRequested(): Promise<void> {
    if (!jsonStudioStore.sourcePath) { studio?.openSaveAs(); return; }
    if (jsonStudioStore.hasJsoncFeatures && !jsonStudioStore.isJsonc) {
      jsoncSavePromptOpen = true;
      return;
    }
    await studio?.doSaveShared();
  }
  async function onSaveAsJsonc(): Promise<void> {
    jsoncSavePromptOpen = false;
    const next = jsonStudioStore.renameSourceToJsonc();
    if (!next) { studio?.openSaveAs(); return; }
    await studio?.onSaveAsPicked(next);
  }
  async function onStripAndSave(): Promise<void> {
    jsoncSavePromptOpen = false;
    const ok = await jsonStudioStore.stripJsoncFeatures();
    if (!ok) return;
    await studio?.doSaveShared();
  }
  async function onSaveAnyway(): Promise<void> {
    jsoncSavePromptOpen = false;
    await studio?.doSaveShared();
  }

  const config: StudioConfig<JsonNodeKind, TNode> = {
    formatId: 'json',
    backend: JSON_BE,
    formatLabel: 'JSON',
    ariaLabel: 'JSON Studio',
    defaultTitle: 'JSON Studio',
    loadingLabel: 'Parsing JSON…',
    rightPaneKey: 'arbor:json-studio:right-pane',
    queryHistoryKey: 'arbor:json-studio:query-history',
    queryPlaceholder: 'Query — name (recursive), $.foo.bar, $.arr[0:5], $.users[?@.age > 30]…',
    saveExtensions: ['json', 'jsonc'],
    saveDefaultName: jsonStudioStore.isJsonc ? 'document.jsonc' : 'document.json',
    separator: ':',
    indentOptions: INDENT_OPTIONS_WITH_8,
    nullPolicy: 'native',
    indentTooltip: 'Indent — applied to Format and tree edits',
    formatTooltip: 'Format — re-emit canonical JSON (loses any non-canonical whitespace)',
    schemaPickerTitle: 'Pick JSON Schema file',
    schemaPickerButton: 'Pick .schema.json',
    schemaPickerExts: ['json', 'schema.json'],
    schemaRailTooltipEmpty: 'Schema — bind a JSON Schema file',
    schemaRailLabel: 'JSON Schema',
    schemaCssPrefix: 'js',
    copyValueLabel: 'Copy value (JSON)',
    pasteLabel: 'Paste JSON over value…',
    kindBadgeStyle: 'tinted',

    store: jsonStudioStore,
    closeDoc: () => jsonStudioStore.closeDoc(),
    openDoc: (opts) => jsonStudioStore.openDoc(opts),
    undo: () => jsonStudioStore.undo(),
    redo: () => jsonStudioStore.redo(),
    setText: (t) => jsonStudioStore.setText(t),
    save: (opts) => jsonStudioStore.save(opts),
    applyExternalMutate: (state) => jsonStudioStore.applyExternalMutate(state),

    mutatePrimitive: (p, v) => jsonStudioStore.mutatePrimitive(p, v),
    removeAt: (p) => jsonStudioStore.removeAt(p),
    insertField: (p, k, s) => jsonStudioStore.insertField(p, k, s),
    insertItem: (p, s) => jsonStudioStore.insertItem(p, s),
    duplicateAt: (p) => jsonStudioStore.duplicateAt(p),
    moveItem: (p, d) => jsonStudioStore.moveItem(p, d),
    replaceAt: (p, s) => jsonStudioStore.replaceAt(p, s),
    newFieldSnippet: () => 'null',
    newItemSnippet: () => 'null',

    kindBadge,
    kindTone,
    isBoolKind: (k) => k === 'bool',
    isContainerKind,
    isEditablePrimitive,

    sortChildren: (_k, kids) => kids,

    computeSeed: (n, valueText) => {
      let seed = valueText ?? n.preview;
      // Strip surrounding quotes for the input; re-added via the `string`
      // primitive serialisation. JSON escapes survive the round-trip.
      if (n.kind === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
        try { seed = JSON.parse(seed) as string; }
        catch { seed = seed.slice(1, -1); }
      }
      return seed;
    },
    commit,

    getSchemaHint: () => jsonStudioStore.schemaHint,
    walkType: walkTypeAtPath,
    flattenedFields: flattenedStructFields,

    currentVariantTag: (n, ctx) => {
      if (n.kind !== 'string') return '';
      return ctx.unquotedString(n.preview) ?? '';
    },
    extractRenameValue: (n, ctx) => ctx.unquotedString(n.preview),
    isDefinitionNode: (n, ctx) =>
      n.kind === 'string' && ctx.isDefinitionFieldName(n.key) && !!ctx.unquotedString(n.preview),
    definitionValue: (n, ctx) => {
      if (n.kind !== 'string' || !ctx.isDefinitionFieldName(n.key)) return null;
      return ctx.unquotedString(n.preview);
    },

    editingEnabled: () => !jsonStudioStore.streamMode,
    onSaveRequested,
  };
</script>

<Studio bind:this={studio} {config}>
  {#snippet headerIcon()}<FileJson size={14} />{/snippet}

  {#snippet bannersExtras()}
    {#if jsonStudioStore.streamMode && !jsonStudioStore.streamBannerDismissed}
      <div class="js-banner-wrap">
        <div class="js-jsonc-banner js-jsonc-banner-info">
          <div class="js-jsonc-banner-text">
            <strong>Large file ({fsFmtBytes(jsonStudioStore.sizeBytes ?? 0)}):</strong>
            opened in streaming mode. Comments and trailing commas are not
            supported, and structural tree edits are disabled — use the
            Text pane for raw edits.
          </div>
          <div class="js-jsonc-banner-actions">
            <button type="button" class="js-jsonc-btn js-jsonc-btn-ghost"
              onclick={() => jsonStudioStore.dismissStreamBanner()}>Dismiss</button>
          </div>
        </div>
      </div>
    {/if}
    {#if jsonStudioStore.hasJsoncFeatures
        && !jsonStudioStore.isJsonc
        && !jsonStudioStore.streamMode
        && !jsonStudioStore.bannerDismissed}
      <div class="js-banner-wrap">
        <div class="js-jsonc-banner js-jsonc-banner-warn">
          <div class="js-jsonc-banner-text">
            <strong>This .json file uses JSONC features</strong>
            (comments / trailing commas). Strict JSON parsers will fail
            to read it.
          </div>
          <div class="js-jsonc-banner-actions">
            <button type="button" class="js-jsonc-btn" onclick={() => void onSaveAsJsonc()}>Rename to .jsonc</button>
            <button type="button" class="js-jsonc-btn" onclick={() => void onStripAndSave()}>Strip & save</button>
            <button type="button" class="js-jsonc-btn js-jsonc-btn-ghost"
              onclick={() => jsonStudioStore.dismissJsoncBanner()}>Dismiss</button>
          </div>
        </div>
      </div>
    {/if}
  {/snippet}

  {#snippet errorsBody({ parseError }: { parseError: string })}
    <div class="js-errors-wrap">
      <Alert variant="error" title="JSON parse error">
        <pre class="js-errors-body">{parseError}</pre>
        <p class="js-errors-hint">
          Switch to the <strong>Text</strong> tab to fix it. The error will
          clear automatically once the document parses.
        </p>
      </Alert>
    </div>
  {/snippet}

  {#snippet bindingsEmpty()}
    <p class="js-bindings-empty">
      Project-wide cross-refs follow the <code>id</code> / <code>name</code>
      convention by default. Open the Schema panel to bind a JSON
      Schema sidecar — schema-derived bindings will appear here as
      they're configured.
    </p>
  {/snippet}

  {#snippet schemaIntro()}
    <p class="js-schema-hint">
      Pick a JSON Schema file (<code>*.schema.json</code> or any JSON
      document with a <code>$schema</code> keyword). JSON Studio will
      surface every <code>$defs</code> / <code>definitions</code>
      entry as a root candidate and walk every <code>$ref</code>
      chain to index the reachable types.
    </p>
  {/snippet}

  {#snippet auxiliaryExtras()}
    {#if jsoncSavePromptOpen}
      <Modal
        onClose={() => jsoncSavePromptOpen = false}
        width="min(520px, 92vw)"
        height="auto"
        padBody={true}
        ariaLabel="Save .json with comments"
      >
        {#snippet header()}
          <h3 style="margin: 0; font-size: var(--font-size-md);">Save .json with JSONC features</h3>
        {/snippet}
        <div style="display: flex; flex-direction: column; gap: 10px; font-size: var(--font-size-sm); line-height: 1.5; color: var(--text-primary);">
          <p style="margin: 0;">
            This file uses <strong>comments</strong> or <strong>trailing
            commas</strong>. Strict JSON parsers (most build tools,
            <code>json.loads</code>, <code>JSON.parse</code>) will fail to read it.
          </p>
          <p style="margin: 0; color: var(--text-secondary);">Pick how to save:</p>
          <div style="display: flex; flex-direction: column; gap: 6px;">
            <button type="button" class="js-jsonc-btn" style="text-align: left;" onclick={() => void onSaveAsJsonc()}>
              <strong>Save as .jsonc</strong>
              <span style="display:block;color:var(--text-secondary);font-size:var(--font-size-xs);">Rename the file to <code>.jsonc</code> and keep all JSONC features intact.</span>
            </button>
            <button type="button" class="js-jsonc-btn" style="text-align: left;" onclick={() => void onStripAndSave()}>
              <strong>Strip & save</strong>
              <span style="display:block;color:var(--text-secondary);font-size:var(--font-size-xs);">Lose comments and trailing commas — pure JSON. Reversible via undo.</span>
            </button>
            <button type="button" class="js-jsonc-btn" style="text-align: left;" onclick={() => void onSaveAnyway()}>
              <strong>Save anyway</strong>
              <span style="display:block;color:var(--text-secondary);font-size:var(--font-size-xs);">Keep the <code>.json</code> path and the JSONC features. Strict parsers will break.</span>
            </button>
          </div>
          <div style="display: flex; justify-content: flex-end; padding-top: 4px;">
            <button type="button" class="js-jsonc-btn js-jsonc-btn-ghost" onclick={() => jsoncSavePromptOpen = false}>Cancel</button>
          </div>
        </div>
      </Modal>
    {/if}
  {/snippet}
</Studio>

<style>
  /* Banners + JSONC prompt buttons — JSON-specific styling. */
  .js-banner-wrap { padding: 6px 8px 0 8px; }
  .js-jsonc-banner {
    display: flex; align-items: center; gap: 12px;
    padding: 8px 10px; border-radius: 6px; font-size: var(--font-size-xs); line-height: 1.45;
  }
  .js-jsonc-banner-info { background: color-mix(in srgb, var(--info, #4a9eff) 14%, transparent); color: var(--text-primary); }
  .js-jsonc-banner-warn { background: color-mix(in srgb, var(--warning, #e5a050) 16%, transparent); color: var(--text-primary); }
  .js-jsonc-banner-text { flex: 1; min-width: 0; }
  .js-jsonc-banner-actions { display: flex; gap: 6px; flex-shrink: 0; }
  .js-jsonc-btn {
    padding: 4px 10px; border-radius: 4px; border: 1px solid var(--border-subtle);
    background: var(--bg-overlay); color: var(--text-primary); font-size: var(--font-size-xs); cursor: pointer;
  }
  .js-jsonc-btn:hover { background: var(--bg-hover); }
  .js-jsonc-btn-ghost { background: transparent; }

  .js-errors-wrap { padding: 16px; height: 100%; overflow: auto; }
  .js-errors-body {
    background: var(--bg-overlay); color: var(--text-primary); padding: 10px; border-radius: 4px;
    font-family: var(--font-code); font-size: var(--font-size-xs); margin: 6px 0 0; overflow: auto; white-space: pre-wrap;
  }
  .js-errors-hint { color: var(--text-muted); font-size: var(--font-size-xs); margin: 6px 0 0; }
  .js-bindings-empty { color: var(--text-muted); font-size: var(--font-size-xs); padding: 12px; margin: 0; line-height: 1.5; }
  .js-schema-hint { color: var(--text-secondary); font-size: var(--font-size-xs); line-height: 1.5; margin: 0; }
  .js-schema-hint code, .js-bindings-empty code {
    font-family: var(--font-code); font-size: var(--font-size-xs); padding: 1px 4px; border-radius: 3px;
    background: var(--bg-overlay); color: var(--text-primary);
  }
</style>
