# Bennu vs IntelliJ IDEA — analisi validazione, concorrenza, performance

Stato: analisi (2026-07-07). Grounding: recon multi-agente su `bennu-check` (inventario dei ~54
check), su `bennu-be`/`bennu-intel`/`bennu-java`/`bennu-index` (architettura + concorrenza), e su un
clone sparse di `JetBrains/intellij-community` (daemon di highlighting + catalogo errori Java tipizzato
`JavaErrorKinds` + `JavaCompilationErrorBundle.properties`).

Complementare a `docs/bennu-indexing-validation-analysis.md`, `docs/bennu-inference-roadmap.md` e a
`crates/products/bennu/check/README.md`.

---

## 0. Verdetto in tre righe

1. **La validazione di Bennu è già molto ampia e concettualmente allineata a IntelliJ.** ~54 check
   coprono la stragrande maggioranza degli errori "pane e burro" di javac. La differenza filosofica —
   *mai un falso positivo → nell'incertezza skip* — è **giusta** per un checker senza compilatore e va
   mantenuta (IntelliJ può essere definitivo perché ha un type-system completo; noi no).
2. **I gap di copertura reali sono pochi e già mappati** nella inference-roadmap (generics arity/bound,
   sealed `permits`, annotation-element, overload ambiguity, method-ref). Nessuna sorpresa.
3. **Il valore più alto NON è aggiungere check, è architetturale**: adottare progressivamente il
   modello a *query incrementale alla rust-analyzer/Salsa* con **invalidazione a due livelli
   (out-of-code-block)**, il *dispatch per node-kind*, e i **messaggi tipizzati** (catalogo `CheckId`).
   Sono le leve che IntelliJ usa per stare reattivo su progetti enormi.

---

## 1. Copertura delle validazioni

### 1.1 Cosa Bennu già copre (bene)

Le famiglie sotto sono **già implementate** e conservative. Riferimento canonico: `check/README.md`.

- **Sintassi & statement**: parse error, statement non-espressione (§14.8), `var` senza tipo target.
- **Dichiarazioni & modifier**: abstract in classe concreta, abstract con body, combinazioni illegali,
  record abstract / con campi d'istanza, enum constant con args senza ctor, method body mancante.
- **Control-flow**: missing return, return-in-void / value-required, unreachable code, switch-yield,
  fall-through (warning), return/break/continue in `finally` (warning).
- **Tipi**: cast/assign/return incompatibili, narrowing primitivo lossy, condizione non-boolean,
  `instanceof` inconvertibile, istanziazione di abstract/interface.
- **Membri & arità**: metodo/campo inesistente su receiver inferito, arity mismatch (varargs-aware),
  argument-type mismatch (single-overload, definite), `super.m()` inesistente.
- **Ereditarietà**: extends final/record/enum/interface, implements non-interface, abstract non
  implementati, ciclo d'ereditarietà, `@Override`-che-non-override-niente, override di `final`,
  covarianza del return.
- **Eccezioni**: checked non gestita (throw diretto **e** da chiamata), widening su override, catch
  irraggiungibile, multi-catch ridondante, resource non-`AutoCloseable`.
- **Generics (sintassi)**: generic array creation, istanziazione di type-var, generics in
  `instanceof`/`catch`, erasure clash, interfaccia duplicata in `implements`.
- **Costruttori & init**: super-ctor obbligatorio, `this(...)` ricorsivo, canonical record ctor
  incompleto, blank-final mai inizializzato, metodo-chiamato-come-la-classe (warning).
- **Nomi & file**: nome tipo pubblico ≠ file, `package-info`/`module-info`, package mismatch.
- **Import**: non risolto, inutilizzato, duplicato, wildcard ridondante, clash tra single-import.
- **Visibilità & static**: `private`/package-private cross-scope, membro non-static da contesto static.
- **Enum**: exhaustiveness dello switch-**espressione** su enum.
- **Lambda/functional**: arità lambda vs SAM, `@FunctionalInterface` non-funzionale, effectively-final
  (mutazione dentro lambda **e** cattura+riassegnazione fuori).
- **Version gating**: feature usata sotto la major target (records/sealed/var/text-block/…).
- **Lint**: self-assignment, `/0`, empty statement, `String ==`.

