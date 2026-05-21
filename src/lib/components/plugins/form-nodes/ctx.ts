/*
 * Shared rendering context handed down by FormNodeRenderer to every
 * sub-renderer (FormNodeLayout, FormNodeButtons, FormNodeField, …).
 *
 * Holding the renderer's $state proxies + helpers here lets each
 * sub-renderer read and mutate them without prop drilling. Svelte 5's
 * fine-grained reactivity follows the proxy, so writes from any child
 * (`ctx.values[name] = v`, `ctx.collapsedMap[id] = true`, …) update the
 * dispatcher's local state and re-trigger the affected templates.
 */
import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
import type {
  FormCondition, FormFieldAutocomplete, FormFieldRange,
  FormNode, FormSelectOption,
} from '$lib/types/plugin';

export interface FormNodeCtx {
  /** Plugin name — used to filter events and fire actions. */
  pluginName:        string;

  // ── reactive state (all $state proxies from the dispatcher) ────────────
  values:            Record<string, any>;
  fieldOverrides:    Record<string, { options?: any; disabled?: boolean }>;
  collapsedMap:      Record<string, boolean>;
  activeTabMap:      Record<string, string>;
  navQueryMap:       Record<string, string>;
  wizardStepMap:     Record<string, string>;
  treeExpanded:      Record<string, boolean>;
  treeLayoutCollapsed: Record<string, boolean>;
  filterBarState:    Record<string, { search: string; filters: Record<string, string[]> }>;
  autoOpen:          Record<string, boolean>;
  autoDynOptions:    Record<string, { value: string; label: string; group?: string }[]>;
  autoActiveIdx:     Record<string, number>;
  kvRows:            Record<string, { key: string; val: string }[]>;

  /** Validation errors per field name — read-only from sub-renderers; the
   *  parent owns the map. */
  validationErrors:  Record<string, string>;

  /** Global disable flag (e.g. parent is submitting). */
  disabled:          boolean;

  /** Name of the action currently in-flight; used to show a spinner in
   *  button nodes. */
  actionPending:     string | null;

  // ── helpers ────────────────────────────────────────────────────────────

  visible:           (n: FormNode) => boolean;
  evalCond:          (c: FormCondition) => boolean;
  resolvedDisabled:  (n: any) => boolean;
  resolvedOptions:   (n: any) => any;

  notifyChange:      (name: string, value: unknown) => void;
  handleButtonAction:(action: string, closeAfter: boolean, extra?: Record<string, unknown>) => Promise<void> | void;

  openMenu:          (e: MouseEvent, menuId: string) => void;
  closeMenu:         () => void;
  isMenuOpen:        (id: string) => boolean;

  openFilePicker:    (name: string) => void;

  toggleTreeLayoutCollapsed: (id: string) => void;

  wizardStepIndex:   (w: any) => number;

  filterAutocomplete:(field: FormFieldAutocomplete, q: string) => any[];
  onAutocompleteInput:(field: FormFieldAutocomplete) => void;
  pickAutocomplete:  (field: FormFieldAutocomplete, value: string) => void;

  buildSelectDropdownItems: (
    raw: FormSelectOption[] | undefined,
    fieldName: string,
    multiple: boolean,
    current: unknown,
  ) => DropdownItem[];
  wrapSelectChange:  (items: DropdownItem[], action: string | undefined) => DropdownItem[];
  multiselectSummary:(raw: FormSelectOption[] | undefined, selected: string[], placeholder: string) => string;
  selectLabelOf:     (raw: FormSelectOption[] | undefined, value: string) => string | undefined;
  selectItemCount:   (raw: FormSelectOption[] | undefined) => number;
  normalizeOptions:  (raw: any) => { value: string; label: string; disabled?: boolean; description?: string }[];

  fmtRange:          (n: FormFieldRange, v: number) => string;
  treeKey:           (field: string, value: string) => string;

  /** Coerce a plugin-supplied value into an array (`{}` from Lua becomes []). */
  toArr:             <T>(v: unknown) => T[];

  /** Read the `children` / `nodes` body of a section node. Mirrors
   *  `_sectionBody` in the dispatcher. */
  sectionBody:       (n: any) => any[];

  containerStyle:    (n: { style?: string; columns?: number | string; gap?: number | string }) => string;
  rowStyle:          (n: { style?: string; gap?: number | string; align?: string; wrap?: boolean }) => string;

  /** Used by `firePluginAction` callsites that bypass `handleButtonAction`
   *  (vec_field axis change, field leaf commit, autocomplete fetch). */
  firePluginAction:  (plugin: string, action: string, payload: string) => Promise<unknown>;
}
