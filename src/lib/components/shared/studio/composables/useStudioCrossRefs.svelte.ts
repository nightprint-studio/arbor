/**
 * useStudioCrossRefs — owns the cross-reference detection + navigation
 * pipeline shared by every Studio modal: definition/reference field
 * naming convention, "Rename across project…" eligibility, the
 * Ctrl+click jump affordance, and the multi-target picker popover.
 *
 * Format-specific bits are passed via config:
 *   · `unquotedString` — strip the format's string-literal quotes
 *     (default: JSON-style `"..."` strip; RON overrides for char/quote
 *     escapes).
 *   · `getSourcePath` / `jumpToPath` / `openExternalDoc` — navigation
 *     hooks the wrapper wires to its store + treePane controller.
 *   · `extraEntries` — RON injects entries for already-open tabs
 *     (carrying `docId`) so the picker can short-circuit navigation
 *     instead of re-opening files on disk. Returned entries claim their
 *     `sourcePath` so the on-disk lookup won't produce duplicates.
 *   · `jumpToOpenTab` — RON-only fast path: when an entry has `docId`,
 *     activate the existing tab rather than going through
 *     `openExternalDoc`. Other formats omit it.
 */

import { studioStore } from '$lib/stores/studio.svelte';
import type { CrossRefDef, UsageMatch } from '$lib/ipc/studio';

export interface CrossRefEntry {
  sourcePath: string;
  fileName:   string;
  defPath:    string[];
  title:      string;
  /** Set when the entry resolves to an already-open Studio tab — the
   *  picker shows a "•" marker and the wrapper can short-circuit
   *  navigation via `jumpToOpenTab`. */
  docId?:     string | null;
}

export interface CrossRefsConfig<TNode> {
  /** Format ID for `studioStore.findCrossRefsForKind` and friends. */
  formatId: string;
  /** The active doc's absolute path. `null` when no doc is open. */
  getSourcePath: () => string | null;
  /** Navigate to a path within the active doc. */
  jumpToPath: (path: string[]) => Promise<void>;
  /** Open an external file as the active doc, then jump to `path`. */
  openExternalDoc: (absolutePath: string, path: string[]) => Promise<void>;
  /** Strip the format's string-literal quotes. Default = JSON `"..."`. */
  unquotedString?: (preview: string) => string | null;
  /** Field names that mark a definition site. Default: `id` / `name`.
   *  Properties returns `true` for every key (every flat key is a
   *  potential def). */
  isDefinitionFieldName?: (key: string) => boolean;
  /** Hook used to extract the format-specific ref-field name from a tree
   *  node (e.g. YAML/JSON use parent key when the value lives inside an
   *  array; RON has its own convention). Default = parent-key fallback. */
  refFieldNameForNode?: (node: TNode) => string | null;
  /** Override the ref-field check. Default consults the builtin pattern
   *  set (`*_id`/`*_ref`/`target`/`source`/…) plus the per-binding
   *  patterns in `.arbor/studio.toml`. Properties returns `true` for
   *  every key — every value is a potential reference. */
  isReferenceFieldName?: (key: string) => boolean;
  /** RON-only: inject entries for already-open tabs. Returned entries
   *  claim their `sourcePath`; on-disk matches with the same path are
   *  filtered out to avoid duplicates. */
  extraEntries?: (value: string) => CrossRefEntry[];
  /** RON-only fast path: when an entry has `docId`, activate the open
   *  tab instead of going through `openExternalDoc`. */
  jumpToOpenTab?: (docId: string, defPath: string[]) => Promise<void>;
  /** Optional per-entry enrichment hook — useful when on-disk matches
   *  may also correspond to an already-open tab (RON folds the
   *  workspace's tab list into every `CrossRefEntry`). */
  enrichOnDiskEntry?: (entry: CrossRefEntry) => CrossRefEntry;
  /** Override the on-disk lookup. Default uses
   *  `studioStore.findCrossRefsForKind(value, formatId)` — i.e. only
   *  same-format defs. RON broadens this to a project-wide lookup
   *  because RON refs can target any file format. */
  onDiskLookup?: (value: string) => CrossRefDef[];
}

