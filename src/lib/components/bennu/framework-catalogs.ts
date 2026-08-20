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
 *
 * Which of them a project gets is {@link availableCatalogs}: having the framework is not the same
 * as having anything in the catalog, and only the second earns a place in the UI.
 */

import type { ExtEntry, ExtStat } from '$lib/ipc/bennu/ext';

/** Bottom-dock ids for the framework catalogs. */
export type FrameworkCatalogId =
  | 'labels'
  | 'beans'
  | 'librarybeans'
  | 'endpoints'
  | 'messages'
  | 'springconfig'
  | 'springbindings'
  | 'springdocumented'
  | 'jpaentities'
  | 'jparepositories'
  | 'taglibs'
  | 'bevycomponents'
  | 'bevysystems'
  | 'bevyconflicts'
  | 'bevyshaders';

/** How rows can be grouped. Which of these a catalog offers is per-catalog. */
export type GroupMode = 'none' | 'path' | 'owner' | 'kind' | 'namespace';

export interface FrameworkCatalogSpec {
  id: FrameworkCatalogId;
  /**
   * Catalog kind asked of the backend.
   *
   * Namespaced by extension id (`spring.beans`) when the panel is about one framework's own
   * model, and **bare** (`endpoints`) when it is about a concept several frameworks answer —
   * the backend unions every contribution for a bare kind, and {@link availableCatalogs}
   * matches a namespaced count against it by its tail.
   */
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
  /** What this catalog's two main columns are called when its rows are exported. A `path` and a
   *  `handler` are not a `name` and a `detail`, and a spreadsheet somebody else opens is exactly
   *  where that stops being a detail. Defaults to `name` / `detail`. */
  columns?: { primary: string; secondary: string };
  /** Whether this panel carries the "resolve against" property-file picker. */
  picker?: boolean;
  /** Give this catalog a button in the right activity rail. */
  rail?: boolean;
  /** Keyboard shortcut, shown in the palette. */
  shortcut?: string;
}

