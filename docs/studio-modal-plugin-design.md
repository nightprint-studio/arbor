# Studio-shaped plugin modals — design proposal

**Stato**: bozza per discussione. Blocco C del piano in
[`plugins/properties-studio-lite/HANDOFF.md`](../plugins/properties-studio-lite/HANDOFF.md).
Non è codice; è la proposta dell'API che D ed E implementeranno.

## Obiettivo

Permettere a un plugin di costruire una modale **shape-equivalent** alle
Studio modal della casa (`RonStudioModal`, `JsonStudioModal`,
`PropertiesStudioModal`, `TomlStudioModal`, `YamlStudioModal`) usando solo
l'API plugin — niente Svelte custom. "Shape-equivalent" significa
riprodurre le **stesse zone strutturali** e gli **stessi pattern di
interazione**; un plugin che vuole una UX da text-editor strutturato deve
poterla disegnare senza forkare il chrome.

Le 7 zone strutturali da coprire (in ordine di lettura):

| Zona | Esempio in PropertiesStudio |
|------|------------------------------|
| 1. Icona + titolo (file-icon · nome doc · dirty dot · meta) | `headerLeft` snippet |
| 2. Tab strip *file* (workspace di documenti, "Open …" launcher) | parte di `headerLeft` |
| 3. Tab strip *view-mode* centrale (Tree / Text / Diff / Errors) | shell-rendered `<Tabs>` |
| 4. Activity bar destra (Inspector · Query · Bindings · Schema · Tools) | `rightRailButtons` snippet |
| 5. Body principale (banner area + query bar + view body) | `bodyBanners` + `queryBarSlot` + `bodyMain` |
| 6. Sidecar destro animato (300-360px, slide-in, contenuto per pane) | `bindings/inspector/query/schema/tools` snippet |
| 7. Footer 3-zone (status pills · tools centrali · CTA Save split-button) | `footerStatusLeft` + `footerCenter` + `footerRight` |

## Cosa c'è già nell'API plugin

`arbor.ui.form{...}` oggi copre:

- `title`, `description` → Zona 1 (versione minima, no icon, no meta).
- `tabs` FormNode → Zona 3 quando il plugin lo mette in cima al body.
- `tree_layout` FormNode → un nav-sidebar 2-region (`nav_children` +
  `content_children`) con resize/collapse. È **dentro il body**, non
  un'activity bar; si comporta come 1 sola "view" senza tab.
- Submit / Cancel + wizard Back/Next nel footer → Zona 7-right minima.
- `loading` overlay → fallback state, ma niente error/empty alternativi.

E sotto: `Modal.svelte` accetta già `leftRail` e `rightRail` snippet
(rese via il widget condiviso `<ActivityBar>`). **L'infrastruttura
host esiste** — manca solo l'API plugin-facing che la piloti.

## Cosa manca rispetto a StudioModal

Mapping zona-per-zona dei gap:

1. **Icona** del titolo: oggi solo testo. Nessuna primitiva per
   `brand_icon` / `lucide_icon` / SVG inline accanto al titolo.
2. **Tab strip file** (multi-doc workspace): nessuna primitiva.
   `tabs` cambia VIEW dentro il body, non documento.
3. **View-mode tabs**: si possono fare oggi con `tabs` in cima al body,
   ma sono *dentro* il body — non sono un'header-tab cluster, hanno il
   loro chrome (border, padding) e occupano spazio above the fold.
4. **Activity bar**: non esposta. Workaround impossibile — il
   `tree_layout.nav_children` è un nav, non un activity-bar (no buttons
   verticali, no "active pane" routing).
5. **Banner zone separata** dal body: oggi un `alert` FormNode si piazza
   come fratello degli altri nodi e scrolla con loro. Pattern non
   utilizzabile per banner persistenti che devono restare visibili
   sopra una view scrollable.
6. **Sidecar animato**: il `tree_layout` è il più vicino, ma è
   fissa (un solo `nav_children`), non multi-pane con switch persistito.
7. **Footer 3-zone**: oggi solo Submit/Cancel/wizard nel footer. Niente
   modo di iniettare pill di stato (parse · dirty · saved), tool bottoni
   centrali (Format / Convert / undo-redo), o sostituire il Submit con
   uno split-button Save+SaveAs.

Più tre capability orizzontali:

- **State fallback** (loading / error / empty / parseError) — oggi
  solo `loading`. Una Studio modal sceglie tra 4 stati visivi prima di
  decidere se renderizzare il body.