Per un editor senza compilatore, questa è una copertura **notevole** — più ampia di molti LSP Java
"leggeri". Il gating anti-FP (tutto passa da `walk.rs`: tipo sconosciuto ⇒ "potrebbe soddisfare" ⇒ mai
verdetto negativo) è coerente e ben applicato.

### 1.2 Gap rispetto a IntelliJ (catalogo `JavaErrorKinds`, 496 error-kind)

Legenda priorità: **P1** alto valore/basso rischio · **P2** valore medio · **P3** deferibile/rischioso ·
**N** non inseguire (serve un vero type-system/dataflow).

| # | Gap (compile-level) | IntelliJ error-kind / messaggio | Prio | Note |
|---|---|---|---|---|
| G1 | **Arità type-argument** `List<A,B>` | `Wrong number of type arguments: {0}; required: {1}` | **P1** | Puramente sintattico + conteggio dei type-param dichiarati. Zero rischio FP. Roadmap **B1**. |
| G2 | **Bound generico violato** `Box<String>` con `<T extends Number>` | `Type parameter ''{0}'' is not within its bound; should extend ''{1}''` | **P1** | Solo su tipi concreti a gerarchia nota. Roadmap **B1**. Sbloccato dall'indicizzazione dipendenze. |
| G3 | **Sealed `permits`** | `''{0}'' is not allowed in the sealed hierarchy` | **P1/P2** | I dati (`PermittedSubclasses`) sono già decodificati, oggi tenuti solo come flag `is_sealed`. Roadmap **B6**. |
| G4 | **Annotation-element obbligatorio mancante** | `{0} missing but required` | **P2** | Serve modellare elementi+default del tipo annotazione. Roadmap **B6**. |
| G5 | **`@Override` su static / static↔instance override conflict** | `Static methods cannot be annotated with @Override`; `Instance method … cannot override static method …` | **P2** | Estensione naturale dei check override già presenti. Dati già disponibili (`is_static`). |
| G6 | **Forward reference** (campo/enum-const usato prima della definizione) | `Cannot reference ''{0}'' before … definition` | **P2** | Ordine testuale nello scope del tipo; gate su same-file. |
| G7 | **Overload ambiguo** `combo(null,null)` | `Ambiguous method call: both ''{0}'' and ''{1}'' match` | **P3** | Serve applicabilità + most-specific (JLS §15.12.2). Roadmap **B3**. Solo su gerarchia completa. |
| G8 | **Method-reference resolution** `Type::m` | `Cannot resolve method ''{0}''` | **P3** | Rischioso senza inference target-typed. Deferito in README. |
| G9 | **Switch a pattern**: exhaustiveness/dominance/`sealed`, deconstruction | `''switch'' … does not cover all possible input values`; `Label is dominated by …` | **P3** | Java 17-21. Meno rilevante per il target legacy (Struts/JDBC), ma cresce. |
| G10 | **PECS / varianza wildcard** `add` su `List<? extends T>` | (via applicabilità) | **P3** | Roadmap **B5**. Massimo rischio FP. |
| G11 | **Definite assignment** "variable might not have been initialized" / "used before assigned" | `Variable ''{0}'' might not have been initialized` | **N** | Serve vera analisi di flusso. Alto rischio FP. README lo esclude esplicitamente — **giusto**. |
| G12 | **Raw-type / unchecked warnings** | `Unchecked assignment: …`, `Unchecked call to …` | **N** | Warning, non errori; alto rumore. Deferito in README — ok. |

**Sintesi copertura**: i gap **P1** (G1, G2, G3) sono già in cima alla inference-roadmap (fase P1),
poggiano su dati esatti/gerarchia-nota e **non** rischiano FP. Sono il ritorno migliore lato "nuove
squiggle". Tutto il resto è correttamente o pianificato (P2/P3) o escluso (N).

### 1.3 Inspection non-compile ad alto valore per il target legacy

IntelliJ ha centinaia di inspection "probable-bug". La regola cardinale ci obbliga a scegliere solo
quelle **decidibili senza dataflow**. Candidate a basso rischio da valutare per il tier lint di Bennu
(`expr_lint`/`switch_flow` già esistono):