export interface CrossRefs<TNode> {
  // Picker state.
  readonly crossRefPicker: { x: number; y: number; entries: CrossRefEntry[] } | null;

  // Pure helpers (re-exposed so wrappers can reuse them in inspector adapters).
  unquotedString(preview: string): string | null;
  isDefinitionFieldName(key: string): boolean;
  builtinIsReferenceField(key: string): boolean;
  isReferenceFieldName(key: string): boolean;
  refFieldNameForNode(node: TNode): string | null;
  relPathInRepo(absPath: string | null): string | null;

  // Cross-ref lookup.
  crossRefsForValue(value: string): CrossRefEntry[];
  crossRefsForNode(node: TNode): CrossRefEntry[];
  isRenameableTreeNode(node: TNode): boolean;

  // Click + portal.
  portal(node: HTMLElement): { destroy(): void };
  onCrossRefClick(entries: CrossRefEntry[], e: MouseEvent): void;
  dismissPicker(): void;

  // Navigation.
  jumpToCrossRef(target: CrossRefEntry): Promise<void>;
  jumpToUsage(hit: UsageMatch): Promise<void>;
  openDefinition(d: CrossRefDef): Promise<void>;
}

/** Default `"..."` strip (JSON/YAML/TOML behaviour). RON overrides. */
function defaultUnquotedString(preview: string): string | null {
  if (preview.length < 2) return null;
  if (!preview.startsWith('"') || !preview.endsWith('"')) return null;
  const inner = preview.slice(1, -1);
  if (inner.endsWith('…')) return null;
  return inner;
}

function defaultIsDefinitionFieldName(key: string): boolean {
  return key === 'id' || key === 'name';
}

function builtinIsRef(key: string): boolean {
  return key === 'target' || key === 'source' || key === 'parent'
      || key === 'owner'  || key === 'prev'   || key === 'next'
      || key.endsWith('_id') || key.endsWith('_ref')
      || key.endsWith('Id')  || key.endsWith('Ref');
}

function samePath(a: string | null, b: string | null): boolean {
  if (!a || !b) return false;
  return a.replace(/\\/g, '/').toLowerCase() === b.replace(/\\/g, '/').toLowerCase();
}