- **Persistenza** della selezione utente (active sidecar / active
  view-mode) tra aperture: oggi i plugin la rifarebbero a mano via
  `arbor.kv` se l'avessero.
- **Cross-modal portal** (`auxiliary` snippet in StudioModal — ospita
  FilePicker / rename / view-source senza nesting). I plugin già hanno
  `arbor.fs.pick_file` e `arbor.ui.confirm` per la maggior parte dei
  casi; il portal è solo zucchero per le Studio.

## Strade considerate

### Strada A — Estendere `arbor.ui.form`

Aggiungere campi top-level dichiarativi: `header`, `activity_bar`,
`sidecars`, `footer`, `state_block`. Tutto opzionale; assenza = comportamento
attuale.

**Pro**

- Zero nuovo entry-point, zero nuova lifecycle. I plugin che oggi
  passano `nodes`/`state`/`submit_action` continuano a funzionare.
- L'API è dichiarativa e JSON-shaped — niente snippet, niente DSL.
- `patch` / `replace` / `set_value` / `set_loading` funzionano senza
  modifiche concettuali. Le nuove zone sono nodi nello stesso albero,
  con i loro `id` patchabili.
- I FormNode esistenti (compresi quelli aggiunti nei Blocchi A/B —
  `brand_icon`, `kbd`, `experimental_badge`, `state_block`,
  `bottom_panel_header`, …) sono **già la libreria** per popolare le
  nuove zone. Niente reinvenzione.

**Contro**

- `arbor.ui.form` diventa più grasso (più campi top-level). Il footer
  passa da 2 azioni a 3 zone; il header da `title` a 4 sub-key.
- Plugin che vogliono solo "submit + 3 input" hanno più API da
  ignorare. Mitigato dalla regola di opt-in: ogni nuovo campo assente
  = render attuale.

### Strada B — Nuovo entry-point `arbor.ui.studio({...})`

Primitivo parallelo dedicato, con lifecycle separato (`plugin:studio`
event, `arbor.ui.studio.patch / set_pane / close`).

**Pro**

- API "marcata" — il plugin sa quando sta costruendo qualcosa di più
  pesante di una form.
- Spazio libero per scelte specifiche (multi-doc, undo-redo handles…)
  senza vincoli di compat.

**Contro**

- Duplicazione massiccia: lifecycle, patch, set_value, set_options,
  set_state_path, set_loading, replace, close — tutto da ri-implementare
  o da fattorizzare via trait. Costo Rust + frontend non banale.
- Due primitive parallele a maintenance: ogni nuova feature di
  arbor.ui.form richiede mirror su studio (validation pattern, sidebar
  flag, debounced `actions.change`, ecc.) o decide cosa lasciare
  asimmetrico — il drift è certo.
- Frontiera artificiale: la differenza tra "form complessa" e "studio
  modal" non è binaria. Una settings modal con 4 sezioni + activity bar
  laterale è una form con attività bar — non un "altro tipo di modale".

### Strada C — Shell-FormNode dedicati (composable)

Tutto il chrome è composto da nuovi FormNode che vivono ai bordi del
body: `activity_bar`, `header_strip`, `footer_strip`, `body_banner`,
`view_mode_tabs`. L'albero `nodes` resta unico; certi nodi-root sono
"speciali" e finiscono nello slot giusto del Modal anziché nel body.

**Pro**

- API "puramente composizionale". Nessun nuovo top-level field.
- Renderer-side, la logica di slotting è una funzione pura `walk(nodes)`
  che separa nodi-chrome da nodi-body.

**Contro**

- Implicito: il plugin scrive una flat list e magicamente certi nodi
  scompaiono dal flow e appaiono nel footer/header. La regola
  "il nodo X esce dal body solo se è il PRIMO root" o "se è un fratello
  diretto della root" diventa una convenzione fragile. Tipo: cosa
  succede se metto `footer_strip` dentro un `section`? E se ne metto
  due?
- Brain-overhead alto rispetto a un campo `footer = {...}` esplicito.
- Patch via `id` continua a funzionare ma il "dove vive" del nodo è
  ambiguo finché non leggi il codice del renderer.

## Strada raccomandata

**Strada A**, integrata da nuovi FormNode per i contenuti dove serve
(Strada C in piccolo — ma vincolato: i FormNode di chrome esistono e
sono usabili **solo** dentro le sotto-key dell'`header`/`footer`/`sidecars`,
non sparse nella `nodes` root).

Razionale:

- È la più piccola sovrapproposta. Estende l'API esistente nella stessa
  direzione in cui si è mossa finora (tutti i nuovi campi del Blocco A/B
  sono opzionali, additivi, dichiarativi).
- Il lifecycle è uno solo — niente duplicazione Rust.
- I FormNode appena aggiunti nel Blocco A (`brand_icon`, `kbd`,
  `experimental_badge`, `state_block`, `bottom_panel_header`,
  `monogram`, `breadcrumb`, …) sono *esattamente* il vocabolario per
  popolare le nuove zone. Senza Blocco A questa proposta sarebbe
  prematura; con A chiuso, è già il momento naturale.

## Forma proposta dell'API

Tutta la superficie qui sotto è **additiva** su `PluginFormConfig`. Tutti
i campi sono opzionali; assenza = comportamento odierno.

### `header` — Zone 1, 2, 3

```lua
arbor.ui.form{
  title = "Mio Studio",
  header = {
    -- Zona 1: icon. Una delle tre forme. Mutuamente esclusive.
    icon = { lucide = "FileText" },               -- nome Lucide
    -- icon = { brand  = "github" },              -- via brand_icon set
    -- icon = { image  = "file:///…/glyph.png" }, -- URL (file:// / data: / https) — host-rendered <img>

    -- Zona 1 supplementari:
    subtitle = "config/app.properties",           -- meta singola riga, muted
    dirty    = true,                              -- mostra `●` accanto al titolo
    tooltip  = "/absolute/path",                  -- tooltip sul titolo
    size_meta = "12.4 KB · 412 lines",            -- secondary meta pill

    -- Zone 2 + 3 (left/centre/right). Liste piatte di FormNode.
    -- `centre` ospita tipicamente `tabs` per il view-mode switcher.
    left   = { ... FormNode[] ... },              -- dopo il titolo, prima del centre
    centre = { ... FormNode[] ... },              -- tab strip view-mode (o vuoto)
    right  = { ... FormNode[] ... },              -- prima della close button (host-owned)

    -- Default-rendered Experimental badge per plugin in alpha — opt-in.
    experimental = { description = "…" },
  },
  -- nodes = { ... } come oggi: questo è il body scrollabile (Zona 5).
}
```

Note di rendering:

- Se `header` è assente → fallback al chrome attuale (`ModalHeader` con
  `plugin_name` + `title`).
- Se `header.centre` contiene un `tabs` FormNode, viene reso con
  `variant = "solid"` e `size = "sm"` come in StudioModal (override via
  prop esplicite del nodo `tabs` resta possibile).
- La close-button host-owned resta sempre all'estrema destra.
- `icon.image` accetta una URL (`file://`, `data:`, `https://`) renderizzata
  via `<img>` con `width = height = 18`. Niente SVG raw — la superficie
  XSS non vale il marginal benefit (vedi decisione Q1 in fondo).

### `activity_bar` — Zona 4 (e 4-sinistra come bonus)

```lua
activity_bar = {
  side = "right",                                  -- "left" | "right" | "both"
  -- Con "both", usa `left_items` / `right_items` invece di `items`.
  items = {
    {
      id      = "inspector",
      icon    = "ScanSearch",
      label   = "Inspector",                       -- shown only on hover (aria-label too)
      tooltip = "Selected node detail",
      -- Visual modifiers driven by patch:
      count   = 3,                                 -- numeric badge (omit / 0 = none)
      dot     = true,                              -- accent dot for "has unread"
      tone    = "warning",                         -- override badge color
      disabled = false,
    },
    { separator = true },                          -- thin divider in the bar
    { id = "schema",    icon = "BookOpen",  label = "Schema"        },
    { id = "tools",     icon = "Wrench",    label = "Tools"         },
    -- N.B. Activity-bar items are ROUTING-ONLY: ognuno DEVE avere un
    -- sidecar omonimo in `sidecars` (vedi Q2). Per pulsanti "azione"
    -- (Open file…, Save As…, Settings…) usa `header.left` o `header.right`
    -- con `button` FormNode standard.
  },
  default      = "inspector",                      -- which one is open at first mount
  storage_key  = "plug:propstudio:rightpane",      -- persists across opens
  always_open  = false,                            -- true → never null (one always selected)
  -- Optional left bar — typically file-tree / explorer toggles.
  -- Same shape as above. (Or use `side = "both"` with paired keys.)
},
```

Stato esposto al plugin:

- L'attivo è materializzato in `liveState.active_sidecar` (string `id` o
  `nil` quando `always_open = false` e l'utente chiude tutto).
