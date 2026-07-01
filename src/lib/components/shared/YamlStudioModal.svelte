<!--
  YamlStudioModal — thin YAML wrapper around the generic `<Studio>`.

  Owns only the YAML-specific bits: schema-aware numeric narrowing +
  null-promotion in `commit`, first-class null (`null` leaf routes
  through `replace_at`), the YAML↔.properties converter (Tools sidecar +
  preview modal), and the JSON Schema sidecar copy. Everything else is
  the generic `<Studio>`.
-->
<script lang="ts">
  import { ArrowLeftRight } from 'lucide-svelte';
  import Icon from '@iconify/svelte';
  import yamlIcon from '@iconify-icons/vscode-icons/file-type-yaml';
  import Dropdown from './ui/Dropdown.svelte';
  import Alert from './ui/Alert.svelte';
  import FileExplorerModal from './FileExplorerModal.svelte';
  import StudioConvertPreviewModal from './studio/StudioConvertPreviewModal.svelte';
  import Studio from './studio/Studio.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { basename as fsBasename } from './studio/helpers';
  import type { StudioConfig, StudioTreeNode, StudioCtx, StudioCommitResult } from './studio/studio-config';
  import type { StudioKindTone } from './studio/StudioKindBadge.svelte';
  import { yamlStudioStore, type YamlNodeKind } from '$lib/stores/yaml-studio.svelte';
  import { studioBackend, type StudioPrimitiveValue } from '$lib/ipc/studio-format';
  import { fsReadTextFile } from '$lib/ipc/fs';
  import { typeAtPath as walkTypeAtPath, flattenedStructFields } from '$lib/utils/studio-schema';

  const YAML_BE = studioBackend<YamlNodeKind>('yaml');
  type TNode = StudioTreeNode<YamlNodeKind>;

  let studio: Studio<YamlNodeKind, TNode> | undefined = $state();

  function kindBadge(k: YamlNodeKind): string {
    switch (k) {
      case 'object':  return '{}';
      case 'array':   return '[]';
      case 'string':  return '“';
      case 'integer': return '#';
      case 'float':   return '⊘';
      case 'bool':    return '✓';
      case 'null':    return '∅';
    }
  }
  function kindTone(k: YamlNodeKind): StudioKindTone {
    switch (k) {
      case 'object':
      case 'array':   return 'type';
      case 'string':  return 'string';
      case 'integer':
      case 'float':   return 'number';
      case 'bool':    return 'keyword';
      case 'null':    return 'muted';
    }
  }
  function isContainerKind(k: YamlNodeKind): boolean { return k === 'object' || k === 'array'; }
  function isEditablePrimitive(k: YamlNodeKind): boolean {
    return k === 'string' || k === 'integer' || k === 'float' || k === 'bool';
  }
  function isPromotableNull(k: YamlNodeKind): boolean { return k === 'null'; }

  async function commit(node: TNode, draft: string, ctx: StudioCtx<YamlNodeKind, TNode>): Promise<StudioCommitResult> {
    if (node.kind === 'null') {
      const snippet = draft.trim().length === 0 ? 'null' : draft;
      try {
        await yamlStudioStore.replaceAt(node.path, snippet);
        await ctx.refresh(node, /* structural */ true);
        return {};
      } catch (e: any) { return { error: e?.message ?? String(e) }; }
    }

    const hint = ctx.schema() ? ctx.primitiveHintAt(node.path) : null;
    const wantFloat = hint === 'f32' || hint === 'f64' || hint === 'number';
    const wantInt   = hint === 'integer' || (hint != null &&
      (hint.startsWith('i') || hint.startsWith('u')) && hint !== 'isize' && hint !== 'usize') ||
      hint === 'isize' || hint === 'usize';
    const wantBool   = hint === 'bool' || hint === 'boolean';
    const wantString = hint === 'string' || hint === 'String' || hint === '&str' || hint === 'str';

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
          if (wantFloat) {
            const n = Number(draft.trim());
            if (!Number.isFinite(n)) throw new Error('schema: expected number');
            value = { type: 'float', value: n };
            break;
          }
          value = { type: 'string', value: draft };
          break;
        case 'bool': {
          const t = draft.trim().toLowerCase();
          if (t === 'true' || t === 'yes' || t === 'on')  { value = { type: 'bool', value: true };  break; }
          if (t === 'false' || t === 'no' || t === 'off') { value = { type: 'bool', value: false }; break; }
          throw new Error('expected "true" or "false"');
        }
        case 'integer': {
          const n = Number(draft.trim());
          if (!Number.isFinite(n)) throw new Error('not an integer');
          if (wantFloat)  { value = { type: 'float',  value: n      }; break; }
          if (wantString) { value = { type: 'string', value: draft  }; break; }
          if (!Number.isInteger(n)) throw new Error('not an integer');
          value = { type: 'int', value: Math.trunc(n) };
          break;
        }
        case 'float': {
          const n = Number(draft.trim());
          if (!Number.isFinite(n)) throw new Error('not a number');
          if (wantInt) {
            if (!Number.isInteger(n)) throw new Error('schema: expected integer');
            value = { type: 'int', value: Math.trunc(n) };
            break;
          }
          if (wantString) { value = { type: 'string', value: draft }; break; }
          value = { type: 'float', value: n };
          break;
        }
        default: return {};
      }
    } catch (e: any) {
      return { error: e?.message ?? String(e) };
    }
    try {
      await yamlStudioStore.mutatePrimitive(node.path, value);
      await ctx.refresh(node, /* structural */ false);
      return {};
    } catch (e: any) { return { error: e?.message ?? String(e) }; }
  }

  // ── YAML ↔ .properties converter ────────────────────────────────────
  type ConvertMode = 'yaml-to-properties' | 'properties-to-yaml';
  let convertOpen   = $state(false);
  let convertMode   = $state<ConvertMode>('yaml-to-properties');
  let convertSource = $state<string>('');
  let importPickerOpen = $state(false);
  let convertActionError = $state<string | null>(null);

  function ysBasenameNoExt(p: string | null | undefined): string {
    const base = fsBasename(p);
    const dot = base.lastIndexOf('.');
    return dot > 0 ? base.slice(0, dot) : base;
  }
  function openConvertToProperties() {
    convertMode = 'yaml-to-properties';
    convertSource = yamlStudioStore.current;
    convertOpen = true;
  }
  function openImportProperties() { importPickerOpen = true; }
  async function onImportPicked(p: string) {
    importPickerOpen = false;
    try {
      const text = await fsReadTextFile(p);
      convertMode = 'properties-to-yaml';
      convertSource = text;
      convertOpen = true;
    } catch (e: any) { convertActionError = `Read .properties failed: ${e?.message ?? e}`; }
  }
  function closeConvert() { convertOpen = false; }
  function convertReplaceHandler() {
    if (convertMode === 'properties-to-yaml') {
      return async (text: string) => {
        await yamlStudioStore.setText(text);
        await studio?.reloadAfterExternalSetText();
      };
    }
    return null;
  }

  const config: StudioConfig<YamlNodeKind, TNode> = {
    formatId: 'yaml',
    backend: YAML_BE,
    formatLabel: 'YAML',
    ariaLabel: 'YAML Studio',
    defaultTitle: 'YAML Studio',
    loadingLabel: 'Opening YAML document…',
    rightPaneKey: 'arbor:yaml-studio:right-pane',
    queryHistoryKey: 'arbor:yaml-studio:query-history',
    queryPlaceholder: 'Query — name (recursive), $.servers[0], $..port, …',
    saveExtensions: ['yaml', 'yml'],
    saveDefaultName: 'document.yaml',
    separator: ':',
    nullPolicy: 'native',
    indentTooltip: 'Indent — informational; yaml_edit preserves the per-doc style on edit',
    formatTooltip: 'Format — re-emit the YAML through yaml_edit (preserves comments)',
    schemaPickerTitle: 'Pick JSON Schema file',
    schemaPickerButton: 'Pick schema file',
    schemaPickerExts: ['json', 'schema.json'],
    schemaRailTooltipEmpty: 'Schema — bind a JSON Schema file',
    schemaRailLabel: 'Schema',
    schemaCssPrefix: 'ys',
    copyValueLabel: 'Copy value (YAML)',
    pasteLabel: 'Paste YAML over value…',
    kindBadgeStyle: 'italic-null',

    store: yamlStudioStore,
    closeDoc: () => yamlStudioStore.closeDoc(),
    openDoc: (opts) => yamlStudioStore.openDoc(opts),
    undo: () => yamlStudioStore.undo(),
    redo: () => yamlStudioStore.redo(),
    setText: (t) => yamlStudioStore.setText(t),
    save: (opts) => yamlStudioStore.save(opts),
    applyExternalMutate: (state) => yamlStudioStore.applyExternalMutate(state),

    mutatePrimitive: (p, v) => yamlStudioStore.mutatePrimitive(p, v),
    removeAt: (p) => yamlStudioStore.removeAt(p),
    insertField: (p, k, s) => yamlStudioStore.insertField(p, k, s),
    insertItem: (p, s) => yamlStudioStore.insertItem(p, s),
    duplicateAt: (p) => yamlStudioStore.duplicateAt(p),
    moveItem: (p, d) => yamlStudioStore.moveItem(p, d),
    replaceAt: (p, s) => yamlStudioStore.replaceAt(p, s),
    newFieldSnippet: () => 'null',
    newItemSnippet: () => 'null',

    kindBadge,
    kindTone,
    isBoolKind: (k) => k === 'bool',
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

    getSchemaHint: () => yamlStudioStore.schemaHint,
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
  };