- **`ResultOfMethodCallIgnored`** — valore di `String.trim()`/`.replace()`/… scartato (lista curata di
  metodi puri noti). Basso FP se la lista è conservativa.
- **`EqualsBetweenInconvertibleTypes`** — `a.equals(b)` con tipi concreti non correlati (riusa `walk`).
- **`NumberEquality`** — `Integer == Integer` (boxed) con `==`. Complementa `String ==` già presente.
- **`OptionalGetWithoutIsPresent`** — `.get()` non guardato (euristica locale, gate stretto).
- **`RedundantThrows`** — checked in `throws` mai lanciata nel corpo (riusa la logica checked-exception).
- **`EqualsAndHashcode`** — override di uno solo dei due.
- **`MethodMayBeStatic`** — metodo privato che non tocca `this` (utile refactor legacy).

**Da NON tentare** senza un motore di dataflow vero (sono il cuore "intelligente" di IntelliJ e senza
di esso producono FP a raffica): `ConstantValue`, `DataFlowIssue`/NPE nullability, "condition always
true/false". Sono esattamente ciò che la regola cardinale vieta di approssimare.

---

## 2. Qualità dei messaggi + catalogo tipizzato

### 2.1 La mossa architetturale di IntelliJ da copiare: `JavaErrorKinds`

JetBrains ha **estratto la detection degli errori Java in un layer a sé** (`com.intellij.java.
codeserver.highlighting`): un catalogo tipizzato `JavaErrorKinds` (~496 costanti, es.
`Method.OVERRIDE_FINAL`, `Generics.TYPE_PARAMETER_INCOMPATIBLE_UPPER_BOUNDS`) + un unico bundle di
stringhe. `HighlightVisitorImpl` fa solo da **adapter** verso l'`HighlightInfo` dell'editor, e i
**quick-fix si registrano per error-kind** in una tabella (`JavaErrorFixProvider`). Detection,
presentazione e fix sono **tre assi disaccoppiati**.

**Raccomandazione per Bennu (P1, allineata alla filosofia "centralizzare" di CLAUDE.md):** introdurre
un `enum CheckId` (o `DiagnosticKind`) in `bennu-proto`/`bennu-check` e portarlo sul `Diagnostic`.
Oggi i messaggi sono stringhe inline sparse nei ~54 moduli. Un catalogo tipizzato dà, in un colpo:

- **messaggi centralizzati** (un posto solo per il wording, niente drift tra check simili);
- **fix-registry per kind** — quando arriveranno più quick-fix (oggi c'è `intentions.rs` per gli
  import), agganciarli a `CheckId` invece che a stringhe fragili;
- **soppressione/config per kind** — abilitare/disabilitare o cambiare severità di una regola dai
  settings, per-progetto (come i profili inspection di IntelliJ);
- **suite di test più solida** — asserire il `CheckId` invece del testo del messaggio.