- Lettura: il plugin legge `ctx.state.active_sidecar` nei dispatch
  handlers.
- Scrittura programmatica:
  `arbor.ui.form.set_sidecar("inspector")` (alias di `set_state_path({"active_sidecar"}, "inspector")`).
- Hook: nessuno per ora — il pattern è "pulla lo stato quando ti serve".
  Plug-in che vogliono reagire usano `on_form_state_change` se mai
  esisterà (vedi *Open questions Q3*).

### `sidecars` — Zona 6

```lua
sidecars = {
  -- Una entry OBBLIGATORIA per ogni `id` dichiarato nell'activity bar.
  -- Items senza sidecar omonimo sono un errore di config (logged warning).
  inspector = {
    width    = 320,                                -- default 320
    title    = "Inspector",                        -- header del sidecar
    children = { ... FormNode[] ... },
  },
  schema = {
    width    = 360,
    title    = "Schema",
    children = { ... FormNode[] ... },
  },
},
```

Note:

- Il sidecar è una mini-form annidata: i nodi possono essere
  value-bearing (`text`, `select`, `tree`, …) e i loro valori
  partecipano normalmente al submit payload (`values[...]`). I plugin
  che vogliono submit isolato per sidecar montano un `button` interno.
- Width per-sidecar; lo shell normalizza alle proporzioni del modale.
- Animazione slide identica a StudioModal (re-uso del CSS già fattorizzato
  via `--anim-d-panel`).

### `footer` — Zona 7

```lua
footer = {
  -- Tre zone, ognuna lista di FormNode.
  status = { ... FormNode[] ... },                 -- pill / breadcrumb / kbd hint
  center = { ... FormNode[] ... },                 -- undo-redo / Format / Convert
  right  = { ... FormNode[] ... },                 -- override completo del CTA

  -- Quando `right` è assente, il default Submit/Cancel/wizard resta.
  hide_submit = false,
  hide_cancel = false,
  submit_label = "Save", submit_action = "save",   -- come oggi
},
```

### `state_block` — fallback states

Generalizzazione del `loading` esistente per coprire i 4 stati visivi di
StudioModal (loading / error / empty / hasDoc).

```lua
state_block = {
  -- One-of: state_block.* sovrascrive il body quando settato.
  -- Settati live via arbor.ui.form.set_state_block({ ... }).
  loading = { label = "Parsing 12 MB…" },          -- spinner + label
  error   = { label = "Parse failed: …" },         -- StateBlock tone="error"
  empty   = {
    title    = "No document",
    body     = "Use the Open file button to start.",
    cta_label = "Open file…",
    cta_action = "open_file",
  },
},
-- Quando nessuna chiave è attiva, il body normale (nodes) è reso.
```

Nuova API runtime:

- `arbor.ui.form.set_state_block(name, cfg)` — flip in/out di uno stato
  (`name = "loading" | "error" | "empty" | nil`).
- `arbor.ui.form.set_loading(...)` continua a esistere; è zucchero per
  `set_state_block("loading", { label = ... })` / `nil`.

### Lifecycle e persistenza

Nessun nuovo evento Tauri. Tutto passa per `plugin:form-update` come
oggi:

- `set_sidecar` → emette `{ op = "set_state_path", path = {"active_sidecar"}, value = "..." }`.
- `set_state_block` → nuovo op `set_state_block`.
- `patch` con `id` continua a indirizzare nodi dentro `sidecars`,
  `header.*`, `footer.*` (l'albero che il renderer cammina include
  tutte queste zone).

Persistenza:

- `activity_bar.storage_key` → mirrora `active_sidecar` in
  `localStorage[storage_key]`. Plugin non lo legge — è chrome-only.
- `view_mode` (se il plugin lo modella come un `tabs` dentro
  `header.centre`) ha già il suo `tabs.persist_key` (esiste su `tabs`
  FormNode? — *open question Q4*).

### Backwards compat

| Caso | Comportamento |
|------|---------------|
| `header` assente | Render attuale: `<ModalHeader>` con `plugin_name` + `title`. |
| `activity_bar` assente | Nessun rail, body a piena larghezza. |
| `sidecars` assente | Nessun pannello a destra. |
| `footer` assente | Submit/Cancel/wizard come oggi. |
| `state_block` assente, `loading` set | Equivalente a `state_block = { loading = { label } }`. |
| `sidebar = true` + nuove API | Funziona, ma `sidebar` ha priorità sui `nodes` e i bordi (`header`/`footer`) si applicano al modale "around" il `tree_layout` implicito. |
| `tree_layout` FormNode dentro `nodes` | Rimane: è un widget di body, ortogonale al `activity_bar`. Un plugin può combinare i due. |