export const FRAMEWORK_CATALOGS: FrameworkCatalogSpec[] = [
  {
    // Every URL the application answers, whoever routes it. A Struts action and a
    // `@GetMapping` are the same fact about an application — and a legacy codebase mid-migration
    // has both, often for the same screen. Hence the BARE kind: the backend unions each
    // framework's contribution instead of the panel belonging to whichever one registered first.
    id: 'endpoints',
    kind: 'endpoints',
    title: 'Endpoints',
    command: 'Endpoints',
    icon: 'target',
    placeholder: 'Filter by path, verb, handler, result or interceptor…',
    empty: 'No routes found in this project — no request mappings and no Struts actions.',
    groups: [
      { id: 'path', label: 'Group by path' },
      { id: 'owner', label: 'Group by handler' },
      { id: 'kind', label: 'Group by method' },
      { id: 'none', label: 'No grouping' },
    ],
    columns: { primary: 'path', secondary: 'handler' },
    rail: true,
    shortcut: 'Alt+4',
  },
  {
    // The other half of every screen: the text is not in the source, it is in a `.properties`
    // file reached by a string. `unused` and `missing <locale>` are what you cannot see any
    // other way.
    id: 'messages',
    kind: 'messages.keys',
    title: 'Messages',
    command: 'Message bundles',
    icon: 'languages',
    placeholder: 'Filter by key, text or bundle…',
    empty: 'No message bundles found in this project.',
    groups: [
      { id: 'kind', label: 'Group by bundle' },
      { id: 'namespace', label: 'Group by key prefix' },
      { id: 'none', label: 'No grouping' },
    ],
    columns: { primary: 'key', secondary: 'text' },
  },
  {
    // The fulcrum engine's i18n: a `.ron` content file names a LABEL, not a string, and the strings
    // live in `i18n/<lang>/<category>.toml`. `unused`, `missing <lang>` and the usage rows under a
    // label are what you cannot see any other way — a mistyped label is invisible to the compiler
    // and the engine renders the label itself on screen when it cannot resolve one.
    id: 'labels',
    kind: 'fulcrum.i18n.labels',
    title: 'Labels',
    command: 'i18n labels',
    icon: 'languages',
    placeholder: 'Filter by label, text or category…',
    empty: 'No i18n labels found — this project has no i18n/languages.toml.',
    groups: [
      { id: 'kind', label: 'Group by category' },
      { id: 'namespace', label: 'Group by key prefix' },
      { id: 'none', label: 'No grouping' },
    ],
    columns: { primary: 'label', secondary: 'text' },
  },
  {
    // The ECS half of a Bevy project: what data exists, and who touches it. The sub-rows are the
    // point — "who writes `Health`" is a question about system signatures, and the text search that
    // usually stands in for it answers with every comment that mentions the word.
    //
    // It gets the rail button because it is the list you keep open while working: in an ECS the
    // component IS the architecture, and the panel is the nearest thing the codebase has to a
    // diagram of itself.
    id: 'bevycomponents',
    kind: 'bevy.components',
    title: 'Components',
    command: 'Bevy components, resources, messages and events',
    icon: 'bevy',
    placeholder: 'Filter by component, resource, message, event or bundle…',
    empty: 'No ECS declarations found — no #[derive(Component)] and no #[derive(Resource)].',
    groups: [
      { id: 'kind', label: 'Group by role' },
      { id: 'none', label: 'No grouping' },
    ],
    columns: { primary: 'type', secondary: 'touched by' },
    rail: true,
    shortcut: 'Alt+Shift+B',
  },
  {
    // Badged by schedule, so grouping by badge groups by schedule — which is the question a system
    // list is usually opened with ("what runs in FixedUpdate?").
    id: 'bevysystems',
    kind: 'bevy.systems',
    title: 'Systems',
    command: 'Bevy systems',
    icon: 'list-checks',
    placeholder: 'Filter by system, schedule or the type it touches…',
    empty: 'No systems found — no function in this project takes a Bevy system parameter.',
    groups: [
      { id: 'kind', label: 'Group by schedule' },
      { id: 'none', label: 'No grouping' },
    ],
    columns: { primary: 'system', secondary: 'accesses' },
  },
  {
    // Keyed on the SHADER, not on the material — which is the opposite direction from every other
    // Bevy catalog, and deliberately so. A `.wgsl` has no declaration to jump to and frequently
    // more than one material behind it, so "who runs this file" is a question only a list keyed
    // this way can answer. The material's own row (in Components) answers the other direction.
    //
    // Rows with a finding sort first: this list is opened to find out whether anything disagrees,
    // and a layout mismatch under nine clean shaders is a mismatch nobody sees.
    id: 'bevyshaders',
    kind: 'bevy.shaders',
    title: 'Shaders',
    command: 'Bevy shaders and the materials that run them',
    icon: 'palette',
    placeholder: 'Filter by shader, material or finding…',
    empty: 'No material in this project names a shader.',
    groups: [
      { id: 'kind', label: 'Group by material count' },
      { id: 'none', label: 'No grouping' },
    ],
    columns: { primary: 'shader', secondary: 'asset path' },
  },
  {
    // Pairs the engine must serialise. NOT a bug list: two conflicting systems that are explicitly
    // ordered are working as intended, and the row says which it is. What is worth looking at is
    // the `unordered` tag — a frame-order dependency nobody wrote down.
    id: 'bevyconflicts',
    kind: 'bevy.conflicts',
    title: 'Access conflicts',
    command: 'Bevy system access conflicts',
    icon: 'shield',
    placeholder: 'Filter by system, schedule or contended type…',
    empty: 'No pair of systems in one schedule contends over the same data.',
    groups: [
      { id: 'kind', label: 'Group by schedule' },
      { id: 'none', label: 'No grouping' },
    ],
    columns: { primary: 'pair', secondary: 'contended' },
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
    id: 'librarybeans',
    kind: 'spring.librarybeans',
    title: 'Library beans',
    command: 'Spring beans from libraries',
    icon: 'box',
    placeholder: 'Filter by bean, class or artifact…',
    // Offered only when the allowlist matched something that declares beans, so this is what
    // you see having configured a coordinate that turns out to have none.
    empty: 'No beans in the allowlisted dependencies.',
    // No grouping options: the rows ARE the artifacts, with their beans nested underneath.
    // Which dependency declared a bean is the only grouping this list wants, and it has it
    // structurally — a "group by" that regrouped it would be undoing the shape.
    groups: [],
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
  {
    id: 'taglibs',
    kind: 'jsp.taglibs',
    title: 'Tag libraries',
    command: 'JSP tag libraries',
    icon: 'list',
    placeholder: 'Filter by URI or file…',
    empty: 'No tag library descriptors were found in this project or its dependencies.',
    // The answer to "why is my tag not completing": a library that did not resolve is not here.
    columns: { primary: 'uri', secondary: 'file' },
    groups: [{ id: 'none', label: 'No grouping' }],
  },
];

/**
 * The catalogs this project has something to show in, from the overview's headline counts.
 *
 * The gate is the count, not the capability that switched the extension on, because those are
 * different questions. A Spring project is a Spring project whether or not it exposes a single
 * route — a batch job, a scheduler, a legacy XML-wired service layer — and on that project the
 * Endpoints rail button is a permanent invitation to open an empty panel. "Which frameworks
 * apply here" turns the tooling on; "what did they actually find" decides what is worth a button.
 *
 * Counts arrive namespaced by extension (`spring.endpoints`), which is exactly {@link
 * FrameworkCatalogSpec.kind} — the backend's `ExtensionRegistry::stats` does the namespacing, so
 * a second framework with an `entities` catalog can never light up JPA's panel.
 *
 * Empty until the extensions have built their models: a catalog that has not been counted yet is
 * not offered, which is the same answer as "it is empty" and never the wrong way round — a button
 * that appears when the index lands is better than one that vanishes under the pointer.
 */
export function availableCatalogs(stats: ExtStat[]): FrameworkCatalogSpec[] {
  return FRAMEWORK_CATALOGS.filter((c) =>
    stats.some((s) => countsToward(s.catalog, c.kind) && s.value > 0),
  );
}

/**
 * Whether a stat's (namespaced) catalog id counts toward a spec's kind.
 *
 * A namespaced spec matches only its own id — two frameworks with an `entities` catalog must
 * never answer for each other. A **bare** spec matches every framework's version of that
 * concept, which is what lets one Endpoints panel be lit by Spring, by Struts, or by both.
 */
export function countsToward(catalog: string | null | undefined, kind: string): boolean {
  if (!catalog) return false;
  if (kind.includes('.')) return catalog === kind;
  return catalog === kind || catalog.endsWith(`.${kind}`);
}

/** How many rows a catalog's counts add up to across every framework that contributes. */
export function catalogCount(stats: ExtStat[], kind: string): number {
  return stats.reduce((n, s) => (countsToward(s.catalog, kind) ? n + s.value : n), 0);
}

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
  // A Struts action answers every verb, so it gets the "any method" colour rather than a
  // family of its own — in a mixed list that is the honest comparison to a `@RequestMapping`
  // with no `method` element.
  if (k === 'ACTION') return 'k-any';
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
    // The steps of a Struts request, coloured by what they do rather than by where they sit:
    // an interceptor runs before the action, a result renders, a chain hands off elsewhere.
    case 'interceptor':
      return 'k-config';
    case 'tiles':
    case 'dispatcher':
      return 'k-get';
    case 'chain':
    case 'redirect':
    case 'redirectAction':
      return 'k-put';
    // A message key's translations, and — under a fulcrum label — the places that read it.
    case 'locale':
      return 'k-service';
    case 'use':
      return 'k-get';

    // ── Bevy ──────────────────────────────────────────────────────────────────
    //
    // Every one of these fell through to `k-neutral` before, which is why an ECS catalog — the
    // one list in Bennu where the badge IS the information — read as a column of grey. The
    // colours follow what a thing IS, not where it sits:
    //
    //   the data on an entity · global state · things that flow · composition · the rest
    //
    // and an ACCESS follows what it costs you: a write is the one that serialises systems and
    // the one a conflict is made of, so it takes the strong colour.
    case 'Component':
      return 'k-service';
    case 'Resource':
      return 'k-post';
    case 'Message':
    case 'Event':
      return 'k-put';
    case 'Bundle':
      return 'k-bundle';
    case 'States':
      return 'k-states';
    case 'Asset':
      return 'k-asset';
    case 'SystemParam':
      return 'k-config';
    case 'read':
    case 'read (resource)':
      return 'k-get';
    case 'write':
    case 'write (resource)':
      return 'k-delete';
    // A filter reads nothing and contends with nothing. Muted on purpose: colouring it like an
    // access is what would make the panel decorative rather than informative.
    case 'filter':
      return 'k-any';
    case 'in bundle':
    case 'inserts':
      return 'k-bundle';
    case 'spawned':
    case 'inserted':
      return 'k-spawn';
    case 'shader':
    case 'material':
      return 'k-asset';
    case 'unordered':
      return 'k-delete';
    case 'ordered':
      return 'k-get';
    default:
      return 'k-neutral';
  }
}

