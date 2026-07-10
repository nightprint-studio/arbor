# Bennu — Analisi: indicizzazione JDK/dependency & validazione AST (senza Maven)

Stato: analisi (2026-07-04). Grounding: recon completo su `bennu-classpath` / `bennu-index` /
`bennu-query` / `bennu-be` (indicizzazione) e su `bennu-be/build.rs` / `bennu-intel` / `bennu-web` /
`NativeJavaProvider` (validazione). Riferimenti `file:line` inline.

L'obiettivo di questo documento è duplice:

1. **Parte A** — fotografare l'indicizzazione JDK + dependency e individuare i pochi interventi ad
   alto valore (caching del bytecode, persistenza cross-sessione).
2. **Parte B** — progettare la **validazione semantica AST** che segnala errori di compilazione
   **senza lanciare Maven/javac**, riusando il resolver che già abbiamo. È il pezzo che oggi manca
   del tutto ed è la leva più grande per l'esperienza "IDE".

I due temi sono legati: la validazione AST martella `members_of`, quindi il caching di Parte A è
**prerequisito di performance** per Parte B.

---

## Parte A — Indicizzazione JDK & dependency

### A.1 Com'è fatta oggi (sintesi)

Tre crate leaf, nessuna dipendenza inter-Bennu:

- **`bennu-classpath`** — lettura bytecode JVM-free. `ClassSource` (trait) su tre container:
  `DirSource` (`target/classes`), `JarSource` (`rt.jar` + dep jar, via `zip`), `JimageSource`
  (JDK 9+ `lib/modules`, via `jimage-rs`). Ogni `.class` è decodificato da **`cafebabe`** +
  decoder homegrown del `Signature` generico (`sig.rs`, JVMS §4.7.9.1) con fallback al descrittore
  cancellato. `resolve_jdk_classpath(version)` scopre il JDK (extra-home configurati → `JAVA_HOME`
  → `Program Files`), con **fallback al JDK più recente** se manca la major esatta.
- **`bennu-index`** — store mmap'd (`fst` nome→offset + blob `rkyv`), header con `FORMAT_VERSION`.
  Schema multi-source (`Source`: `ProjectSource`, `JdkBytecode`, `DepBytecode`, `TargetClasses`,
  config-graph…) + relazioni (`Extends`/`Implements`/`Overrides`/`References`/config edges). I tipi
  di progetto portano `members_json` (i loro membri, per risolvere senza ri-parse del sorgente).
  Patch **per-file** incrementale.
- **`bennu-query`** — `IndexResolver` implementa `TypeResolver` di `bennu-java`: risolve
  **overlay in-memory** (file editato) → **indice progetto** (mmap) → **JDK/dep bytecode** live.
  `JdkMemberIndex` avvolge il `ClassSource !Sync` in un `Mutex`. **`members_cache`**
  (`RwLock<HashMap>`) memoizza `members_of` (anche i miss) — svuotata a ogni patch overlay.
  Modo `project_only` per il reference-walk (non decodifica il JDK per use-site che non si possono
  rinominare).

Il ciclo per-keystroke è solido: estrazione simboli del solo file editato → overlay → cache membri
invalidata; **nessun re-index globale, nessun re-resolve JDK**. Le build persistono in directory di
generazione `g<NNN>` (niente overwrite dell'mmap vivo → evita l'errore Windows 1224).

### A.2 Gap individuati (per valore)

| # | Gap | Dettaglio | Impatto |
|---|-----|-----------|---------|
| A1 | **Nessun cache del bytecode parse a livello sorgente** | `SourceMemberIndex::members_of` **rilegge i byte `.class` e ri-parsa a ogni chiamata**; l'unico cache è `members_cache` nell'`IndexResolver` (per-resolver, svuotato a ogni edit). | Alto — il reference-walk e (domani) il validatore chiamano `members_of` decine di migliaia di volte. |
| A2 | **Dep jar riaperti per lookup** | Ogni `members_of` su un tipo di dipendenza riapre/riscansiona lo ZIP; il cache Maven è solo sulla *risoluzione classpath* (mtime del pom), non sul bytecode. | Medio. |
| A3 | **Niente indice bytecode persistente cross-sessione** | `JdkBytecode`/`DepBytecode` esistono come `Source` nello schema ma vengono **ri-estratti a ogni open**; il bytecode è immutabile (mtime del jar/JDK). | Medio — apertura progetto più lenta del necessario. |
| A4 | **Maven solo offline di default** | Prima popolazione `~/.m2` richiede risoluzione online esplicita; dep di repo privati finiscono in `unresolved` (silenzioso, non-fatale). | Basso (by design). |
| A5 | **Fallback JDK non verificato per API** | Progetto Java 8 su JDK 21 risolve, ma differenze `java.*` non controllate. | Basso (by design). |

### A.3 Raccomandazioni (prioritizzate)