È un refactor incrementale (si può introdurre l'enum e migrarci i check uno per volta) e **abilita** il
lazy-fix-by-kind e la config per-regola. Massimo ritorno strutturale a basso rischio.

### 2.2 Wording: IntelliJ separa "cosa" da "richiesto/trovato"

Pattern IntelliJ da adottare dove manca:

- **Found/Required strutturato**: `Incompatible types. Found: ''{1}'', required: ''{0}''` + tooltip con
  righe `Required type:` / `Provided:`. Un secco "incompatible types" è già indietro.
- **Conteggi espliciti** con pluralizzazione: `Expected {0} argument(s) but found {1}`.
- **Choice/variant nel messaggio**: `Cannot inherit from {final class|enum|record} ''{0}''`.

Upgrade concreti suggeriti (Bennu → target):

| Check | Oggi | Suggerito |
|---|---|---|
| `arity` | "Cannot find a method `m` that matches the argument count" | "Method `m` cannot be applied: expected N argument(s) but found M" |
| `arguments` | "Inconvertible types: cannot convert `X` to `Y`" | "Incompatible types. Found: `X`, required: `Y`" (allinea al wording IntelliJ) |
| `inheritance` | "Cannot inherit from final `X`" | ok — già in stile choice |
| `casts` | "Inconvertible types: cannot cast `S` to `T`" | ok — **già identico** a IntelliJ |

(Il grosso dei messaggi Bennu è già buono; qui si tratta di rifinire una manciata di casi, meglio se
**dopo** aver introdotto il catalogo `CheckId` così il wording vive in un posto solo.)

---

## 3. Architettura & concorrenza

### 3.1 Cosa Bennu fa già bene (confermato dal recon)

- **Un solo parse per file**, una sola raccolta nodi condivisa da tutti i check
  (`check.rs`); una sola `extract_symbols` + un `InferCache` condiviso per la fase resolver.
- **Completion lock-free**: `RwLock<Arc<NativeJavaProvider>>` → si clona l'`Arc` sotto lock breve, poi
  si lavora senza contesa. Nessun lock annidato osservato.
- **Generation-swap** dell'indice (`g<NNN>`): il nuovo provider è costruito su file nuovi, l'mmap
  vecchio resta valido → niente errore Windows 1224, niente stall.
- **Supersession guard**: ogni rebuild bumpa una generation; il thread stale controlla e **esce
  pulito** a 3 checkpoint (nessun busy-wait).
- **Validazione incrementale**: overlay in-memory del solo file editato, `members_cache` invalidata al
  patch, **cache diagnostica dependency-aware** (epoch = hash JDK+jar; per-file content-hash) con
  warm-up job on-open e parallelizzazione. Cold 23s → warm ~175ms (dalla memoria di progetto).
- **Parallelismo** con work-stealing che lascia ~2 core liberi per il path interattivo.

Questo è un impianto **già molto sano**. Le raccomandazioni sotto sono evoluzioni, non correzioni.

### 3.2 I pattern IntelliJ trasferibili (mappati sullo stato Bennu)

| # | Pattern IntelliJ | Bennu oggi | Azione |
|---|---|---|---|
| A1 | **Modello a query incrementale** (CachedValue + modification-tracker + stub-index) = *Salsa/rust-analyzer* | Cache diagnostica per-file + overlay + InferCache (coarse, per-file) | **P2 strategico.** Muovere progressivamente il resolve/inference verso un query-engine con revisioni. È l'espressione Rust nativa di ciò che IntelliJ fa a mano. Grosso, incrementale. |
| A2 | **Invalidazione a due livelli (out-of-code-block)**: edit dentro un body **non** invalida struttura/resolve cross-file; edit a signature/import sì | La cache diag è keyed sul content-hash dell'intero file → **ogni** edit ri-valida tutto il file | **P1/P2.** Distinguere "edit dentro un method body" da "edit strutturale". Un edit di body dovrebbe invalidare solo quel metodo (e nessun file dipendente). La leva perf singola più grande di IntelliJ. |
| A3 | **Dispatch per node-kind** (`InspectionVisitorOptimizer`): indice `kind → nodi`, ogni rule visita solo i suoi kind | I ~22 check resolver iterano **ciascuno** l'intera slice di nodi | **P1.** Costruire una volta `HashMap<SyntaxKind, Vec<Node>>` e far dichiarare a ogni check i kind che gli interessano. O(nodi interessanti) invece di O(nodi × check). Safe, misurabile (l'agente ha flaggato `check.rs` come hotspot medio). |
| A4 | **Viewport-first** (`Divider`: analizza il range visibile prima del resto) | Valida sempre l'intero file | **P2.** Per file molto grandi: emettere prima le diagnostiche del range visibile, poi il resto. Migliora la *percezione*. Meno urgente (0.7s per 2.8k righe è ok), utile sui file legacy giganti. |
| A5 | **NBRA** (NonBlockingReadAction): background + read-locked + token-cancellabile + idempotente-fino-al-publish + `coalesceBy` (debounce) + `finishOnUiThread` atomico | Ogni richiesta IPC su thread proprio; supersession-guard come cancellazione coarse | **P2.** Formalizzare un primitivo "analisi cancellabile" con **coalescing** per (file): un nuovo keystroke cancella l'analisi in volo dello stesso file. In Rust: `parking_lot::RwLock::upgradable_read` mappa **esattamente** il Write-Intent lock di IntelliJ. |
| A6 | **Cancellation cooperativa** con generation bump + `checkCanceled()` in ogni loop | Guard a 3 checkpoint (coarse) | **P2.** Portare il `check_cancelled()` **dentro** i loop dei check pesanti (per-nodo), non solo tra le fasi, così un edit interrompe una validazione lunga a metà. |
| A7 | **Recycle-and-diff dei marker** keyed by `(toolId, element)` (anti-flicker) | (lato FE CodeMirror) | **P2 FE.** Diffare le diagnostiche per `(CheckId, node_id)` così ri-analizzare un nodo swappa solo la sua slice, senza far "sfarfallare" tutte le squiggle. |
| A8 | **Lazy quick-fix**: i fix si calcolano on-demand solo per la diagnostica vicina al caret/visibile | `intentions.rs` è già on-demand (handler `bennu_import_edit`) | **OK.** Già sostanzialmente lazy. Da mantenere quando crescono i fix (registrarli per `CheckId`, A-cat 2.1). |
| A9 | **Due tier netti**: correctness always-on (HighlightVisitor) vs lint pluggable (LocalInspectionTool) | `check_file` (puro-AST) + `check_file_resolved`; lint mescolato ai check | **P3.** La separazione c'è di fatto; volendo, esplicitare un `LintPass` con regole config-abilitabili separate dai check di correttezza. |
| A10 | **Stub-index**: parse-once solo-signature, resolve contro l'indice, body AST lazy | L'indice progetto porta già `members_json` per-tipo (≈ stub) | **OK/parziale.** Bennu ha già l'equivalente dello stub (membri persistiti). Il salto è legare A2 a questo: editare un body non tocca lo stub → i resolve cross-file restano validi. |

