/**
 * useStudioSchema — schema sidecar state + helpers shared by every
 * Studio modal: probe / load / clear / auto-load from binding hint,
 * view-source modal, type-walker adapters (delegated to the wrapper-
 * supplied `walkType` so RON's enum-aware walker can coexist with the
 * shared `studio-schema` helper), CSS chip class builders, and the
 * Inspector adapter objects.
 */

import type {
  StudioBackend,
  Schema, CrateProbe, TypeSource, ResolvedType, TypeDef, VariantDef, SchemaHint,
} from '$lib/ipc/studio-format';

export interface FlattenedStructField {
  name:        string;
  ty:          ResolvedType;
  has_default: boolean;
  aliases?:    string[];
}

export interface SchemaTypeInfo {
  label:      string;
  isUnknown:  boolean;
  isExternal: boolean;
}

export interface SchemaVariantPickerInfo {
  enumName:   string;
  currentTag: string;
  variants:   { name: string; suffix: string }[];
}

export interface SchemaMissingField {
  name:       string;
  typeLabel:  string;
  hasDefault: boolean;
}

export interface SchemaConfig<TKind extends string, TNode> {
  backend: StudioBackend<TKind>;
  /** Reactive accessor for the per-doc schema-hint (from `.arbor/studio.toml`). */
  getSchemaHint: () => SchemaHint | null;
  /** Schema walker — yaml/json/toml delegate to `studio-schema.ts`;
   *  RON uses its own enum-aware version. */
  walkType: (schema: Schema | null, path: string[]) => ResolvedType | null;
  /** Flatten struct fields (incl. serde `flatten`). Wrapper passes the
   *  shared `flattenedStructFields` or RON's variant. */
  flattenedFields: (schema: Schema, def: TypeDef & { kind: 'struct' }) => FlattenedStructField[];
  /** CSS class prefix for type chips — e.g. 'ys', 'js', 'rs'. Produces
   *  `${prefix}-type-prim`, `${prefix}-type-option`, etc. */
  cssPrefix: string;
  /** Children of the selected node, used by `inspectorMissingFields`. */
  getSelectedChildKeys: (node: TNode) => string[];
  /** Current variant tag — wrapper threads its own implementation
   *  (unquoting + currentVariantTag from crossRefs). */
  currentVariantTag: (node: TNode) => string;
}

export interface StudioSchema<TKind extends string, TNode> {
  // Reactive state (readonly).
  readonly schema:        Schema     | null;
  readonly schemaProbe:   CrateProbe | null;
  readonly schemaRsPath:  string     | null;
  readonly schemaRootSel: string     | null;
  readonly schemaLoading: boolean;
  readonly schemaError:   string     | null;
  readonly viewSource:    TypeSource | null;
  readonly viewSourceBusy: boolean;
  readonly viewSourceErr: string | null;

  // Lifecycle.
  probeSchemaSource(rsPath: string): Promise<void>;
  setSchemaRoot(canonical: string): void;
  loadSchemaForRoot(): Promise<void>;
  clearSchema(): void;
  openViewSource(canonical: string): Promise<void>;
  closeViewSource(): void;

  // Schema-aware helpers.
  typeAtPath(path: string[]): ResolvedType | null;
  enumDefAt(path: string[]): (TypeDef & { kind: 'enum' }) | null;
  primitiveHintAt(path: string[]): string | null;
  namedTypeAt(path: string[]): string | null;
  fmtType(ty: ResolvedType | null): string;
  typeChipClass(ty: ResolvedType | null): string;

  // Inspector adapters.
  inspectorSchemaTypeInfo(node: TNode): SchemaTypeInfo | null;
  inspectorVariantPickerInfo(node: TNode): SchemaVariantPickerInfo | null;
  inspectorMissingFields(node: TNode): SchemaMissingField[];
}

const _PHANTOM_VARIANT: VariantDef | null = null;
void _PHANTOM_VARIANT;

