# Bennu — Design & MVP Analysis

> **Prodotto:** Bennu — editor Java + motore semantico di dominio per stack legacy enterprise (Struts2 / JSP / Entando-jAPS / Spring-XML / JDBC-DAO), estensibile a MyBatis/Hibernate/Spring-Data.
> **Pattern:** nuovo prodotto Arbor, backend crate Rust standalone (pattern Corvus), FE consumer (Svelte 5 + CodeMirror 6 + shared chrome).
> **Stato:** design / pre-spike. **Data:** 2026-07-02.
> **Banco di prova #1:** `regione-marche/e-procurement-PortaleAppalti` (Entando/jAPS, Struts 2.1.6, Java 8, tag 4.8.0).

---

## 0. Cosa è cambiato dopo l'analisi del progetto reale

Il documento requisiti originale assumeva uno stack **MyBatis + Spring-Data + Hibernate**. Il progetto reale (e la classe di progetti che rappresenta) è **un'altra cosa**, e questo **riordina le priorità di dominio dell'MVP**:

| Assunzione doc originale | Realtà del progetto | Conseguenza |
|---|---|---|
| Persistenza MyBatis / Spring-Data / JPA | **JDBC-DAO puro**, SQL come stringhe-costante nei DAO. Zero mapper.xml, zero JPA, zero Spring-Data | `findByX`/`#{param}`/`resultMap` **non si applicano al primo cliente** → restano *capability* per "Bennu editor globale", ma **fuori dall'MVP** |
| `findByX` = feature a ROI più alto | Non c'è Spring-Data | ROI più alto si sposta su **grafo config Struts + Tiles + TLD custom** |
| Config Struts modellabile "invertendo l'indice" | **880 `<action>` su 69 frammenti XML** mergiati via `<include>` per classpath-name, **155 wildcard + 128 backref `{1}`**, **221 result Tiles** | XML è un **linguaggio di prima classe** da indicizzare; wildcard/Tiles vanno modellati, non ignorati |
| EL `${...}` come espressione primaria nelle view | **OGNL `%{...}` usato 7742×** vs `${}` 4827× | Il resolver di espressioni prioritario è **OGNL value-stack**, non JSP-EL |
| Generics dal bytecode = scelta di crate | **Nessun crate Rust decodifica l'attributo `Signature`** | Parser generics **homegrown** — voce di lavoro inevitabile (JVMS §4.7.9.1) |
| Encoding: default UTF-8 | Il progetto è **`Cp1252`** (dichiarato nel pom) | Encoding-detection dal pom è **critico**, non nice-to-have; UTF-8 è solo il fallback |

**Ribadito:** Bennu resta un **editor Java globale capability-based**. Entando/Struts è il primo banco di prova, non il data-model. MyBatis/Hibernate/Spring-Data entrano come *fonti/rilevatori* attivabili, in fase successiva.

---

## 1. Il riframe (invariato, confermato dai dati)

Il prodotto è **l'indice**, non l'editor né tree-sitter. Il moat è un **symbol index / grafo semantico cross-file** multi-fonte. Ogni feature "smart" è una **query sopra l'indice**. Direzione confermata: **editor** (non LSP-into-IntelliJ), con **CM6 + shared chrome di Corvus** per il guscio (→ il costo vero è il motore semantico, non la chrome). LSP come **client** solo *predisposto* (per Rust/rust-analyzer, post-MVP).

La memoria: IntelliJ sta a 2-3 GB per JVM + piattaforma generalista, non per "2 GB di AST". Bennu può stare **basso** con tre discipline non negoziabili:
1. **Indice `mmap`-ato su disco** (`fst` + `rkyv`), decode-on-demand con LRU — non tutto on-heap.
2. **Indicizza solo la fetta di dominio**, non l'universo Java.
3. **Zero JVM in-process** (bytecode reader nativo Rust).

---

## 2. Architettura — struttura crate finale

Nuovo prodotto Arbor sul pattern Corvus. Backend = più crate coesi (filosofia "free crate split" + `prelude` obbligatorio per ogni crate libreria). Layout proposto sotto `crates/products/bennu/`:

```
crates/products/bennu/
├─ be/            (bennu-be)        Shell Tauri/tarpc. SOLO glue: IPC, routing query → provider, spawn async.
├─ proto/         (bennu-proto)     Contratto IPC (query/response) tra BE e FE. Tipi condivisi.
├─ project/       (bennu-project)   Modello progetto/workspace: parse pom Maven, moduli, classpath deps
│                                   (mvn dependency:build-classpath cached), encoding-detection, selezione JDK
│                                   per-progetto, file-watch, orchestrazione (re)indicizzazione incrementale.
├─ index/         (bennu-index)     ★ IL PRODOTTO. Schema simboli multi-fonte + store mmap (fst→offset, rkyv blob)
│                                   + motore query (completion/refs/goto/diagnostics/symbols). Format-version header.
├─ classpath/     (bennu-classpath) Bytecode: reader .class (cafebabe) + parser generics Signature (homegrown)
│                                   + trait ClassSource (dir / jar-zip / jimage) + discovery JDK (rt.jar vs modules).
├─ java/          (bennu-java)      Modello sorgenti Java: parse tree-sitter-java, estrazione simboli, scope
│                                   resolution, ★ type-inference locale (il pezzo hard, homegrown).
├─ jsp/           (bennu-jsp)       Parse JSP (grammatica nostra), modello TLD/taglib, AST EL/OGNL (resolver → fase 2).
├─ web/           (bennu-web)       Grafo config web: Struts2/XWork (struts.xml + include-graph per classpath,
│                                   espansione wildcard, validation.xml), Tiles, Spring-bean-XML (id→impl).
│                                   Entando-aware (merge frammenti).
└─ intel/         (bennu-intel)     Seam "code-intel provider": trait IntelProvider. Impl nativa-Java (→ index).
                                    Slot impl LSP-client PREDISPOSTO (rust-analyzer, non wired nell'MVP).
```