/**
 * Short names for the media types a mapping produces or consumes.
 *
 * `{MediaType.APPLICATION_JSON_VALUE, MediaType.TEXT_EVENT_STREAM_VALUE}` is what the source
 * says and it is thirty characters of ceremony around two facts: JSON, and SSE. In a list of two
 * hundred routes it is also the widest thing on the row, so it pushes the path — the thing you
 * are reading — into an ellipsis.
 *
 * Both spellings are recognised, the constant and the literal (`"application/json"`), because a
 * project uses whichever its author preferred and often both. Anything unrecognised keeps its
 * own last segment rather than being dropped: an unusual media type is exactly the one worth
 * seeing, and inventing a name for it would be worse than showing it.
 */
export function mediaAliases(text: string): string[] {
  const ALIASES: Record<string, string> = {
    APPLICATION_JSON: 'JSON',
    APPLICATION_PROBLEM_JSON: 'JSON+problem',
    APPLICATION_XML: 'XML',
    TEXT_XML: 'XML',
    TEXT_EVENT_STREAM: 'SSE',
    TEXT_PLAIN: 'text',
    TEXT_HTML: 'HTML',
    TEXT_MARKDOWN: 'markdown',
    APPLICATION_PDF: 'PDF',
    APPLICATION_OCTET_STREAM: 'binary',
    APPLICATION_FORM_URLENCODED: 'form',
    MULTIPART_FORM_DATA: 'multipart',
    APPLICATION_NDJSON: 'NDJSON',
    APPLICATION_STREAM_JSON: 'JSON stream',
    ALL: 'any',
  };
  const MIME: Record<string, string> = {
    'application/json': 'JSON',
    'application/problem+json': 'JSON+problem',
    'application/xml': 'XML',
    'text/xml': 'XML',
    'text/event-stream': 'SSE',
    'text/plain': 'text',
    'text/html': 'HTML',
    'text/markdown': 'markdown',
    'text/csv': 'CSV',
    'application/pdf': 'PDF',
    'application/octet-stream': 'binary',
    'application/x-www-form-urlencoded': 'form',
    'multipart/form-data': 'multipart',
    'application/x-ndjson': 'NDJSON',
    '*/*': 'any',
  };
  const out: string[] = [];
  for (const raw of text.split(',')) {
    const piece = raw.trim().replace(/^[{[]|[}\]]$/g, '').replace(/^"|"$/g, '').trim();
    if (!piece) continue;
    const constant = piece.replace(/^MediaType\./, '').replace(/_VALUE$/, '');
    const mime = piece.toLowerCase();
    const alias =
      ALIASES[constant] ??
      MIME[mime] ??
      // An unknown mime keeps its subtype (`application/vnd.acme+json` → `vnd.acme+json`); an
      // unknown constant keeps its own words.
      (mime.includes('/') ? mime.slice(mime.indexOf('/') + 1) : constant.toLowerCase().replace(/_/g, ' '));
    if (alias && !out.includes(alias)) out.push(alias);
  }
  return out;
}

/**
 * Whether a type is worth offering to open — a first, **display-side** filter over the obviously
 * scalar, so a `String` and a `boolean` do not wear an expander that would answer "nothing".
 *
 * Deliberately not the whole answer: what a name resolves to is the backend's to know (see
 * `bennu_type_shape`), and this side never claims a type *has* fields — only that asking would be
 * absurd. A name that gets past this and turns out to be a leaf simply expands to nothing once.
 */
export function looksComposite(typeText: string): boolean {
  const bare = typeText.replace(/<.*>/, '').replace(/\[\]$/, '').trim();
  const simple = bare.split('.').pop() ?? bare;
  const SCALAR = new Set([
    'void', 'boolean', 'byte', 'char', 'short', 'int', 'long', 'float', 'double',
    'Boolean', 'Byte', 'Character', 'Short', 'Integer', 'Long', 'Float', 'Double',
    'String', 'CharSequence', 'Object', 'Number', 'BigDecimal', 'BigInteger',
    'Date', 'LocalDate', 'LocalDateTime', 'Instant', 'UUID', 'Class', 'Enum',
  ]);
  // A lower-case first letter is a type variable or a primitive, never a DTO.
  return !!simple && /^[A-Z]/.test(simple) && !SCALAR.has(simple);
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