1. **[A1 — priorità 1] Cache dei membri parsati nel `JdkMemberIndex`.** Il bytecode JDK/dep è
   immutabile entro la sessione: aggiungere una `HashMap<String, Option<Arc<ClassMembers>>>` dentro
   `JdkMemberIndex` (già `Mutex`-protetto) elimina il ri-parse. È un cambiamento localizzato,
   **fortemente unit-testabile**, e sblocca la performance del validatore di Parte B. Nessun rischio
   di invalidazione (immutabile). *Effort: S. Rischio: basso.*
2. **[A3 — priorità 2] Persistenza `DepBytecode`/`JdkBytecode` keyed by mtime.** Estrarre i simboli
   bytecode una volta, persistere, e allo `open` successivo saltare l'estrazione se il jar/JDK
   non è cambiato (mtime). Lo schema ha già i `Source` giusti. *Effort: M. Rischio: medio (path
   di cache + invalidazione).*
3. **[A2] Riuso della `JarSource` aperta** già mitigato da #1 (il cache membri evita di riaprire
   lo ZIP per tipi già visti). Nessun intervento dedicato finché #1 non è misurato insufficiente.
4. **[A4/A5]** lasciare by-design; eventualmente un'azione esplicita "Resolve dependencies online"
   (una tantum) e un warning informativo quando il fallback JDK non copre una major.

> **Sintesi:** l'unico intervento *necessario a breve* è **A1** — ed è dettato da Parte B.

---

## Parte B — Validazione semantica AST (senza Maven)

### B.1 Cosa esiste e cosa manca oggi

**Esiste** (nessuno tocca il sorgente Java a livello semantico senza compilare):

- Errori di **compilazione** solo via `bennu_build` (`mvn -q -o compile` → fallback `javac`), parsati
  in `BuildDiagnostic` (`be/src/build.rs`). Richiede un build esplicito, Maven/JDK, ed è lento.
- **Config-graph** conservativo (`bennu_diagnostics` → `intel.rs`): esistenza azione Struts +
  esistenza dei file `<%@ include%>`, solo su JSP. Mai falso-positivo (wildcard/OGNL → `Inconclusive`).
- **JDK mancante**, **encoding**, **spell-check** — diagnostica non-semantica.

**Manca del tutto** — `NativeJavaProvider::diagnostics()` è **uno stub** (`provider.rs`, ritorna
`Vec::new()` con commento "syntactic diagnostics land with tree-sitter in a later wave"):

- riferimenti a **tipi non risolti** (import errato, classe inesistente/typo);
- **membri inesistenti** (`obj.metodoCheNonEsiste()`);
- **simboli non risolti** (variabile/identificatore sconosciuto);
- import inutilizzati, variabili inutilizzate;
- incompatibilità di tipo.

### B.2 La leva: il resolver c'è già

Abbiamo **tutto il substrato** per una validazione nominale senza compilatore:

- `bennu_java::infer_receiver_type(source, offset, resolver)` — tipo statico dell'espressione a
  sinistra del `.`.
- `TypeResolver::members_of(binary)` + `resolve_simple_name(name, imports)` — il seam che completion,
  hover e inherited già consumano.
- `extract_symbols(source)` — package, import, tipi/metodi/campi del file.
- l'AST tree-sitter-java (parsing veloce, già usato ovunque).

Il validatore è **la stessa macchina della completion, girata al contrario**: invece di "dammi i
membri di questo tipo", chiede "questo membro/tipo/nome esiste?".

### B.3 Principio guida: **conservativo come il config-graph**

La regola che ha reso affidabile la diagnostica Struts va replicata: **emetti un errore solo quando
la non-risolvibilità è certa**; in ogni situazione di incertezza, **taci**. Un checker homegrown che
produce falsi positivi perde la fiducia dell'utente in un giorno. Cause di incertezza da trattare
come "non segnalare":

- **JDK assente** → niente errori di tipo-non-risolto (non possiamo sapere cosa esiste nella stdlib).
- **Indice non ancora caldo** (build in corso) → nessuna diagnostica semantica.
- **Star import** `import java.util.*` / `import static Foo.*` → un simbolo non risolto localmente
  potrebbe venire da lì → non segnalare i tipi/membri potenzialmente coperti.
- **Generici non sostituiti** / inferenza fallita (limiti Fase-1: no overload-by-args, no
  flow-typing) → se `infer_receiver_type` ritorna `None`, **non** dedurre "membro inesistente".
- **Tipi dello stesso file / appena aggiunti** → già gestiti dal fallback `symbols.types`.

In pratica: `Diagnostic` di severità `error` **solo** quando il tipo è risolto con certezza **e** il
membro/nome non c'è; altrimenti niente (o al più `hint` de-enfatizzato in una fase successiva).

### B.4 Architettura proposta: crate `bennu-check`

Nuovo crate **puro** `bennu-check` (allineato alla filosofia "free crate split" + testabilità):