### 3.3 Cosa **non** copiare da IntelliJ (specifico PSI/JVM/Swing)

- **EDT/write-thread come identità fissa** (JetBrains la sta pure rimuovendo, IJPL-53). Conta la
  *semantica* del lock, non il thread. In Rust: `RwLock` + upgradable-read.
- **`ProcessCanceledException` come control-flow** → in Rust `Result`/early-return + token, mai
  panic/unwind.
- **`RangeMarker` GC-driven** → rebasing di offset `(start,end)` contro il delta dell'edit (o
  rope/piece-table con marker layer).
- **Reflection dei visitor / `plugin.xml`** → dichiarare i `SyntaxKind` staticamente; manifest TOML/Lua
  di Arbor.
- **`SoftReference` eviction** → LRU/arena con cap esplicito.
- **`DumbService`/`IndexNotReadyException`** → serve solo se si replica un'indicizzazione async con
  "dumb mode"; oggi Bennu gate su `jdk_available`/indice-caldo, sufficiente.

---

## 4. Idle CPU (~13% a riposo)

Issue nota e ancora aperta (memoria `project_bennu_idle_cpu`). Il recon di concorrenza la **esclude
dalla logica di indicizzazione di Bennu**:

- il thread di build **esce pulito** ai checkpoint di supersession, **nessun** `loop { recv_timeout }`,
  `loop { sleep + check }`, né busy-wait nei crate `bennu-*`;
- c'è persino un log diagnostico esplicito ("index build thread exiting") oltre il quale, se la CPU
  brucia ancora, **non è quel thread**.

Sospetti residui, in ordine:
1. **reader framed-stdio di `arbor_be`** (il transport IPC, fuori dai crate bennu) — un loop di lettura
   che non si blocca bene in idle.
2. **file watcher nativo** (se attivo) che polla il filesystem.
3. eventuale **runtime tokio** in `arbor_be` con un timer/driver che gira a vuoto.

**Prossimo passo diagnostico concreto**: con la BE avviata e progetto aperto ma *inattivo*, campionare
gli stack del processo `bennu-be` (es. `dotnet-trace`/`ETW` o un semplice attach del debugger e pausa
ripetuta): lo stack che ricorre nel ~1 core in spin nomina il colpevole. Se punta al reader IPC, il fix
è nel transport `arbor_ipc` (read bloccante vs poll), non in Bennu. (Il sospetto `refs.rs` in memoria
**non** è stato confermato: il reference-walk gira una volta a build, non a riposo.)

---

## 5. Roadmap prioritizzata (ROI)

