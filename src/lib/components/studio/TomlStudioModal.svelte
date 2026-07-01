<!--
  TomlStudioModal — thin TOML wrapper around the generic `<Studio>`.

  Owns only the TOML-specific bits: container taxonomy (table /
  inline_table / array / array_of_tables), schema-aware numeric narrowing
  in `commit`, the dual-source schema sidecar (Rust struct OR JSON
  Schema), and `null_handling = as_delete` for bulk edit. Everything else
  is the generic `<Studio>`.
-->
<script lang="ts">
  import { FileText } from 'lucide-svelte';
  import Studio from './Studio.svelte';
  import type { StudioConfig, StudioTreeNode, StudioCtx, StudioCommitResult } from './studio-config';
  import type { StudioKindTone } from './StudioKindBadge.svelte';
  import { tomlStudioStore, type TomlNodeKind } from '$lib/stores/studio/toml-studio.svelte';
  import { studioBackend, type StudioPrimitiveValue } from '$lib/ipc/studio/studio-format';
  import { typeAtPath as walkTypeAtPath, flattenedStructFields } from '$lib/utils/studio-schema';

  const TOML_BE = studioBackend<TomlNodeKind>('toml');
  type TNode = StudioTreeNode<TomlNodeKind>;

  function kindBadge(k: TomlNodeKind): string {
    switch (k) {
      case 'table':           return '{}';
      case 'inline_table':    return '{ }';
      case 'array':           return '[]';
      case 'array_of_tables': return '[[]]';
      case 'string':          return '“';
      case 'integer':         return '#';
      case 'float':           return '⊘';
      case 'bool':            return '✓';
      case 'datetime':        return '🕒';
    }
  }
  function kindTone(k: TomlNodeKind): StudioKindTone {
    switch (k) {
      case 'table':
      case 'inline_table':
      case 'array':
      case 'array_of_tables': return 'keyword';
      case 'string':          return 'string';
      case 'integer':
      case 'float':
      case 'bool':            return 'number';
      case 'datetime':        return 'type';
    }
  }
  function isContainerKind(k: TomlNodeKind): boolean {
    return k === 'table' || k === 'inline_table' || k === 'array' || k === 'array_of_tables';
  }
  function isObjectLike(k: TomlNodeKind): boolean { return k === 'table' || k === 'inline_table'; }
  function isArrayLike(k: TomlNodeKind): boolean { return k === 'array' || k === 'array_of_tables'; }
  function isEditablePrimitive(k: TomlNodeKind): boolean {
    return k === 'string' || k === 'integer' || k === 'float' || k === 'bool';
  }

  async function commit(node: TNode, draft: string, ctx: StudioCtx<TomlNodeKind, TNode>): Promise<StudioCommitResult> {
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
          if (t !== 'true' && t !== 'false') throw new Error('expected "true" or "false"');
          value = { type: 'bool', value: t === 'true' };
          break;
        }
        case 'integer': {
          const s = draft.trim();
          const n = Number(s);
          if (!Number.isFinite(n)) throw new Error('not an integer');
          if (wantFloat)  { value = { type: 'float',  value: n     }; break; }
          if (wantString) { value = { type: 'string', value: draft }; break; }
          if (!Number.isInteger(n) && !/^-?\d+(_\d+)*$/.test(s)) throw new Error('not an integer');
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
      await tomlStudioStore.mutatePrimitive(node.path, value);
      await ctx.refresh(node, /* structural */ false);
      return {};
    } catch (e: any) { return { error: e?.message ?? String(e) }; }
  }

  const config: StudioConfig<TomlNodeKind, TNode> = {
    formatId: 'toml',
    backend: TOML_BE,
    formatLabel: 'TOML',
    ariaLabel: 'TOML Studio',
    defaultTitle: 'TOML Studio',
    loadingLabel: 'Parsing TOML…',
    rightPaneKey: 'arbor:toml-studio:right-pane',
    queryHistoryKey: 'arbor:toml-studio:query-history',
    queryPlaceholder: 'Query — name (recursive), $.section.key, $.servers[0], …',
    saveExtensions: ['toml'],
    saveDefaultName: 'document.toml',
    separator: ':',
    nullPolicy: 'as_delete',
    indentTooltip: 'Indent — informational; toml_edit owns per-table decor',
    formatTooltip: 'Format — re-emit through toml_edit (may normalise trailing newline / whitespace)',
    schemaPickerTitle: 'Pick schema source (.rs or .schema.json)',
    schemaPickerButton: 'Pick schema file',
    schemaPickerExts: ['rs', 'json', 'schema.json'],
    schemaRailTooltipEmpty: 'Schema — bind a Rust struct (`.rs`) or JSON Schema file',
    schemaRailLabel: 'Schema',
    schemaCssPrefix: 'ts',
    copyValueLabel: 'Copy value (TOML)',
    pasteLabel: 'Paste TOML over value…',
    kindBadgeStyle: 'tinted',

    store: tomlStudioStore,
    closeDoc: () => tomlStudioStore.closeDoc(),
    openDoc: (opts) => tomlStudioStore.openDoc(opts),
    undo: () => tomlStudioStore.undo(),
    redo: () => tomlStudioStore.redo(),
    setText: (t) => tomlStudioStore.setText(t),
    save: (opts) => tomlStudioStore.save(opts),
    applyExternalMutate: (state) => tomlStudioStore.applyExternalMutate(state),

    mutatePrimitive: (p, v) => tomlStudioStore.mutatePrimitive(p, v),
    removeAt: (p) => tomlStudioStore.removeAt(p),
    insertField: (p, k, s) => tomlStudioStore.insertField(p, k, s),
    insertItem: (p, s) => tomlStudioStore.insertItem(p, s),
    duplicateAt: (p) => tomlStudioStore.duplicateAt(p),
    moveItem: (p, d) => tomlStudioStore.moveItem(p, d),
    replaceAt: (p, s) => tomlStudioStore.replaceAt(p, s),
    newFieldSnippet: () => '""',
    newItemSnippet: (parentKind) => (parentKind === 'array' ? '""' : '{}'),

    kindBadge,
    kindTone,
    isBoolKind: (k) => k === 'bool',
    isContainerKind,
    isObjectLike,
    isArrayLike,
    isEditablePrimitive,

    sortChildren: (_k, kids) => kids,

    computeSeed: (n, valueText) => {
      let seed = valueText ?? n.preview;
      if (n.kind === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
        try { seed = JSON.parse(seed) as string; }
        catch { seed = seed.slice(1, -1); }
      }
      return seed;
    },
    commit,

    getSchemaHint: () => tomlStudioStore.schemaHint,
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

<Studio {config}>
  {#snippet headerIcon()}<FileText size={14} />{/snippet}

  {#snippet bindingsEmpty()}
    <p class="ts-bindings-empty">
      Project-wide cross-refs follow the <code>id</code> / <code>name</code>
      convention by default. Custom reference-field patterns live in
      the repo's <code>.arbor/studio.toml</code> bindings.
    </p>
  {/snippet}

  {#snippet schemaIntro()}
    <p class="ts-schema-hint">
      Pick a schema source for this TOML document:
      a Rust source file (<code>*.rs</code>) from a crate that
      deserialises this TOML via <code>serde</code>, or a JSON Schema
      file (<code>*.schema.json</code>). TOML Studio surfaces every
      struct/enum (Rust) or <code>$defs</code> entry (JSON Schema)
      as a root candidate.
    </p>
  {/snippet}
</Studio>

<style>
  .ts-bindings-empty { color: var(--text-muted); font-size: 11px; padding: 12px; margin: 0; line-height: 1.5; }
  .ts-schema-hint { color: var(--text-secondary); font-size: 11px; line-height: 1.5; margin: 0; }
  .ts-bindings-empty code, .ts-schema-hint code {
    font-family: var(--font-code); font-size: 11px; padding: 1px 4px; border-radius: 3px;
    background: var(--bg-overlay); color: var(--text-primary);
  }
</style>