**FE** (per-prodotto, come da regola layout Arbor):
```
src/lib/components/bennu/   componenti prodotto (editor window, panels: project-tree, problems, outline, ...)
src/lib/{stores,ipc,types}/bennu/
src/lib/components/shared/ui/CodeEditor.svelte   ← editor astratto riusabile (CM6 + binding a IntelProvider),
                                                    parametrizzato per linguaggio. Consumato da Bennu e (poi) Merula.
```

Ogni crate libreria: `src/lib.rs` con `pub mod prelude;` + `src/prelude.rs` + `README.md` allineato (regole CLAUDE.md). `bennu-be` è il guscio, non ha prelude.

**Seam provider (il cuore dell'astrazione):**
```
FE (CodeEditor.svelte) ──tarpc──> bennu-be ──> IntelProvider
                                                 ├─ NativeJavaProvider  (index-backed)  [MVP]
                                                 └─ LspClientProvider   (rust-analyzer)  [predisposto, no impl MVP]
```
L'FE parla un solo protocollo per tutti i linguaggi; Java va al motore nativo (ricco, custom, veloce, no JSON-RPC), Rust andrà a rust-analyzer via LSP-client. Questo è il "prestabilisci LSP" richiesto.

---

## 3. Schema indice multi-fonte

**Principio:** ogni record porta un tag `source`. Aggiungere una fonte (es. Maven `.m2`) = nuova variante che alimenta la stessa tabella, **non** un rewrite.

```
Symbol {
  id, kind (Class|Interface|Enum|Record|Method|Field|Param|LocalVar|Package),
  simple_name, fqn, owner_id,
  source: ProjectSource | JdkBytecode | TargetClasses | DepBytecode
        | StrutsAction | TldTag | SpringBean | ...,      ← multi-fonte
  signature (generics, dal parser Signature),
  modifiers, visibility,
  loc: FileSpan { file, byte_range } | ClassRef { jar/jimage, class_name }
}

Relation (edge) {
  from_id, to_id, kind:
    Extends | Implements | Overrides | References
    | ActionToClass | ActionToResult | ResultToView(tiles/jsp) | JspInclude
    | JspUsesTaglib | BeanIdToImpl | PropertyPath(later) | ...
}
```

**Storage (lean, low-RAM):**
- **`fst::Map`** (name/fqn → `u64` offset). Più mappe per lookup diversi: per-simple-name (completion + fuzzy/prefix gratis), per-fqn (goto), per-file (outline). `fst` dà prefix/Levenshtein per l'autocomplete senza costo extra.
- **Blob `rkyv`** mmap'd (offset → record archiviato), decode zero-copy on-demand, `bytecheck` on (mmap corrotto = UB altrimenti).
- **Header format-version** → mismatch = rebuild (ricostruire da sorgenti/jar è cheap ed è la strategia di evoluzione più sicura; `rkyv` 0.8.x non ha migration).
- **Partizione per-progetto** (workspace = N partizioni) in cache globale `%appdata%/bennu/index/<hash-path>` (non sporca il repo).
- **Fonti immutabili** (JDK, jar deps, target/classes) cache-ate per (mtime jar) → rebuild solo se jar/pom cambiano. **Fonti mutabili** (sorgenti) = set simboli per-file, patchati incrementalmente all'edit.

**Async / incrementale:** build su **blocking pool** (`spawn_blocking`, mai su worker tokio — gotcha nota: il reverse-channel starverebbe i worker). Query servite da ciò che è pronto; banner sottile "indexing… risultati parziali", niente spinner invadenti. Query hot-path < 50ms (indice caldo in RAM anche se persistito).

---

## 4. Shopping-list crate — **richiede approvazione (hard rule 7)**

Verificati live (crates.io/docs.rs/GitHub, 2026-07-02). Verdetto onesto su manutenzione/rischio.

| Crate | Scopo | Ultima release | Lic. | Verdetto |
|---|---|---|---|---|
| **cafebabe** | Read metadati `.class` | 0.9.0 / 2025-06 | 0BSD | **Adotta** (primario) |
| **zip** | Aprire rt.jar / jar (JDK 8) | corrente | MIT/Apache | **Adotta** |
| **jimage-rs** | Reader jimage puro-Rust (JDK 9+) | 0.0.4 / 2025-10 | MIT | **Adotta (rischio)** — 0.0.x mono-maintainer → **vendorizzare** |
| **tree-sitter-java** | Grammatica Java | 0.23.5 / 2024-12 | MIT | **Adotta** |
| **tree-sitter-html** | Backbone HTML per injection JSP | corrente | MIT | **Adotta** |
| **fst** | Indice name→offset (mmap) | 0.4.7 (stabile) | Unlic/MIT | **Adotta** |
| **rkyv** | Blob record zero-copy mmap | 0.8.16 / 2026-04 | MIT | **Adotta** (pin + `bytecheck`) |
| tree-sitter-rust / tower-lsp | (predisposizione LSP, post-MVP) | — | — | **Rimandato** |
| winnow | Parser EL/OGNL/HQL (fase 2) | 1.0.3 / 2026-05 | MIT | **Rimandato** (o recursive-descent a mano) |

**Fallback documentati (dietro lo stesso trait `ClassSource`, se qualcosa stalla):**
- **ristretto_classfile + ristretto_classloader** (0.31.0 / 2026-05, Apache/MIT, mantenuto fino al 2026): loader unificato dir/jar/jmod/**jimage** su tutte le versioni JDK. Copre da solo §sourcing. Più basso livello (risolvi tu gli indici constant-pool), più pesante. **Alternativa a `cafebabe + jimage-rs`** se preferisci un solo stack mantenuto al posto del rischio jimage-rs.
- **redb** (4.1.0 / 2026-04, puro-Rust, ACID) al posto di `fst+rkyv` se preferisci un unico store transazionale invece di due layer a mano (costo: niente zero-copy, un filo più RAM).

**Rischi da mettere in chiaro:**
- ⚠️ **`jimage-rs` è l'unico adopt genuinamente rischioso** (0.0.4). Mitigazione: vendorizzarlo (è piccolo) dietro `ClassSource`, o passare a `ristretto_classloader`.
- ⚠️ **`rkyv` è pre-1.0** con storia di breaking minori + zero migration → pin di versione + header + rebuild-on-mismatch.
- ⚠️ **Nessun crate decodifica i generics** (`Signature`) → **parser homegrown** obbligatorio (JVMS §4.7.9.1). Non è una scelta di libreria, è una voce di lavoro.
- ⚠️ **Grammatiche JSP community morte** (merico-dev 3★, QthCN 0★) → **grammatica JSP nostra** (piccola: directive/scriptlet/expression/EL + injection Java+HTML). Da possedere, non dipendere.

**Decisione richiesta:** (A) stack **cafebabe + zip + jimage-rs(vendor)** [più lean, un rischio 0.0.x], oppure (B) stack **ristretto** [un solo stack mantenuto, più pesante/verboso]. Raccomando **A** con `ClassSource` che tiene aperta la porta a B.

---

## 5. Feature MVP (la tua lista) — triage per fase, dipendenza, rischio

Legenda costo: **○ chrome/quasi-gratis** (CM6+Corvus) · **◐ config/bytecode-index** · **● inference-deep** (il rischio).

| # | Feature | Fase | Costo | Note |
|---|---|---|---|---|
| 1 | Highlight Java | 0 | ○ | tree-sitter-java |
| 2 | Linting (sintattico) | 0 | ○ | errori da tree-sitter; lint semantico (simbolo irrisolto) → fase 1-2 |
| 3 | Error-tolerant | 0 | ○ | nativo tree-sitter |
| 4 | **Autocomplete member-access** (`.`) | 1 | ● | index + bytecode + **generics parser** + inference locale. Lazy: decode-on-demand, filtra membri privati |
| 5 | Highlight JSP | 0 | ◐ | **grammatica JSP nostra** |
| 6 | Go-to-line/column | 0 | ○ | CM6 |
| 7 | Find-usages metodo (Ctrl+F12) | 2 | ● | indice riferimenti; cross-file richiede resolution |
| 8 | Go-to-decl / implementations | 1→2 | ◐→● | same-project (1); cross-file + interfaccia→impl via type-hierarchy (2) |
| 9 | Find file/class/everywhere + replace | 0→1 | ○→◐ | file/testo (0); class/"everywhere" = symbol index (1) |
| 10 | **Refactor rename classe** | 3 | ● | cross-file + preview. ⚠️ in Entando deve aggiornare anche i `class="..."` in struts.xml → rename **domain-aware** |
| 11 | Refactor rename metodo | 3 | ● | overload resolution + preview |
| 12 | Refactor rename variabile | 1 | ◐ | scope-locale, facile |
| 13 | Generate getter/setter (fluent+no) | 1 | ◐ | codegen da AST, no cross-file |
| 14 | Generate constructor | 1 | ◐ | idem |
| 15 | Split-view 2-pane tabbed | 0 | ○ | FE/CM6 |
| 16 | Albero progetto | 0 | ○ | Tree Corvus |
| 17 | Analisi dependency (collisioni + outdated) | 3 | ◐ | riuso plugin esistente + **flag CVE** (vedi §6: Struts 2.1.6 ha RCE note) |
| 18 | Terminale | 0 | ○ | copia da Corvus |
| 19 | Build & run | 0/3 | ◐ | shell mvn/javac → diagnostics; run = java+classpath. Bonus: produce `target/classes` → alimenta l'indice |
| 20 | **Auto-format** | 3 | ⚠️ | **scope-risk** — nessun formatter Java buono in Rust. Opzioni: (a) normalizzatore whitespace tree-sitter-based [spaces configurabili, MVP], (b) bundle google-java-format [serve JVM, contro il lean], (c) esterno opzionale. Raccomando (a) per MVP, (b/c) later |
| 21 | Encoding-detection (pom / default UTF-8 / override x-progetto e x-file) | 0-1 | ◐ | reale = `Cp1252`. Legge `project.build.sourceEncoding`, fallback UTF-8, override footer stile IntelliJ |
| 22 | JDK per-progetto + auto-detect maven/pom | 1 | ◐ | `maven.compiler.*`, compiler-plugin, `<toolchains>` + override. Multi-JDK (rt.jar 8 / jimage 9+) |
| — | Plugin Emmet (JSP/HTML) | later | ○ | estensione CM6 |
| — | Plugin Lombok | 1-2 | ◐ | quasi-gratis se si indicizza `target/classes` (post-compile ha già i membri generati); alternativa: synthesizer annotation-aware in bennu-java |

### Linea di taglio v1 difesa
**MVP = Fase 0 + Fase 1 + fetta di Fase 2** (grafo Struts/JSP config-index, il valore Entando). Fase 3 (refactor pesanti, autoformat, dependency-analysis) subito dopo. **Fuori v1:** OGNL/EL resolver value-stack (●), MyBatis/Hibernate/Spring-Data (altri progetti), rust-analyzer, bytecode `.m2` per autocomplete deps.

---

## 6. Piano a fasi

**Fase 0 — Scaffold** *(rischio basso, pattern editor già fatto)*
Prodotto Bennu (skeleton crate + finestra `+page.svelte` + tile launcher). FE shell: CM6 + shared chrome Corvus (tabs, split 2-pane, tree progetto, terminale, find/replace, go-to-line). tree-sitter-java: highlight + folding + error-tolerant. Grammatica JSP nostra (highlight JSP). Apertura file con **encoding-detection**. Nessuna semantica cross-file ancora.

**Fase 1 — Indice + bytecode + member-access autocomplete** *(il cuore)*
Schema indice (`fst`+`rkyv`, multi-fonte). `ClassSource` (dir/jar/jimage) + discovery JDK + **JDK per-progetto da pom**. Reader `cafebabe` + **parser generics Signature**. Indicizza: sorgenti (tree-sitter) + JDK + `target/classes`. **Autocomplete `.`**. **Type-inference locale** (spike). Persistenza + incrementale + async. Generate getter/setter/constructor, rename variabile locale.

**Fase 2 — Dominio Struts/JSP (config-index, valore Entando)**
`bennu-web`: modello struts.xml + **include-graph per classpath** + **espansione wildcard** (nav a *candidati*, marcati come inferiti) + action→class→method. **Tiles resolver** (result→def→JSP). **action↔view nav**. Modello **TLD/taglib** + autocomplete/validazione tag custom (`wp`/`wpsf`/`wpsa`/`es`). **Spring bean-id→impl**. **"Action inesistente" conservativa** (falsi positivi = veleno, vedi §7 wildcard). find-usages, go-to-decl/impl cross-file.

**Fase 3 — Refactor + generate + polish**
Rename metodo/classe (preview, domain-aware sui `class=` XML). Auto-format (whitespace tree-sitter). **Dependency analysis** (collisioni + outdated + **CVE flag**). Build/run migliorato.

**Later (post-MVP):** OGNL/EL resolver value-stack · MyBatis/Hibernate/Spring-Data capability · `findByX`/`#{param}`/HQL · rust-analyzer via LSP-client · bytecode `.m2`.

---

## 7. Spike da fare (posso eseguirli io sul progetto clonato — nessun input tuo)

Il progetto pubblico clonato in `disposable-projects\PortaleAppalti` è rappresentativo → gli spike girano su codice vero senza toccare il tuo privato.

**Spike A — Bytecode generics** *(~mezza giornata, confidence 6/10 → da sciogliere)*
`cafebabe` + parser Signature nostro su **jimage JDK21** (presente) e su un **rt.jar JDK8** (ne scarico uno in disposable). Deliverable: un `main` che stampa i membri di `java.util.Optional` / `List` / `Stream` **con i generics risolti**. Se i generics escono puliti → l'incognita più grossa del bytecode è chiusa.

**Spike B — Type-inference homegrown** *(~mezza giornata, confidence 5/10 → l'incognita vera)*
5 casi reali estratti da PortaleAppalti: (1) `%{...}` OGNL contro i getter di un'action, (2) `service.getX()` dove `service` è un bean Spring-XML, (3) catena getter in un DAO, (4) `Abstract*Action` con tipo ereditato, (5) local var tipata + chiamata a metodo. Misura: **quanti risolvo dai soli sorgenti progetto** vs **quanti richiedono di entrare in un jar**. Il rapporto dice se homegrown regge o se serve fallback.

---

## 8. Valutazione complessiva del portale (banco di prova)

Read-only su tag `4.8.0`, 3586 file. Assessment da senior.

**Cifre:** 1245 `.java` (~252k LOC), 612 `.jsp` (~61k LOC), 196 `.xml` (~20k), 8 `.tld`, ~85 dep dirette. Framework **jAPS/Entando vendorizzato come sorgente** nel tree (`com.agiletec.aps.*`) — insolito, ma *fortunato per un analizzatore* (config e TLD su disco, non nei jar).

**Architettura.** 3-tier jAPS riconoscibile (Action → Manager/Service → DAO) con DI Spring-by-XML. Ma **separazione responsabilità debole nell'app layer**: le action `ppgare` mescolano stato-wizard HTTP, regole di business, generazione PDF/report e orchestrazione SQL. Vendorizzare il framework nel sorgente gonfia l'albero e sfuma il confine app/framework.

**Code smells (gravi).** God object marcati: `BandiManager.java` **137 KB**, cinque `*Action`/`*Wizard` > 100 KB, `InterceptorEncodedData` **123 KB**. Gerarchia wizard `AbstractOpenPage*`/`AbstractProcessPage*` con molte famiglie 2-3 membri quasi-duplicate → forte segnale di duplicazione. Magic string ovunque (nomi action/result, session key, path OGNL).

**Test: praticamente zero.** 0 `*Test.java`, nessun `src/test`. Nessuna copertura.

**Sicurezza — bandiere rosse.** (1) ~49 siti di **concatenazione SQL** (su 189 PreparedStatement: il grosso è parametrizzato, ma quei siti sono target di review SQLi). (2) Gestione XSS **in-app** custom (`XSSRequestFilter`, `XSSParameterInterceptor`, filtri DNS/Bot) invece di libreria vettata → da auditare. (3) **Dipendenze EOL con CVE critiche note**: **Struts 2.1.6** (2008, linea con multipli RCE S2-*), **Spring 2.5.3/2.0.8**, **Axis 1.4**, **Lucene 2.4.1**, **iText 2.1.7**, Groovy 2.4. log4j bumpato a 2.17.2 (post-Log4Shell) → patching reattivo, ma il core framework è congelato. (4) Encoding **`Cp1252`** = footgun sottile su input handling. (5) ~6 literal `password="..."` in Java da triage.

**Gestione JSP.** Media. 42% dei JSP con scriptlet, OGNL usato 7742× (logica-in-view reale); ma JSTL presente e include **dinamici** (`jsp:include` 1943×) invece che scriptlet-driven. `c.tld`/`fmt.tld` **copie locali** che shadowano gli URI JSTL standard → un analizzatore JSP deve caricare i TLD project-local per URI-web e riconciliare gli URI standard shadowati.

**Action / XML.** 478 classi `*Action`, **880 `<action>`** su **69 frammenti** mergiati via 64 `<include>` per classpath-name. **Zero convention-plugin** (100% XML esplicito). **155 wildcard + 128 backref** → route e persino nomi-metodo sintetizzati dall'URL. **221 result Tiles** + 5 tiles.xml. **Dual front-controller**: Entando `ControllerServlet` (`*.wp`, `/pages/*`) per il sito CMS + Struts `/do/*`. 43 bean-XML Spring wire per string-id. XML fragmentato ma disciplinato (dominio-per-file) — il costo è **indirezione**: capire un'action = struts.xml → include → frammento → base action → interceptor stack → tiles def → JSP.

**Validazioni.** Doppie: **63 `*-validation.xml`** XWork **e** 63 `validate()` Java, più interceptor custom (`ActionFieldValidationInterceptor`, `XSSParameterInterceptor`). Sparse su XML + Java + interceptor.

**Dependency health.** ~85 dirette, decine di blocchi `<exclusion>` a districare collisioni transitive (commons-*, jackson, jersey, bouncycastle, spring-beans esclusi ripetutamente). Alcune dep marcate "non su nexus/central" → build dipende da **repo privato**, non riproducibile da Maven pubblico.

### Cosa è genuinamente DIFFICILE per un analizzatore statico (→ dove degradare)
1. **Wildcard mapping** (155 + 128 backref): mappa action→classe→metodo→result computata dall'URL → nav a **candidati**, non target singolo esatto, marcata "inferita".
2. **Indirezione Tiles** (221 result + 5 def): result→def→JSP è lookup a due salti fuori dal frammento struts.
3. **Merge runtime Entando**: config effettiva = unione di ~69 frammenti pescati per classpath-name; su install non-vendorizzato arrivano dai jar → serve un **resource-index classpath-aware**.
4. **OGNL value-stack** (7742 `%{...}`): property path reflective contro l'action/model in cima allo stack → resolver **best-effort**, mai assunto completo.
5. **View composition dinamica**: 1943 `jsp:include` con `page=` computato + 7558 tag `wp:` showlet → grafo view assemblato a runtime da config DB-stored, non da include statici. Rappresentare gli include computati come **edge-con-espressione irrisolta**, non droppare.
6. **DI Spring-by-name** (43 bean-XML): call-site → impl richiede risoluzione XML (indice separato dalla type-hierarchy Java).
7. **God file** 100-137 KB: outlining/index/parse incrementale devono restare responsivi su file >130 KB — reparse-intero-a-keystroke stalla.

### Top lezioni per il tool (evidence-based)
1. **L'XML *è* il source-of-truth del control-flow** → indicizzalo come linguaggio di prima classe (0 annotazioni, 880 mapping XML).
2. **Modella l'espansione wildcard e degrada con grazia** (nav a candidati marcati).
3. **Risolvi config che vive nei jar**, non solo nel workspace (assunzione di design, anche se qui è vendorizzata).
4. **Tiles + `wp:`/showlet richiedono un resolver dedicato** — "go to view" da action è la cosa che gli sviluppatori vogliono di più.
5. **OGNL/EL = resolver parziale, mai completo** — la falsa-confidenza qui è peggio del nulla.
6. **Include dinamici** rappresentati onestamente come irrisolti-con-espressione.
7. **God file** → parse incrementale responsivo obbligatorio.
8. **JSP scriptlet+taglib-misto è la norma** → carica TLD project-local per URI-web, riconcilia URI JSTL shadowati.
9. **DI Spring-XML-by-name** → indice bean-graph separato.
10. **Encoding, config commentata, dep EOL = segnali di prima classe** → leggi nell'encoding dichiarato (non UTF-8), non indicizzare XML/JSP commentati come config viva, **flagga coordinate dep vulnerabili**, e "simbolo irrisolto da jar mancante" deve essere uno **stato normale non-fatale** (build dipende da repo privato).

---

## 9. Decisioni aperte per te
1. **Approvazione shopping-list crate** (§4) — hard rule 7. In blocco o voce-per-voce.
2. **Stack bytecode**: (A) cafebabe + jimage-rs(vendor) [lean, 1 rischio] vs (B) ristretto [1 stack mantenuto, pesante]. Raccomando A.
3. **Auto-format** (§5 #20): normalizzatore whitespace tree-sitter per MVP (raccomandato) vs bundle google-java-format (serve JVM).
4. **Ordine**: parto dagli **spike A+B** (li eseguo io sul progetto clonato) prima dello scaffold? Raccomando sì — sciolgono le due incognite prima di fissare le stime.

---

## 10. Spike findings (round 1) — 2026-07-02

Quattro spike eseguiti con **crate Rust reali compilati e girati** sui JDK veri (8/21) e sul progetto clonato. **Tutti SUCCESS.**

### Confidence aggiornata

| Claim | Prima | Dopo | Nota |
|---|---|---|---|
| Bytecode generics (estrazione JVM-free dei generics JDK) | 6/10 | **9/10** | Tutti i target (`Optional.map`, `List.iterator`, `Map.entrySet`, `Stream.collect` + class signature) decodificati con generics/wildcard/bound/nesting da **rt.jar (JDK8) E jimage (JDK21)**. `cafebabe`+`zip`+`jimage-rs` off-the-shelf + un decoder Signature homegrown di ~250 LOC (JVMS §4.7.9.1). Zero fallimenti. Non 10 solo perché `jimage-rs` è 0.0.x e abbiamo campionato ~5 classi di un modulo. |
| Type-inference homegrown (no jdtls) | 5/10 | **8/10** | Tutti i 5 casi member-access reali risolti. **Nessun caso** ha richiesto inference compiler-grade. Il lavoro portante è traversal del config-graph + member index bytecode — entrambi nativi al design homegrown. |
| Storage `fst`+`rkyv` (indice mmap) | — | **ADOPT** | Indice da **2M simboli = 251 MiB su disco** interrogato a **~15 MiB di working set**. Zero-copy validato, prefix + fuzzy (Levenshtein) funzionanti. Conferma il profilo low-RAM vs IntelliJ 2-3 GB. |

### Decisione cardine: **GO su inference homegrown** (con 2 caveat obbligatori)

Lo spike ha **invertito il modello di rischio**: la roba temuta (overload resolution per tipi-argomento, sostituzione generics, flow-typing) **non è mai comparsa** nei path reali. jdtls non serve. Il lavoro portante è altro, e il design homegrown lo possiede già:

- **C1 — Config-graph resolution è portante, non opzionale.** In Struts `<action class="beanId">` contiene un **bean-id Spring, non un FQCN**. Risolvere JSP→action per FQCN risolve *niente*. La catena `JSP OGNL → struts.xml → bean-id Spring → classe` (+ `ref=` DI per campi interface-typed) è load-bearing — e **jdtls non la fa affatto**, quindi va implementata comunque. Deve esistere *prima* di qualsiasi risoluzione JSP/action.
- **C2 — I generics sui return type vanno "portati attraverso".** `Map<Integer,String>`, `SearchResult<PagamentiOutType>`: il member-access sull'element type risolve solo se la sostituzione parametrica viene propagata. Meccanico, non inference — ma va implementato, non saltato.
- Corollario: **member index bytecode obbligatorio** — 2/5 casi entrano in stub generati (SOAP/XMLBeans, `it.eldasoft.*`) che vivono solo nei jar. Spike A prova che li leggiamo JVM-free.

### Aggiustamenti architetturali dai findings
- **`bennu-project` = crate leaf** che possiede la capability-detection (dipende solo dal modello progetto + `error`). Gli analyzer dipendono *da lui*, mai il contrario.
- **Estrazione bytecode = crate leaf a sé** (`cafebabe`+`zip`+`jimage-rs`+decoder Signature); i due formati container (rt.jar ZIP / jimage) dietro **una sola member-index API** (l'unica differenza è il path: `java/lang/String.class` vs `/java.base/java/lang/String.class`).
- **Indice = blob `rkyv` framed `[u32 len][bytes]` con allineamento a 16 byte per record** — unico rischio di produzione emerso da Spike C (misalignment = fallimento `bytecheck` rumoroso, non UB, ma va forzato nel writer). Chiavi fst ordinate+uniche, feature `levenshtein` on. Un fst + un blob per fonte.
- **Risoluzione config-graph-first**, type-walk nominale sopra. Il **capability bitset gate la *costruzione*** delle fonti/resolver (un progetto MyBatis-only non paga mai il grafo Struts, e viceversa), non solo l'enable a query-time. Union per-modulo nei multi-modulo.

### Capability-detection — ruleset (da Spike D)
Tier segnali: **A** = coordinata dipendenza pom (più forte) · **B** = presenza/path file config · **C** = annotation/package/import nel sorgente (corroborante). Una capability si attiva su **≥1 segnale forte (A o B)**; match **solo-C/transitivo** = attivazione *provvisoria* a bassa priorità (mai hard-fail).

| Capability | Forte (A/B) | Debole (C) |
|---|---|---|
| StrutsXmlConfig | `struts2-core`; `struts.xml`/`*-struts-plugin.xml` | FilterDispatcher; `<package>/<action>/<result>` |
| StrutsConvention | `struts2-convention-plugin` | `@Action`/`@Namespace`; classi `*Action` |
| JspTaglibTld | `*.tld` sotto `WEB-INF/**`; web.xml `<taglib>` | direttive `<%@ taglib %>` |
| OgnlValueStack | (segue StrutsXmlConfig) | `%{…}`/`${…}`; `*-validation.xml` |
| TilesViews | `struts2-tiles-plugin`/`tiles-*`; `tiles.xml` | listener Tiles; `result type="tiles"` |
| SpringXmlDi | `spring-beans`/`spring-context`/`spring-jdbc`; root XML `<beans>` | `ContextLoaderListener`; `getBean(...)` |
| SpringAnnotationDi | `spring-context` + `<context:component-scan>` | `@Component/@Service/@Autowired` |
| SpringDataRepo | `spring-data-*` | `extends JpaRepository/CrudRepository` |
| JpaHibernate | `hibernate-core`/`persistence.xml`/`*.hbm.xml` | `@Entity/@Table/@Id`; `EntityManager` |
| MyBatisMapper | `mybatis`/`mybatis-spring`; `*Mapper.xml`/`sqlMapConfig.xml` | `@Mapper/@Select`; `SqlSession` |
| JdbcDao | `spring-jdbc`/`commons-dbcp`/driver JDBC + ≥1 hit sorgente | `JdbcTemplate`/`java.sql`/`AbstractDAO` |
| Lombok | `org.projectlombok:lombok` | `import lombok.*`; `@Data/@Getter/@Builder` |
| EntandoJaps | dep Entando/jAPS (`org.entando.*`/`com.agiletec.*`); `*japs-struts-plugin.xml`; `aps-core.tld` | showlet `<wp:*>`; `ControllerServlet`/`*.wp` |

**Progetto di riferimento → profilo Struts/Entando/JDBC**, con MyBatis/Spring-Data/JPA/Lombok/DI-ad-annotazioni **provabilmente OFF** (0 hit su grep in 1245 `.java`). Convalida il modello capability-based.

### Next action
**GO Fase 0 (scaffold).** Tutte le incognite pre-implementazione sono sciolte; nessun secondo round di spike necessario.

---

## 11. Fase 1 — implementazione BE (round 1) — 2026-07-02

Member-access autocomplete **reale, end-to-end**, sviluppato e provato in `disposable` (compilato+testato) su JDK 8 reale + PortaleAppalti, poi portato nei crate arbor.

| Crate | Esito |
|---|---|
| `bennu-classpath` | `resolve_jdk_classpath` (rt.jar 8 / jimage 21) + member index bytecode con **generics carry-through** dal Signature. 15 test verdi. (3 fix di build applicati durante il porting.) |
| `bennu-java` | `extract_symbols` + `infer_receiver_type` dietro trait `TypeResolver`. 12/12 test; **397/400 file legacy → tipo, 0 panic**. tree-sitter **0.25** (allineato a Merula), walking manuale (niente Query API → 0.25-safe). |
| `bennu-index` | `IndexBuilder` + `PersistedIndex` su fst+rkyv, grouping per-file → patch/delete incrementale. FORMAT_VERSION **1→2** (rebuild on open by design). |
| `bennu-intel` | `NativeJavaProvider` reale: infer ricevente → walk super/interfacce → prefix-filter. Un `unsafe impl Send+Sync` documentato (JarSource JDK8 `!Sync` in `Mutex`). |
| `bennu-be` + `project` | `IndexService` su **thread di background** (mai blocca l'IPC), empty provider durante il build → **hot-swap**. Persist in `bennu_data_dir()/index/<hash>/`. |

**Demo end-to-end:** indice su **1128 tipi / 19610 membri in ~2s**, query **<50ms** (warm ~150–470µs). JDK con generics (`List<Customer>.get(0).` → membri di `Customer`; `String.to` → `toCharArray/toLowerCase(Locale)/…`) + tipi di progetto (`Order.getCustomer().` cammina la catena getter); su legacy vero `RequestContext.get` → `getRequest():HttpServletRequest`, …; patch incrementale 1.9ms.

**Confidence inference (implementata): 8→8.5/10.**

**Limiti Fase 1 (bounded, non sorprese):** no overload-resolution-by-arg (primo per nome vince), no flow-typing (reassign/ternary/instanceof), no static su nome-tipo (`Integer.MAX_VALUE`, `Collections.emptyList()`), no array-element (`arr[i].`), generics shallow (E/T/K/V one-hop; V→2° type-arg per Map euristico), **dep `.m2` fuori scope** (solo JDK bootclasspath + sorgenti), JDK discovery **Windows-only**, live-edit re-index cablato (`patch_file`) ma **non triggato** (manca hook `bennu_did_change`/save → l'indice è corretto all'apertura, gli edit si riflettono a riapertura).

**Gap FE per rendere VISIBILE l'autocomplete:** il BE serve `bennu_completion`, ma l'editor CM ha ancora la completion come stub — va agganciata una completion-source (hook `intel` del `CodeEditor` → `ipc/bennu completion`) al descriptor Java. Piccolo follow-up FE.

**Fase 2 (config-graph, additivo sull'indice, niente rewrite):** Struts `<action>`→classe (`StrutsAction`/`ActionToClass`), Spring `bean-id`→classe (`SpringBean`), Tiles result→JSP (`ResultToView`), JSP/OGNL/TLD contro l'indice tipi di Fase 1. Pull-forward dal backlog: `.m2` dep-jar sourcing + hook live-edit re-index.

---

## 12. Fase 2 — BE config-graph (round 1) — 2026-07-02

Grafo config Struts/Spring/Tiles + sourcing dep-jar `.m2`, **additivo sull'indice di Fase 1** (nessun rewrite), sviluppato e provato in `disposable` (compilato+testato) sul PortaleAppalti reale, poi portato nei crate arbor. Ogni pezzo è stato validato con un **port-check harness**: la sorgente arbor *verbatim* compila e i suoi unit test passano contro la shape reale del seam.

| Crate | Esito |
|---|---|
| `bennu-web` **(new)** | Parser config-graph: Struts (`struts.xml` + include-graph per classpath, wildcard+backref), Spring bean-XML (`id`→FQCN), Tiles (def→JSP). `build_web_graph` + catene `resolve_action_class` (C1) / `resolve_action_view`. Record string-keyed, `RelKind::into_index()` sul seam. **10 test verdi**, zero warning. `roxmltree 0.20` (approvato). |
| `bennu-classpath` | Aggiunto sourcing **dep-jar `.m2`**: `resolve_maven_classpath` (`mvn dependency:build-classpath` offline+cached su pom-mtime), `MavenClasspath::augment(jdk)` — dep dietro al bootclasspath JDK (core reale vince su copie shaded), stessa API `ClassSource`/`MultiSource`/`members_of` (nessun cambio di shape). **7 test verdi**. `zip` già presente, niente nuove lib. |
| `bennu-index` | `Relation::inferred` (edge candidati wildcard) + `RelationKind: Copy`; nuovo `RelationWriter`/`RelationReader` (edge-store config keyed by `from_id`, run-per-nodo). FORMAT_VERSION **2→3** (rebuild on open). Round-trip test inline. |
| `bennu-intel` | `ingest_config_graph` (assegna id, scrive symbol+relation store) → `ConfigResolver`: `resolve_action_class` (C1), `resolve_action_view`, `diagnose_action` (`ActionVerdict::{Exists,Missing,Inconclusive}`), `action_class_ref` (go-to-def). `IntelProvider` invariato (aggiunte additive). |
| `bennu-be` + `project` | `web_discovery` (walk progetto → `WebInputs`), `ConfigResolver` nello `ProjectSlot` (build off-thread all'apertura), `patch_file` XML-aware (edit config → rebuild grafo, edit Java → re-index Java). IPC: `bennu_definition`, `bennu_diagnostics` arricchito, `bennu_did_change` (re-index live, handler sync su `spawn_blocking`). |

**action→classe (catena C1) + action→view — funziona end-to-end.** Sul progetto live (901 action / 566 bean / 97 tiles-def / 4594 edge parsati, 1455 edge risolvibili ingeriti):
- **action→classe: 734/739** classi di action concrete risolte via mappa Spring. Spot-check `/do/Category/viewTree → categoryAction → com.agiletec.apsadmin.category.CategoryAction`.
- **action→view: 214** view via result→Tiles-def→JSP. Spot-check `/do/Category/viewTree → /WEB-INF/apsadmin/jsp/category/categoryTree.jsp`.
- Conteggi allineati al design doc (901/566/97 vs 880/155). 898 `ActionToClass` + 562 `BeanIdToImpl` + 1460 `ResultToView`. 1 include irrisolto (`contentModel.xml`, jar-resident) riportato **non-fatale**.

**Diagnostica "action inesistente" — resta CONSERVATIVA.** Sul progetto reale: **742 exact-resolved / 159 wildcard-inconclusive / 3 genuinely-missing**, e **0 false-missing su wildcard**. Solo un Missing genuino produce un `warning`; candidati wildcard/OGNL (`portal*`, value-stack) → `Inconclusive`, nessun rumore. Un riferimento bogus → `Missing`.

**Completamento tipi da dipendenza `.m2` — funziona.** **177 jar risolti, 0 unresolved** (questo `~/.m2` era completo; offline usa solo la cache), **1 jar open-failure** (il crate `zip` rifiuta un archivio che .NET accetta — skippato, non-fatale). Resolve a freddo ~8–22s, **cache-hit 0.0001s**. Membri reali da dep: `HttpServletRequest` → 25 metodi (`getHeader`, …), XWork `ActionSupport` → 41, commons-lang `StringUtils` → 177, Spring `ApplicationContext`, commons-io `IOUtils` → 141.

**Nuova superficie IPC (per il team FE da consumare)** — convenzione `{ args: {…} }` esistente, nessun wire-name cambiato:
- `bennu_definition { file, action }` → `Option<{ config_file, class_fqcn?, view_jsp? }>` — go-to-def di un riferimento action da JSP (frammento config + FQCN classe + JSP view).
- `bennu_diagnostics { file, actions?: [{ qualified_name, start, end }] }` → `[Diagnostic]` — action-existence conservativa (arg `actions` opzionale, retrocompatibile: solo Missing genuino → `warning`).
- `bennu_did_change { file, text? }` → `bool` — re-index live (file Java, XML config, o delete).

**Limiti Fase 2 (bounded, non sorprese):** **view-endpoint non ancora simboli** (target Tiles-def / `<action>#result` restano nel grafo parsato, non nel id-store → la catena view si risponde off-graph; id JSP/Tiles arrivano con l'ondata JSP). **Parsing JSP a carico del FE per ora** — `bennu_diagnostics`/`bennu_definition` prendono il riferimento action (+ byte-range) come argomento; estrarre `<s:form action=…>`/`<s:url>` dal buffer JSP è Fase 3. `bennu_did_change` ritorna `true` anche se nessun progetto possiede il file (patch no-op silenzioso — innocuo). **Include da jar di dipendenza** (install non-vendored) risolti solo da resource-root on-disk (riportati in `BuildReport.unresolved_includes`); **convention-plugin Struts** (`@Action`/`@Namespace`) fuori scope (progetto 100% XML esplicito); interceptor-stack, `validation.xml`, result-param oltre la view, TLD tag (`Source::TldTag` esiste ma il parsing TLD arriva con la taglib JSP) non modellati. Deps da repo privato su `~/.m2` freddo → `unresolved`, skippate (mai fatale). `mvn` su Windows è `mvn.cmd` (il port arbor prende il launcher da config).

**Cosa compili/validi in arbor:** i crate `bennu-web` (+`roxmltree 0.20`), `bennu-classpath`, `bennu-index`, `bennu-intel`, `bennu-be`. Per hard rule non è stato eseguito `cargo` nel repo arbor — la validità (compile + test + run) è provata copiando la sorgente arbor *verbatim* in due port-check harness disposable (`bennu-index-mirror` per `relations.rs`; `bennu-portcheck`+`bennu-web-mirror` per `bennu-intel::config` e `bennu-be::web_discovery`), entrambi verdi. Da validare a mano: build dei 5 crate + smoke dei 3 nuovi comandi IPC. Nota minore: `serde`/`serde_json` restano dichiarati in `bennu-web/Cargo.toml` (carryover skeleton, i record non derivano ancora `Serialize`) — tenuti per l'integrazione imminente, innocui.

**Fase 3 (prossima):** **risoluzione espressioni JSP/OGNL** — parse dei buffer JSP per estrarre riferimenti action + espressioni value-stack `%{…}`, `<s:property>`, `jsp:include page="%{…}"` dinamico, composizione view runtime `wp:`/showlet — così diagnostica/definition girano senza riferimenti forniti dal FE. Poi **`@Query` HQL** + **MyBatis mapper XML** per i progetti non-Entando (il capability-bitset già flagga `jpa_hibernate`/`mybatis_mapper`), **simboli TLD tag** (`Source::TldTag`) per la navigazione `JspUsesTaglib`, **convention-plugin Struts** per i progetti annotation-routed.

---
*Testa dritta.*
