/**
 * The framework catalogs Bennu surfaces as bottom panels — plus how each one is grouped and
 * coloured.
 *
 * One table, read by four places that would otherwise drift: the bottom dock (which panel to
 * mount), the activity rail (which button exists), the command palette (which entries to offer),
 * and the panel itself (its title, its groupings, its empty state). Adding a catalog — Spring
 * Data repositories, scheduled tasks, a second framework entirely — is a row here plus a backend
 * `catalog(kind)` arm. No new component, no new store.
 *
 * Most of these are **palette-only**: a framework tool is relevant to the projects that use that
 * framework and invisible noise on the ones that don't, and the activity rail is the one piece of
 * chrome that is always on screen. Endpoints is the exception — it earns a rail button because a
 * route list is something you keep open while working, not something you go and fetch.
 */

import type { ExtEntry } from '$lib/ipc/bennu/ext';

/** Bottom-dock ids for the framework catalogs. */
export type FrameworkCatalogId =
  | 'beans'
  | 'endpoints'
  | 'springconfig'
  | 'springbindings'
  | 'springdocumented'
  | 'jpaentities'
  | 'jparepositories';

/** How rows can be grouped. Which of these a catalog offers is per-catalog. */
export type GroupMode = 'none' | 'path' | 'owner' | 'kind' | 'namespace';

export interface FrameworkCatalogSpec {
  id: FrameworkCatalogId;
  /** Catalog kind asked of the backend, namespaced by extension id. */
  kind: string;
  /** Panel title + rail tooltip. */
  title: string;
  /** Command-palette entry label (fuller than the panel title). */
  command: string;
  /** Icon key resolved by the window's palette icon map. */
  icon: string;
  placeholder: string;
  /** Shown when the catalog is genuinely empty (not merely filtered). */
  empty: string;
  /** Groupings this catalog offers, in menu order. The first is the default. */
  groups: { id: GroupMode; label: string }[];
  /** Whether this panel carries the "resolve against" property-file picker. */
  picker?: boolean;
  /** Give this catalog a button in the right activity rail. */
  rail?: boolean;
  /** Keyboard shortcut, shown in the palette. */
  shortcut?: string;
}

export const FRAMEWORK_CATALOGS: FrameworkCatalogSpec[] = [
  {
    id: 'endpoints',
    kind: 'spring.endpoints',
    title: 'Endpoints',
    command: 'Spring endpoints',
    icon: 'target',
    placeholder: 'Filter by path, verb, handler, return type or parameter…',
    empty: 'No request mappings found in this project.',
    groups: [
      { id: 'path', label: 'Group by path' },
      { id: 'owner', label: 'Group by controller' },
      { id: 'kind', label: 'Group by method' },
      { id: 'none', label: 'No grouping' },
    ],
    rail: true,
    shortcut: 'Alt+4',
  },
  {
    id: 'beans',
    kind: 'spring.beans',
    title: 'Beans',
    command: 'Spring beans',
    icon: 'box',
    placeholder: 'Filter by name, class or stereotype…',
    empty: 'No Spring beans found in this project.',
    groups: [
      { id: 'kind', label: 'Group by stereotype' },
      { id: 'owner', label: 'Group by package' },
      { id: 'none', label: 'No grouping' },
    ],
    shortcut: 'Ctrl+Shift+B',
  },
  {
    id: 'springconfig',
    kind: 'spring.properties',
    title: 'Config',
    command: 'Spring configuration',
    icon: 'sliders',
    placeholder: 'Filter by key or value…',
    empty: 'No application property files found in this project.',
    groups: [
      { id: 'kind', label: 'Group by file' },
      { id: 'none', label: 'No grouping' },
    ],
    picker: true,
  },
  {
    // The property reference, version-matched to this project's jars: everything Spring and the
    // libraries on the classpath accept, whether or not the project sets it. What you would
    // otherwise keep open in a browser tab, except that this one knows which keys you already use.
    id: 'springdocumented',
    kind: 'spring.documented',
    title: 'Property reference',
    command: 'Spring property reference',
    icon: 'book',
    placeholder: 'Filter by key, type or description…',
    empty: 'No configuration metadata found — build the project once so its dependencies resolve.',
    groups: [
      { id: 'namespace', label: 'Group by namespace' },
      { id: 'none', label: 'No grouping' },
    ],
  },
  {
    id: 'jpaentities',
    kind: 'jpa.entities',
    title: 'Entities',
    command: 'JPA entities',
    icon: 'box',
    placeholder: 'Filter by entity, table, column or field…',
    empty: 'No @Entity classes found in this project.',
    groups: [
      { id: 'kind', label: 'Group by kind' },
      { id: 'owner', label: 'Group by package' },
      { id: 'none', label: 'No grouping' },
    ],
  },
  {
    id: 'jparepositories',
    kind: 'jpa.repositories',
    title: 'Repositories',
    command: 'JPA repositories',
    icon: 'list',
    placeholder: 'Filter by repository, entity or query…',
    empty: 'No Spring Data repositories found in this project.',
    groups: [
      { id: 'kind', label: 'Group by base interface' },
      { id: 'owner', label: 'Group by package' },
      { id: 'none', label: 'No grouping' },
    ],
  },
  {
    id: 'springbindings',
    kind: 'spring.bindings',
    title: 'Bound properties',
    command: 'Spring bound properties',
    icon: 'list',
    placeholder: 'Filter by key, class or type…',
    empty: 'No @ConfigurationProperties classes in this project.',
    groups: [
      { id: 'owner', label: 'Group by class' },
      { id: 'none', label: 'No grouping' },
    ],
  },
];