**P1 — alto valore, basso rischio (fare prima):**
1. **Catalogo `CheckId` tipizzato** (§2.1) — abilita fix-by-kind, config-per-regola, messaggi
   centralizzati, test più solidi. Refactor incrementale.
2. **Dispatch per node-kind** (§3.2 A3) — perf pura, safe, misurabile.
3. **Generics arity + bound** (G1, G2 = inference-roadmap **B1**) — le prime "squiggle nuove" a rischio
   FP nullo, moltiplicate dall'indicizzazione dipendenze appena introdotta.
4. **Sealed `permits`** (G3 = **B6**) — dati già decodificati.

**P2 — valore medio / più lavoro:**
5. **Invalidazione out-of-code-block** (§3.2 A2) — la leva perf incrementale grande; edit di body ≠
   edit strutturale.
6. **Annotation-element mancante** (G4 = **B6**); **`@Override` su static / override conflict** (G5);
   **forward reference** (G6).
7. **Cancellazione cooperativa + coalescing per-file** (A5/A6); **viewport-first** sui file giganti (A4).
8. Manciata di **inspection lint conservative** (§1.3): `ResultOfMethodCallIgnored`,
   `EqualsBetweenInconvertibleTypes`, `RedundantThrows`, `NumberEquality`.

**P3 — deferire (serve type-system più profondo, alto rischio FP):**
9. Overload ambiguity (**B3**), method-ref resolution, switch-a-pattern (G7-G9), PECS (**B5**).

**N — non inseguire** senza un vero motore di dataflow: definite-assignment (G11), unchecked/raw (G12),
nullability/constant-value.

**Trasversale**: chiudere l'idle-CPU (§4) — è UX percepita (ventola/batteria) e va isolato nel transport.

---

## 6. Claude skills utili per Bennu

Nessuna skill Java pronta all'uso è installata (le disponibili sono plugin-dev, mcp-server-dev,
frontend-design, skill-creator, claude-md-improver, hookify…). Ciò che conviene **creare**:

- **Skill `/bennu-check`** (come l'esistente `/nemus`): incapsula il pattern consolidato per aggiungere
  un check — modulo in `check/src/`, gating conservativo via `walk.rs`, `#[cfg(test)] mod tests` con
  `MapResolver` mock (casi tipici + edge + **negativi che devono restare vuoti**), aggiornare
  `check/README.md` + `CHANGELOG` + docs in-app. Scaffoldabile col plugin **skill-creator**.
- **Hook anti-compilazione** (plugin **hookify**): un `PreToolUse` che blocca `cargo`/`yarn`/`npm run`
  — codifica la hard-rule "mai compilare, lo fa l'utente" così nemmeno un agente futuro la viola.
- **`claude-md-improver`**: il `CLAUDE.md` è molto grande; una passata per estrarne le parti Bennu in un
  file dedicato ridurrebbe il contesto caricato ogni sessione (opzionale).

---

## Appendice — riferimenti IntelliJ (clone sparse in `temp/disposable-projects/intellij`)

- **Catalogo errori Java**: `java/codeserver/highlighting/.../JavaErrorKinds.java` (~496 kind),
  `.../JavaErrorCollector.java`, bundle `JavaCompilationErrorBundle.properties` (stringhe canoniche),
  legacy `JavaErrorBundle.properties` (generics/unchecked).
- **Adapter presentazione + fix**: `java/java-analysis-impl/.../analysis/HighlightVisitorImpl.java`,
  `JavaErrorFixProvider.java`, `DefaultJavaErrorFixProvider.java`.
- **Daemon/pass/incrementalità**: `platform/analysis-impl/.../daemon/impl/{GeneralHighlightingPass,
  Divider, HighlightVisitorRunner, ProgressableTextEditorHighlightingPass, DaemonProgressIndicator,
  InspectionVisitorOptimizer, LazyQuickFixUpdaterImpl, FileStatusMap, FileStatus, UpdateHighlightersUtil,
  HighlightInfoUpdater}.java`.
- **Concorrenza/cache**: `platform/core-api/.../openapi/application/{Application, ReadAction,
  NonBlockingReadAction}.java`; `.../psi/util/{CachedValuesManager, CachedValue, PsiModificationTracker}.java`.