export function useStudioCrossRefs<TKind extends string, TNode extends { kind: TKind; key: string; path: string[]; preview: string }>(
  config: CrossRefsConfig<TNode>,
): CrossRefs<TNode> {
  let crossRefPicker = $state<{ x: number; y: number; entries: CrossRefEntry[] } | null>(null);

  const unquotedString          = config.unquotedString          ?? defaultUnquotedString;
  const isDefinitionFieldName   = config.isDefinitionFieldName   ?? defaultIsDefinitionFieldName;
  const refFieldNameForNode     = config.refFieldNameForNode     ?? defaultRefFieldName;

  function defaultRefFieldName(node: TNode): string | null {
    if ((node.kind as string) !== 'string') return null;
    const idx = parseInt(node.key, 10);
    if (Number.isInteger(idx) && String(idx) === node.key && node.path.length >= 2) {
      return node.path[node.path.length - 2];
    }
    return node.key;
  }

  function relPathInRepo(absPath: string | null): string | null {
    if (!absPath) return null;
    const norm = absPath.replace(/\\/g, '/');
    const hit = studioStore.files.find(f => f.absolute_path.replace(/\\/g, '/') === norm);
    return hit ? hit.relative_path : null;
  }

  function defaultIsReferenceFieldName(key: string): boolean {
    const repoRel = relPathInRepo(config.getSourcePath());
    const patterns = repoRel ? studioStore.referenceFieldsFor(repoRel) : null;
    if (!patterns) return builtinIsRef(key);
    return patterns.some(p => studioStore.matchesPattern(p, key));
  }
  const isReferenceFieldName = config.isReferenceFieldName ?? defaultIsReferenceFieldName;

  function defaultOnDiskLookup(value: string): CrossRefDef[] {
    return studioStore.findCrossRefsForKind(value, config.formatId);
  }
  const onDiskLookup = config.onDiskLookup ?? defaultOnDiskLookup;

  function crossRefsForValue(value: string): CrossRefEntry[] {
    const extra   = config.extraEntries?.(value) ?? [];
    const claimed = new Set<string>();
    for (const e of extra) {
      if (e.sourcePath) claimed.add(e.sourcePath);
    }
    const enrich = config.enrichOnDiskEntry;
    const ondisk = onDiskLookup(value)
      .filter(d => !claimed.has(d.absolute_path))
      .map<CrossRefEntry>(d => {
        const base: CrossRefEntry = {
          sourcePath: d.absolute_path,
          fileName:   d.file_name,
          defPath:    (d.def_path && d.def_path.length > 0) ? d.def_path : [d.def_field],
          title:      d.file_name,
        };
        return enrich ? enrich(base) : base;
      });
    return [...extra, ...ondisk];
  }

  function crossRefsForNode(node: TNode): CrossRefEntry[] {
    if ((node.kind as string) !== 'string') return [];
    const fieldName = refFieldNameForNode(node);
    if (!fieldName) return [];
    if (!isReferenceFieldName(fieldName)) return [];
    const value = unquotedString(node.preview);
    if (!value) return [];
    return crossRefsForValue(value);
  }

  function isRenameableTreeNode(n: TNode): boolean {
    if ((n.kind as string) !== 'string') return false;
    const v = unquotedString(n.preview);
    if (!v) return false;
    if (isDefinitionFieldName(n.key)) return true;
    const ref = refFieldNameForNode(n);
    return !!ref && isReferenceFieldName(ref);
  }

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy() { node.parentNode?.removeChild(node); } };
  }

  function onCrossRefClick(entries: CrossRefEntry[], e: MouseEvent): void {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    e.stopPropagation();
    if (entries.length === 1)      void jumpToCrossRef(entries[0]);
    else if (entries.length > 1)   crossRefPicker = { x: e.clientX, y: e.clientY, entries };
  }

  async function jumpToCrossRef(target: CrossRefEntry): Promise<void> {
    crossRefPicker = null;
    if (target.docId && config.jumpToOpenTab) {
      try { await config.jumpToOpenTab(target.docId, target.defPath); }
      catch (e) { console.warn('jumpToCrossRef: activate open tab failed', e); }
      return;
    }
    if (samePath(target.sourcePath, config.getSourcePath())) {
      await config.jumpToPath(target.defPath);
      return;
    }
    try { await config.openExternalDoc(target.sourcePath, target.defPath); }
    catch (e) { console.warn('jumpToCrossRef: open target failed', e); }
  }

  async function jumpToUsage(hit: UsageMatch): Promise<void> {
    if (samePath(hit.absolute_path, config.getSourcePath())) {
      await config.jumpToPath(hit.field_path);
      return;
    }
    try { await config.openExternalDoc(hit.absolute_path, hit.field_path); }
    catch (e) { console.warn('jumpToUsage: open target failed', e); }
  }

  async function openDefinition(d: CrossRefDef): Promise<void> {
    if (samePath(d.absolute_path, config.getSourcePath())) {
      await config.jumpToPath(d.def_path);
      return;
    }
    try { await config.openExternalDoc(d.absolute_path, d.def_path); }
    catch (e) { console.warn('openDefinition: open target failed', e); }
  }

  function dismissPicker() { crossRefPicker = null; }

  return {
    get crossRefPicker() { return crossRefPicker; },
    unquotedString,
    isDefinitionFieldName,
    builtinIsReferenceField: builtinIsRef,
    isReferenceFieldName,
    refFieldNameForNode,
    relPathInRepo,
    crossRefsForValue,
    crossRefsForNode,
    isRenameableTreeNode,
    portal,
    onCrossRefClick,
    dismissPicker,
    jumpToCrossRef,
    jumpToUsage,
    openDefinition,
  };
}