```
bennu-check
  ├─ dipende solo da: bennu-java (AST + resolver seam + extract_symbols), bennu-proto (Diagnostic)
  ├─ NON dipende da bennu-be/Tauri (zero glue) → interamente unit-testabile
  └─ API: fn check_file(source: &str, resolver: &dyn TypeResolver, opts: CheckOptions) -> Vec<Diagnostic>
```

`CheckOptions { jdk_available: bool, level: LangLevel, … }` porta i gate conservativi (es.
`jdk_available=false` ⇒ salta i check di tipo). Il resolver è lo **stesso** `IndexResolver` che
l'`IndexService` già costruisce — nessun nuovo indice.

**Wiring:** `NativeJavaProvider::diagnostics(file)` chiama `bennu_check::check_file(...)` con il
resolver del progetto. Il risultato viaggia nella `Diagnostic` **già esistente** (byte offset) →
compare **automaticamente** nel Problems panel e nel lint gutter (nessuna modifica FE). La
diagnostica è già fetchata per-file al tab switch; un AST-walk per file è ampiamente sotto budget.

**Perché un crate e non dentro `bennu-query`:** `bennu-query` è il motore di *query* (read-only,
completion/inherited); la *validazione* è una policy separata che consuma quel motore. Tenerla a
parte mantiene i confini netti e rende il checker testabile in isolamento con un `MapResolver` finto
(come già fanno inherited/completion).

### B.5 Fasi (per valore/rischio)

| Fase | Cosa segnala | Riuso | False-positive risk | Effort |
|------|--------------|-------|---------------------|--------|
| **1** | **Import non risolti** + **type-reference non risolte** (classe/typo) — solo quando NON risolvibili e NON coperte da star-import e con JDK presente | `resolve_simple_name`, `extract_symbols.imports` | Basso (con i gate) | **M** |
| **2** | **Membro inesistente** su receiver risolto con certezza (`obj.foo()` dove `foo` non è tra i membri) | `infer_receiver_type` + `members_of` (walk super/interfacce) | Medio (gate su inferenza `None`) | **M/L** |
| **3** | **Import inutilizzati** + **variabili locali inutilizzate** (severità `warning`/`hint`) | AST-walk puro (nessun resolver) | Basso | **S** |
| **4** | Incompatibilità di tipo / overload-by-args | — | Alto (oltre i limiti Fase-1 dell'inferenza) | **XL — DEFERITO** |

- **Fase 1** è il maggior ritorno: cattura import morti, refactor incompleti, typo di classe — gli
  errori "rossi" che un legacy-dev vuole vedere prima di compilare. Con i gate (star-import, JDK,
  same-file) il rischio di falso-positivo è basso.
- **Fase 2** aggiunge il "metodo che non esiste"; va gated forte sull'inferenza (se `infer` non è
  sicuro, silenzio).
- **Fase 3** è lint leggero, tutto-AST, senza resolver — utile e a basso rischio, ottima come primo
  taglio parallelo alla Fase 1.
- **Fase 4** richiede type-checking vero (overload resolution, flow-typing) — **fuori scope**: la
  disposition Fase-1 dell'inferenza lo esclude esplicitamente. Non inseguirla.

### B.6 Testabilità (il punto centrale)

`check_file` è una funzione pura `(source, resolver) -> Vec<Diagnostic>` → si testa esattamente come
`inherited`/`completion`: un `MapResolver` finto con tipi/membri noti + sorgenti Java inline. Ogni
regola porta i suoi casi: il tipo risolto/non-risolto, lo star-import che silenzia, il JDK assente
che silenzia, il tipo same-file che non è errore, il membro presente/assente, l'import
usato/inutilizzato. È il tipo di superficie dove costruire **molti** unit test (nessun runtime Tauri,
nessun JDK vivo — il resolver finto basta).

### B.7 Dipendenza da Parte A

Fase 1–2 chiamano `members_of`/`resolve_simple_name` su ogni type-reference e call-site del file →
**A1 (cache membri nel `JdkMemberIndex`) va fatto prima o insieme**, o il validatore ri-parsa il
bytecode JDK a ogni apertura di file. È l'unico accoppiamento forte tra le due parti.

---

## Prossimi passi concreti

1. **A1** — cache `HashMap<String, Option<Arc<ClassMembers>>>` in `JdkMemberIndex` (+ unit test).
   *Prerequisito di B.*
2. **B — Fase 3** (lint AST puro: import/variabili inutilizzate) come primo taglio a basso rischio,
   crate `bennu-check` scaffold + `check_file` + wiring `NativeJavaProvider::diagnostics`.
3. **B — Fase 1** (import + type-reference non risolti, conservativo) con la batteria di test sui
   gate (star-import, JDK assente, same-file).
4. **B — Fase 2** (membro inesistente su receiver certo).
5. **A3** (persistenza bytecode cross-sessione) quando l'apertura progetto diventa il collo di
   bottiglia percepito.

Fase 4 (type-checking completo) resta esplicitamente fuori scope finché l'inferenza non supera i
limiti Fase-1.