### Esempio completo (concept)

```lua
arbor.ui.form{
  title = "Properties Studio",
  width = "min(1480px, 97vw)",
  height = "min(960px, 94vh)",
  submit_action = "save",
  hide_submit   = true,                            -- footer.right is custom

  header = {
    icon     = { lucide = "FileText" },
    subtitle = current_path,
    dirty    = is_dirty,
    centre = {
      F:tabs("view_mode", {
        items = {
          { id = "tree",   label = "Tree",   icon = "ListTree" },
          { id = "text",   label = "Text",   icon = "FileText" },
          { id = "errors", label = "!",      icon = "AlertCircle", tone = "error" },
        },
        persist_key = "plug:propstudio:viewmode",
      }),
    },
    experimental = { description = "Iterating in alpha — schema rules may shift." },
  },

  activity_bar = {
    side = "right",
    items = {
      { id = "inspector", icon = "ScanSearch", label = "Inspector" },
      { id = "query",     icon = "ListFilter", label = "Query", count = hit_count },
      { id = "bindings",  icon = "Layers",     label = "Bindings" },
      { id = "schema",    icon = "BookOpen",   label = "Schema" },
      { separator = true },
      { id = "tools",     icon = "Wrench",     label = "Tools" },
    },
    default     = "inspector",
    storage_key = "plug:propstudio:rightpane",
  },

  sidecars = {
    inspector = { width = 320, title = "Inspector", children = build_inspector() },
    query     = { width = 360, title = "Query results", children = build_query()   },
    bindings  = { width = 320, title = "Bindings & broken refs", children = build_bindings() },
    schema    = { width = 360, title = "Schema",    children = build_schema()  },
    tools     = { width = 280, title = "Tools",     children = build_tools()   },
  },

  footer = {
    status = {
      F:state_block_pill({ tone = parse_tone, label = parse_label }),
      F:breadcrumb({ segments = selected_path }),
    },
    center = {
      F:button({ icon = "Undo2", action = "undo",  size = "xs", variant = "ghost" }),
      F:button({ icon = "Redo2", action = "redo",  size = "xs", variant = "ghost" }),
      F:button({ label = "Format",  action = "format",  size = "sm" }),
    },
    right = {
      F:button({ label = "Save",
                 icon = "Save",
                 action = "save",
                 variant = "primary",
                 size = "md" }),
    },
  },

  state_block = {
    loading = loading and { label = loading_label } or nil,
    error   = parse_error and { label = parse_error }    or nil,
  },

  -- Body = la view "tree" / "text" / "errors" decisa dal view-mode tab.
  nodes = build_body(view_mode),
}
```

## Touch-point implementativi (per il prossimo turno)

Lista non esaustiva — è la mappa di cosa toccare in PR-D:

1. **Types TS** [`src/lib/types/plugin.ts`](../src/lib/types/plugin.ts):
   estendere `PluginFormConfig` con `header`, `activity_bar`, `sidecars`,
   `footer`, `state_block`. Definire le sub-interface (`FormHeaderCfg`,
   `FormActivityBarItem`, `FormSidecarCfg`, `FormFooterCfg`,
   `FormStateBlockCfg`).
2. **Renderer** [`PluginFormModal.svelte`](../src/lib/components/plugins/PluginFormModal.svelte):
   sostituire la composizione attuale con uno scheletro che riproduce
   l'albero di `StudioModal.svelte`, popolato dalle nuove sotto-key e
   con fallback su `title`/`description`/`nodes` quando assenti. Il
   `bodyMain` resta `<FormNodeRenderer nodes={form.nodes} ...>`; le
   altre zone sono ognuna un `<FormNodeRenderer nodes={...} ...>` con
   `liveState` condiviso (vedi punto 4).
3. **Bridge Rust** [`crates/plugin/core/src/lua_api/ns/ui/form.rs`](../crates/plugin/core/src/lua_api/ns/ui/form.rs):
   pass-through opaco — le nuove sotto-key viaggiano in `payload[k]` già
   oggi. Aggiungere `arbor.ui.form.set_sidecar(id)` e
   `arbor.ui.form.set_state_block(name, cfg)` come thin wrapper su
   `set_state_path` + nuovo op `set_state_block`.