</script>

<Studio bind:this={studio} {config}>
  {#snippet headerIcon()}<Icon icon={yamlIcon} width={18} height={18} />{/snippet}

  {#snippet toolsExtras()}
    <div class="sts-row">
      <div class="sts-row-label">Convert</div>
      <Dropdown items={[
        { kind: 'item', id: 'yaml-to-properties', label: 'Convert → .properties…',
          onclick: openConvertToProperties,
          disabled: !!yamlStudioStore.parseError || !yamlStudioStore.docId },
        { kind: 'item', id: 'properties-to-yaml', label: 'Import .properties → YAML…',
          onclick: openImportProperties },
      ]} position="fixed" direction="down">
        {#snippet trigger({ toggle })}
          <button type="button" class="sts-btn" onclick={toggle} use:tooltip={'YAML ↔ .properties bridge'}>
            <ArrowLeftRight size={13} />
            <span>YAML ↔ .properties</span>
          </button>
        {/snippet}
      </Dropdown>
    </div>
  {/snippet}

  {#snippet errorsBody({ parseError }: { parseError: string })}
    <div class="ys-errors-wrap">
      <Alert variant="error" title="YAML parse error">
        <pre class="ys-errors-body">{parseError}</pre>
        <p class="ys-errors-hint">
          Switch to the <strong>Text</strong> tab to fix it. The error will
          clear automatically once the document parses.
        </p>
      </Alert>
    </div>
  {/snippet}

  {#snippet bindingsEmpty()}
    <p class="ys-bindings-empty">
      Project-wide cross-refs follow the <code>id</code> / <code>name</code>
      convention by default. Custom reference-field patterns live in
      the repo's <code>.arbor/studio.toml</code> bindings.
    </p>
  {/snippet}

  {#snippet schemaIntro()}
    <p class="ys-schema-hint">
      Pick a JSON Schema file (<code>*.schema.json</code> or
      <code>*.json</code> with a <code>$schema</code> keyword) to
      decorate this YAML document. YAML Studio surfaces every
      <code>$defs</code> entry as a root candidate.
    </p>
  {/snippet}

  {#snippet auxiliaryExtras()}
    {#if importPickerOpen}
      <FileExplorerModal
        mode="file"
        title="Pick a .properties file to convert"
        extensions={['properties']}
        onConfirm={onImportPicked}
        onCancel={() => importPickerOpen = false}
      />
    {/if}
    {#if convertOpen}
      <StudioConvertPreviewModal
        mode={convertMode}
        sourceText={convertSource}
        defaultFilename={
          convertMode === 'yaml-to-properties'
            ? `${ysBasenameNoExt(yamlStudioStore.sourcePath) || 'document'}.properties`
            : `${ysBasenameNoExt(yamlStudioStore.sourcePath) || 'document'}.yaml`
        }
        onReplace={convertReplaceHandler()}
        onClose={closeConvert}
      />
    {/if}
  {/snippet}
</Studio>

<style>
  .ys-errors-wrap { padding: 16px; height: 100%; overflow: auto; }
  .ys-errors-body {
    background: var(--bg-overlay); color: var(--text-primary); padding: 10px; border-radius: 4px;
    font-family: var(--font-code); font-size: 11px; margin: 6px 0 0; overflow: auto; white-space: pre-wrap;
  }
  .ys-errors-hint { color: var(--text-muted); font-size: 11px; margin: 6px 0 0; }
  .ys-bindings-empty { color: var(--text-muted); font-size: 11px; padding: 12px; margin: 0; line-height: 1.5; }
  .ys-schema-hint { color: var(--text-secondary); font-size: 11px; line-height: 1.5; margin: 0; }
  .ys-bindings-empty code, .ys-schema-hint code {
    font-family: var(--font-code); font-size: 11px; padding: 1px 4px; border-radius: 3px;
    background: var(--bg-overlay); color: var(--text-primary);
  }
</style>