export function useStudioSchema<TKind extends string, TNode extends { kind: TKind; path: string[] }>(
  config: SchemaConfig<TKind, TNode>,
): StudioSchema<TKind, TNode> {
  let schema        = $state<Schema     | null>(null);
  let schemaProbe   = $state<CrateProbe | null>(null);
  let schemaRsPath  = $state<string     | null>(null);
  let schemaRootSel = $state<string     | null>(null);
  let schemaLoading = $state(false);
  let schemaError   = $state<string     | null>(null);

  let viewSource    = $state<TypeSource | null>(null);
  let viewSourceBusy = $state(false);
  let viewSourceErr  = $state<string | null>(null);

  async function probeSchemaSource(rsPath: string): Promise<void> {
    schemaLoading = true;
    schemaError   = null;
    try {
      const probe = await config.backend.schemaProbe(rsPath);
      schemaProbe   = probe;
      schemaRsPath  = rsPath;
      schemaRootSel = probe.root_candidates[0]?.canonical_path ?? null;
      schema        = null;
    } catch (e) {
      schemaError = String(e);
      schemaProbe = null;
      schemaRootSel = null;
    } finally {
      schemaLoading = false;
    }
  }

  function setSchemaRoot(canonical: string): void { schemaRootSel = canonical; }

  async function loadSchemaForRoot(): Promise<void> {
    if (!schemaRsPath || !schemaRootSel) return;
    schemaLoading = true;
    schemaError   = null;
    try {
      schema = await config.backend.schemaLoad(schemaRsPath, schemaRootSel);
    } catch (e) {
      schemaError = String(e);
    } finally {
      schemaLoading = false;
    }
  }

  function clearSchema(): void {
    schema       = null;
    schemaProbe  = null;
    schemaRsPath = null;
    schemaRootSel = null;
    schemaError  = null;
  }

  async function openViewSource(canonical: string): Promise<void> {
    if (!schemaRsPath) return;
    viewSourceBusy = true;
    viewSourceErr  = null;
    viewSource     = null;
    try {
      viewSource = await config.backend.schemaViewSource(schemaRsPath, canonical);
    } catch (e) {
      viewSourceErr = String(e);
    } finally {
      viewSourceBusy = false;
    }
  }
  function closeViewSource(): void { viewSource = null; viewSourceErr = null; }

  // Auto-load schema from the .arbor/studio.toml binding.
  $effect(() => {
    const hint = config.getSchemaHint();
    if (!hint) return;
    if (schema && schemaRsPath === hint.rs_file && schema.root_type === hint.root_type) return;
    void autoLoadSchemaFromHint(hint.rs_file, hint.root_type);
  });

  async function autoLoadSchemaFromHint(rsFile: string, rootCanonical: string): Promise<void> {
    schemaRsPath  = rsFile;
    schema        = null;
    schemaError   = null;
    schemaLoading = true;
    try {
      schemaProbe   = await config.backend.schemaProbe(rsFile);
      schemaRootSel = rootCanonical;
      schema        = await config.backend.schemaLoad(rsFile, rootCanonical);
    } catch (e) {
      schemaError = String(e);
    } finally {
      schemaLoading = false;
    }
  }

  function typeAtPath(path: string[]): ResolvedType | null {
    return config.walkType(schema, path);
  }

  function enumDefAt(path: string[]): (TypeDef & { kind: 'enum' }) | null {
    if (!schema) return null;
    let ty = typeAtPath(path);
    if (!ty) return null;
    if (ty.kind === 'option') ty = ty.inner;
    if (ty.kind !== 'named') return null;
    const def = schema.types[ty.path];
    if (!def || def.kind !== 'enum') return null;
    return def;
  }

  function primitiveHintAt(path: string[]): string | null {
    let ty = typeAtPath(path);
    if (!ty) return null;
    if (ty.kind === 'option') ty = ty.inner;
    if (ty.kind === 'primitive') return ty.name;
    return null;
  }

  function fmtType(ty: ResolvedType | null): string {
    if (!ty) return '';
    switch (ty.kind) {
      case 'primitive': return ty.name;
      case 'option':    return `Option<${fmtType(ty.inner)}>`;
      case 'vec':       return `Vec<${fmtType(ty.inner)}>`;
      case 'map':       return `Map<${fmtType(ty.key)}, ${fmtType(ty.value)}>`;
      case 'tuple':     return `(${ty.items.map(fmtType).join(', ')})`;
      case 'named':     return ty.path.replace(/^crate::/, '').replace(/^#\//, '');
      case 'external':  return ty.path + ' (external)';
      case 'unknown':   return `? ${ty.hint}`;
    }
  }

  function typeChipClass(ty: ResolvedType | null): string {
    if (!ty) return '';
    const p = config.cssPrefix;
    switch (ty.kind) {
      case 'primitive': return `${p}-type-prim`;
      case 'option':    return `${p}-type-option`;
      case 'vec':       return `${p}-type-vec`;
      case 'map':       return `${p}-type-map`;
      case 'tuple':     return `${p}-type-tupletype`;
      case 'external':  return `${p}-type-external`;
      case 'unknown':   return `${p}-type-unknown`;
      default:          return '';
    }
  }

  function namedTypeAt(path: string[]): string | null {
    if (!schema) return null;
    const ty = typeAtPath(path);
    if (!ty) return null;
    const named = ty.kind === 'named' ? ty
                : ty.kind === 'option' && ty.inner.kind === 'named' ? ty.inner
                : null;
    if (!named) return null;
    const pStr = named.path.replace(/^crate::/, '').replace(/^#\//, '');
    return pStr.split('/').pop()?.split('::').pop() ?? null;
  }

  function inspectorSchemaTypeInfo(node: TNode): SchemaTypeInfo | null {
    if (!schema) return null;
    const ty = typeAtPath(node.path);
    if (!ty) return null;
    return {
      label:      fmtType(ty),
      isUnknown:  ty.kind === 'unknown',
      isExternal: ty.kind === 'external',
    };
  }

  function inspectorVariantPickerInfo(node: TNode): SchemaVariantPickerInfo | null {
    if (!schema || (node.kind as string) !== 'string') return null;
    const def = enumDefAt(node.path);
    if (!def || def.variants.length === 0) return null;
    return {
      enumName:   def.name,
      currentTag: config.currentVariantTag(node),
      variants:   def.variants.map((v: VariantDef) => ({
        name:   v.name,
        suffix: v.shape === 'unit' ? '' : v.shape === 'tuple' ? '(…)' : ' { … }',
      })),
    };
  }

  function inspectorMissingFields(node: TNode): SchemaMissingField[] {
    if (!schema) return [];
    const ty = typeAtPath(node.path);
    if (!ty || ty.kind !== 'named') return [];
    const def = schema.types[ty.path];
    if (!def || def.kind !== 'struct') return [];
    const seenSegs = new Set(config.getSelectedChildKeys(node));
    return config.flattenedFields(schema, def)
      .filter(f => !seenSegs.has(f.name) && !(f.aliases ?? []).some(a => seenSegs.has(a)))
      .map(f => ({
        name:       f.name,
        typeLabel:  fmtType(f.ty),
        hasDefault: f.has_default,
      }));
  }

  return {
    get schema()        { return schema; },
    get schemaProbe()   { return schemaProbe; },
    get schemaRsPath()  { return schemaRsPath; },
    get schemaRootSel() { return schemaRootSel; },
    get schemaLoading() { return schemaLoading; },
    get schemaError()   { return schemaError; },
    get viewSource()    { return viewSource; },
    get viewSourceBusy() { return viewSourceBusy; },
    get viewSourceErr() { return viewSourceErr; },

    probeSchemaSource,
    setSchemaRoot,
    loadSchemaForRoot,
    clearSchema,
    openViewSource,
    closeViewSource,

    typeAtPath,
    enumDefAt,
    primitiveHintAt,
    namedTypeAt,
    fmtType,
    typeChipClass,

    inspectorSchemaTypeInfo,
    inspectorVariantPickerInfo,
    inspectorMissingFields,
  };
}