4. **liveState condiviso**: le value-bearing dei sidecar entrano nello
   stesso `values` map del body. Naming convention: pre-fissare gli `id`
   dei nodi sidecar quando si patcha; nessun namespace forzato (i plugin
   già scelgono `id` univoci).
5. **FormBuilder Lua** [`builders.lua`](../crates/plugin/core/src/lua_builtins/builders.lua):
   helper opzionale `FormBuilder:studio{...}` che chiama
   `arbor.ui.form{...}` con i campi raggruppati in modo più leggibile.
   Decisione aperta — vedi *Open questions Q5*.
6. **SDK Lua** [`sdk.d.lua`](../plugins/sdk.d.lua nel repo arbor-extensions):
   classi `arbor.FormHeaderCfg`, `arbor.FormActivityBar`,
   `arbor.FormActivityBarItem`, `arbor.FormSidecarCfg`,
   `arbor.FormFooterCfg`, `arbor.FormStateBlockCfg`.
7. **Docs** [`PluginDevApiUI.svelte`](../src/lib/components/shared/docs/PluginDevApiUI.svelte):
   nuova sezione "Studio-shaped modal" che mostra l'esempio completo e
   linka alle tabelle dei FormNode già documentati.
8. **Smoke test**: estendere il `properties-studio-lite` plugin a
   shape-equivalent con il modello vero (può rimanere `_lite` —
   l'obiettivo è verificare l'API surface, non duplicare PropertiesStudio).

## Decisioni (round 3) — risolte

Le 5 questioni sollevate dal doc sono state risolte dall'utente. Riassunte
qui per chi codifica D+E:

- **Q1 — Icona header**. **No SVG raw**. Forme accettate: `lucide`
  (nome icona Lucide), `brand` (id da `ProviderBrandName`), `image`
  (URL `file://` / `data:` / `https://`). Niente sanitize HTML host-side
  da mantenere; chi vuole un pittogramma custom passa per un PNG/SVG-file
  via `image`.
- **Q2 — Activity-bar items: routing-only**. Niente `action` / `dispatch`
  sugli items. Ogni `id` dichiarato in `activity_bar.items` DEVE avere
  un'entry omonima in `sidecars`. Pulsanti "azione" (Open file…, Save
  As…, Settings…) vanno in `header.left` / `header.right` con un `button`
  FormNode normale. Semplifica anche il modello mentale: il rail è un
  router di pannelli, mai un'azione.
- **Q3 — Stateless**. Nessun hook nuovo. Il plugin patcha
  `sidecars.<id>.children` quando vuole; non riceve callback su switch.
  Se in futuro un plugin reale ha bisogno di lazy-load, si aggiungerà
  `on_form_sidecar_change` allora — non specularmente.
- **Q4 — `tabs.persist_key`: da aggiungere**. Verificato:
  `FormNodeTabs` ha `default_tab` (statico) ma non `persist_key`. È un
  add gratis, mirror del pattern già usato in `<Tabs>` Svelte (localStorage
  per-id). Parte del PR di D.
- **Q5 — `FormBuilder:studio` helper: rimandato**. Solo cfg-table piatta
  in D. Si rivaluta quando un plugin reale si lamenta della verbosità.

---

## Cosa cambia per chi codifica D+E

Le decisioni qui sopra restringono il PR:

- **`FormActivityBarItem`** ha 6 campi: `id`, `icon`, `label`,
  `tooltip?`, `count?`, `dot?`, `tone?`, `disabled?`. Niente
  `action` / `dispatch` / `pinned`. Più una variante `{ separator = true }`.
- **`FormHeaderIcon`** ha 3 varianti mutuamente esclusive:
  `{ lucide: string } | { brand: ProviderBrandName } | { image: string }`.
- **`sidecars` è map non-null da `id` → cfg**: il renderer deve loggare
  warning se `activity_bar.items[i].id` non ha un sidecar
  corrispondente.
- **`FormNodeTabs.persist_key`** aggiunto come field opzionale; quando
  set, mirrora l'`id` del tab attivo in `localStorage[persist_key]` —
  stessa semantica già implementata in `<Tabs>` shared (component-side
  controllo locale, plugin-side opt-in via il key).

Stima informale del PR D+E unificato: ~600-900 LOC TS + ~150 LOC Rust +
doc + SDK + smoke-test estesa nel `properties-studio-lite`. Realisticamente
un PR unico — separare D ed E significherebbe rilasciare due cambi
adiacenti a `PluginFormConfig` in due round e costringere il plugin demo
a un doppio refactor.