/** Whether a bottom-dock id is one of the framework catalogs. */
export function isFrameworkCatalog(id: string): id is FrameworkCatalogId {
  return FRAMEWORK_CATALOGS.some((c) => c.id === id);
}

/** The spec for a catalog id. Falls back to the first entry rather than throwing — a panel with
 *  the wrong title is a bug you can see; a crashed dock is one you can't. */
export function catalogFor(id: FrameworkCatalogId): FrameworkCatalogSpec {
  return FRAMEWORK_CATALOGS.find((c) => c.id === id) ?? FRAMEWORK_CATALOGS[0];
}

/**
 * The group a row belongs to under `mode`.
 *
 * Derived from the row's generic fields rather than from catalog-specific knowledge, which is
 * what keeps one panel able to group all of them: `path` takes the first URL segment, `owner`
 * takes the declaring class (before the `#`) or the package, `kind` is the badge itself.
 */
export function groupKeyOf(mode: GroupMode, row: ExtEntry): string {
  switch (mode) {
    case 'kind':
      return row.kind || '—';
    case 'path': {
      const seg = row.primary.split('/').filter(Boolean)[0];
      return seg ? `/${seg}` : '/';
    }
    case 'namespace': {
      // Two leading segments, because one is uselessly coarse for property keys — everything
      // interesting lives under `spring.` — and three splits families that belong together.
      const segs = row.primary.split('.');
      return segs.length > 1 ? segs.slice(0, 2).join('.') : row.primary || '—';
    }
    case 'owner': {
      if (row.secondary.includes('#')) return row.secondary.split('#')[0];
      const dot = row.secondary.lastIndexOf('.');
      return dot > 0 ? row.secondary.slice(0, dot) : row.secondary || '—';
    }
    default:
      return '';
  }
}

/**
 * The colour class for a row's badge.
 *
 * HTTP methods are the reason this exists: a list where `GET`, `POST` and `DELETE` are the same
 * colour makes you read every badge, and the whole value of a route list is being able to skim
 * it. Destructive verbs are red, mutating ones warm, reads cool — the same convention every API
 * console uses, so it needs no legend.
 */
export function kindClass(kind: string): string {
  const k = kind.toUpperCase();
  if (k.includes('DELETE')) return 'k-delete';
  if (k.includes('POST')) return 'k-post';
  if (k.includes('PUT') || k.includes('PATCH')) return 'k-put';
  if (k.includes('GET')) return 'k-get';
  if (k === 'ANY') return 'k-any';
  // Bean stereotypes and parameter bindings get their own families.
  switch (kind) {
    case '@Service':
    case '@Component':
      return 'k-service';
    case '@Repository':
      return 'k-repository';
    case '@Controller':
    case '@RestController':
      return 'k-controller';
    case '@Configuration':
    case '@Bean':
      return 'k-config';
    case '<bean>':
      return 'k-xml';
    // JPA. The distinction that earns a colour is how a repository method is written — a
    // derived name is compiled from the name at startup, a `@Query` is not, and a native one
    // bypasses the entity model entirely. Three risks, three colours.
    case 'derived':
      return 'k-get';
    case '@Query':
      return 'k-put';
    case 'native':
      return 'k-delete';
    case 'entity':
      return 'k-service';
    case 'embeddable':
    case 'mapped-superclass':
      return 'k-config';
    case 'id':
      return 'k-controller';
    case 'OneToMany':
    case 'ManyToOne':
    case 'OneToOne':
    case 'ManyToMany':
      return 'k-repository';
    case 'column':
      return 'k-neutral';
    case 'path':
      return 'k-get';
    case 'query':
      return 'k-put';
    case 'body':
      return 'k-post';
    case 'arg':
      return 'k-any';
    default:
      return 'k-neutral';
  }
}

/** Text a row is matched against by the filter — including its children, so filtering by a
 *  parameter name or type finds the route that takes it. */
export function searchTextOf(row: ExtEntry): string {
  const own = `${row.primary} ${row.secondary} ${row.kind} ${row.tags.join(' ')}`;
  const kids = childrenOf(row).map((c) => `${c.primary} ${c.secondary} ${c.kind}`).join(' ');
  return `${own} ${kids}`.toLowerCase();
}

/** A row's sub-rows, tolerating a payload from a backend that predates them — during
 *  development the running `bennu-be` is often one build behind the frontend, and a missing
 *  field should degrade to "no children", not to a crashed panel. */
export function childrenOf(row: ExtEntry): ExtEntry[] {
  return row.children ?? [];
}
