# Bennu — potenziare l'inference per sbloccare i check mancanti

Analisi di come far crescere il motore di type-inference di `bennu-java` (e il seam dei tipi) per
abilitare i controlli di validazione oggi **deliberatamente rimandati** perché richiedono
un'inferenza più profonda della camminata nominale conservativa attuale. Complementare a
`docs/bennu-indexing-validation-analysis.md` (che elenca i check) e alla roadmap del crate
`bennu-check/README.md`.

Regola d'oro invariata: **mai un falso positivo**. Ogni capacità qui sotto va introdotta in modo che
un dato incompleto (tipo non risolto, gerarchia parziale, firma illeggibile) porti a **skip**, non a
un verdetto sbagliato.

---

## 1. Cosa sa fare l'inference OGGI

Motore: `crates/products/bennu/java/src/infer.rs`, seam in `java/src/seam.rs`.

- **Tipo nominale di un'espressione/receiver** (`infer_receiver_type`, `infer_expression_type`,
  `infer_node_type_cached`): literal, string-concat (`"x"+n → String`), accesso a campo/metodo sul
  tipo inferito del receiver, catene `a.b().c`, `this`/`super`, `new T(...)`, cast. Restituisce un
  `TypeRef { binary_name, type_args }`.
- **Risoluzione membri gerarchica** (`InferCache::resolve_methods` → `walk_methods`): raccoglie
  l'intero **overload set** di un nome lungo la gerarchia (superclass + interfacce) e segna se la
  gerarchia è **completa** (`MethodResolution { candidates, complete }`). I check arity/argument
  post-processano l'insieme in modo conservativo.
- **Sostituzione generica POSIZIONALE** (`substitute_generics`, infer.rs:498): se il receiver porta
  `type_args` (es. un local `List<Foo> xs`), un return che è una type-variable single-letter (`E`,
  `T`, col caso speciale `V` per il secondo arg di `Map`) viene sostituito **per indice** con
  l'argomento del receiver → `List<Foo>.get()` inferisce `Foo`. Ricorsivo sui `type_args`.
- **Dati già modellati nel seam** (`Member`/`ClassMembers`): visibilità, `is_static`/`is_abstract`/
  `is_default`/`is_final`, `ClassFlags` (interface/abstract/final/enum/record/**sealed**), e — chiave
  per la famiglia eccezioni — **`throws`** (le checked exception dichiarate).

### Limiti strutturali (la radice dei check mancanti)

1. **Nessuna dichiarazione di type-parameter con bound nel seam.** `ClassMembers`/`Member` non
   sanno che `class GenericsPlayground<T extends Number>` vincola `T`, né che
   `static <U> U identity(U)` introduce `U`. **MA il dato è già decodificato**: il decoder di firme
   JVMS §4.7.9.1 (`classpath/src/sig.rs`) produce `ClassSig.type_params` / `MethodSig.type_params`
   con `TypeParam { name, class_bound, interface_bounds }`. È **decodificato e poi scartato al
   seam** — il pezzo difficile (parsing) è fatto.
2. **Sostituzione generica euristica, non signature-driven.** `substitute_generics` indovina per
   indice/lettera; regge `List`/`Map` ma non un metodo qualsiasi che rimappa le variabili
   (`Map.entrySet(): Set<Entry<K,V>>`, `Optional.map(Function<T,R>): Optional<R>`).
3. **Nessun ranking di specificità né applicabilità con subtyping.** `walk_methods` dà l'insieme
   candidati "grezzo"; non c'è JLS §15.12.2 (fase applicabilità → most-specific).
4. **Nessuna variabile d'inferenza.** Non si risolve `<T> T max(T,T)` da due argomenti; niente
   constraint-set/bounds/soluzione.
5. **Nessuna varianza dei wildcard.** `? extends T` vs `? super T` non è modellato.
6. **Dati non-inference mancanti:** l'elenco `permits` di un sealed (c'è il *flag* `is_sealed`, non i
   nomi permessi), e gli **elementi/default di un tipo annotazione**.

---

## 2. Mappa check-mancante → capacità richiesta

Dai casi del progetto di test `prova-bennu` (package `com.nightprint.bennutest`) e dalla roadmap:

| Check mancante | Capacità che serve | Blocco (§3) |
|---|---|---|
| Bound generico violato (`GenericsPlayground<String>`, `T extends Number`) | type-param + bound + subtype | **B1** |
| Arità type-argument (`List<A,B>`) | conteggio type-param dichiarati | **B1** |
| Type-argument esplicito in conflitto (`this.<Integer>identity("s")`) | binding esplicito + sostituzione + subtype arg | B1+**B2** |
| Inferenza generica fallita (`max_of("s",42)`, `T extends Comparable<T>`) | variabili d'inferenza + solving | **B4** |
| PECS: `add` su `? extends T` | varianza wildcard + posizione input/output | **B5** |
| Overload ambiguo (`combo(null,null)`, varargs su `null`) | applicabilità + most-specific | **B3** |
| Catena su elemento generico (`list.get(0).x()`) | sostituzione signature-driven | **B2** |
| Sealed `permits` violato | elenco permitted-subclasses | **B6** |
| Elemento obbligatorio d'annotazione mancante | elementi + default del tipo annotazione | **B6** |

Nota: le famiglie **narrowing**, **static-context**, **checked-exception**, **instanceof
impossibile**, **covariant-return** (quest'ultima appena aggiunta) NON servono nuova inference — sono
già fatte o fattibili sui dati attuali.

---

## 3. I blocchi di capacità (in ordine di ROI)

### B1 — Modellazione signature-driven dei generici (type-param + bound)  ⟵ *massimo ROI, rischio basso*

**Cosa.** Portare `ClassSig.type_params` / `MethodSig.type_params` (già decodificati in `sig.rs`) fino
al seam: aggiungere a `ClassMembers` un `type_params: Vec<TypeParamDecl>` e a `Member` idem, dove
`TypeParamDecl { name, bounds: Vec<TypeRef> }` (class_bound + interface_bounds unificati). Popolarli:
- **bytecode** (`classpath/members.rs`): dalla `ClassSig`/`MethodSig` che già si decodifica per
  `raw_signature`.
- **sorgente di progetto** (`java/symbols.rs` → `intel/java_index.rs`): dai type-parameter scritti
  (`<T extends Number>`), banale da tree-sitter.
- attraversare il boundary in `query/resolver.rs::convert_members` (campo-a-campo, come gli altri).

**Check sbloccati** (tutti nuovi moduli in `bennu-check`, resolver-backed, gerarchia-fully-known):
- **arità type-argument**: `type_args.len()` scritto ≠ `type_params.len()` dichiarato → errore
  (dato puramente sintattico + conteggio; zero rischio FP).
- **bound violato**: per ogni `type_args[i]`, se risolve a una classe concreta e il bound risolve a
  una classe/interfaccia concreta a gerarchia nota, `!reaches(arg, bound)` → errore. Riusa
  `walk::reaches` / `hierarchy_fully_known`. Skip su wildcard, type-var, o bound non risolto.

**Effort:** medio (plumbing seam + 2 check). **Rischio FP:** basso (verdetti solo su tipi concreti a
gerarchia nota). **Prerequisito abilitante:** l'indicizzazione dipendenze (appena aggiunta) — ora
`List`/`Map`/`Optional`/tipi Spring risolvono, quindi i loro type-param diventano ispezionabili.

### B2 — Sostituzione generica signature-driven

**Cosa.** Sostituire l'euristica posizionale di `substitute_generics` con una vera **mappa
type-var → TypeRef** costruita da `(type_params dichiarati del tipo del receiver) ↔ (type_args del
receiver)`, applicata al return del metodo **per nome di variabile**, non per indice. Estendere alle
type-var introdotte dal metodo stesso (per B4).

**Check/feature sbloccati:** catene su elemento generico corrette (`list.get(0).foo()`,
`map.entrySet()`, `optional.map(...)`) → migliora TUTTI i check a valle (unknown-member/arity/argument
sul risultato di una catena generica) e la completion. Prerequisito per B1-esplicito e B4.

**Effort:** medio. **Rischio FP:** basso→medio (una sostituzione sbagliata degrada a "unknown", che è
skip, non falso positivo — purché in caso di dubbio si ritorni `None`).

### B3 — Applicabilità + most-specific overload ranking (JLS §15.12.2)

**Cosa.** Sopra l'attuale `MethodResolution.candidates`, implementare le tre fasi di applicabilità
(strict / loose / variable-arity) e la relazione "più specifico". Serve un `is_subtype(a, b)`
robusto (già quasi tutto in `walk::reaches`, da estendere a boxing/widening primitivi).

**Check sbloccati:**
- **overload ambiguo** (`combo(null,null)`): due candidati applicabili, nessuno più specifico →
  errore. Flag SOLO quando l'insieme candidati è **completo** e ≥2 restano massimamente-specifici.
- rafforza **argument-type** (oggi conservativo al singolo overload) e la selezione per l'inference.

**Effort:** alto (è il cuore della risoluzione metodi Java). **Rischio FP:** medio-alto — l'ambiguità
va dichiarata solo su gerarchia completa e regole di specificità complete; altrimenti skip.

### B4 — Variabili d'inferenza + solving

**Cosa.** Per una chiamata a metodo generico `<T…> R m(P₁…Pₙ)` senza type-argument espliciti:
introdurre variabili d'inferenza `αᵢ` per i type-param, raccogliere i **vincoli** `type(argⱼ) <:
Pⱼ[α]`, propagare i **bound** (dai `TypeParamDecl` di B1 + i vincoli), e **risolvere** (lub/glb sui
bound). Se i bound sono **incompatibili** (nessuna soluzione) → il caso `max_of("s",42)`.

**Check sbloccati:** inferenza generica fallita, type-argument esplicito in conflitto (con B2),
lambda-with-ambiguous-overload (con B3+target-typing).

**Effort:** molto alto (è un mini constraint-solver, la parte più vicina a un vero compilatore).
**Rischio FP:** alto. Consigliato **ultimo** e con soglia altissima: flag solo l'incompatibilità
*provata* su tipi concreti; ogni incertezza → skip.

### B5 — Varianza dei wildcard (PECS)

**Cosa.** Modellare `? extends X` / `? super X` / `?` nei `TypeRef` (oggi i type_args sono nominali).
Poi, per una `recv.method(arg)` dove il parametro usa la type-var in **posizione input** e il
receiver ha un `? extends` su quella variabile → scrittura vietata (`add` su `List<? extends T>`).

**Check sbloccati:** violazione PECS. **Effort:** alto (serve un `TypeRef` con nozione di wildcard +
posizione delle variabili nella firma del metodo). **Rischio FP:** alto — richiede B1+B2 solidi.

### B6 — Dati non-inference (indipendenti, ROI immediato)

- **Sealed `permits`**: `classpath/members.rs` decodifica già l'attributo `PermittedSubclasses`
  (oggi solo come flag `is_sealed`); esporne i **nomi** in `ClassFlags`/`ClassMembers`
  (`permitted: Vec<String>`). Check: un `class X extends Sealed`/`implements Sealed` non presente
  nella lista permessa → errore (gerarchia + lista note). Sblocca il caso `Cavalry`/`Unit`.
- **Elementi/default d'annotazione**: modellare, per un tipo annotazione, i suoi elementi e quali
  hanno `default`. Check: uso di `@Ann` senza un elemento obbligatorio senza default → errore.
  Serve estrarre gli elementi (da bytecode `AnnotationDefault` / da sorgente) — un piccolo modello a
  parte, non l'inference.

**Effort:** basso-medio ciascuno. **Rischio FP:** basso (dati esatti, non inferiti).

---

## 4. Roadmap consigliata (fasi)

1. **Fase P1 — plumbing generico + vittorie facili** (basso rischio, alto valore):
   B1 (type-param+bound al seam) → check *arità type-argument* + *bound violato*; B6 (*permits* +
   *annotation-element*). Tutto su dati esatti/gerarchia-nota. Moltiplicato dall'indicizzazione
   dipendenze appena introdotta.
2. **Fase P2 — sostituzione corretta**: B2 (signature-driven substitution). Migliora catene generiche
   e completion; prerequisito per il resto. Nessun nuovo check "rischioso", solo qualità.
3. **Fase P3 — overload**: B3 (applicabilità + most-specific) → *overload ambiguo* + argument-type più
   forte. Prima capacità davvero "da compilatore"; introdurre dietro gerarchia-completa.
4. **Fase P4 — inferenza vera**: B4 (variabili d'inferenza) e B5 (varianza) → *inferenza fallita*,
   *type-arg esplicito*, *PECS*. Massimo effort/rischio; soglia FP altissima.

Ogni fase è indipendente e testabile a slice fitte (come i batch precedenti di `bennu-check`): un
`#[cfg(test)] mod tests` con `MapResolver` mock che semina i `TypeParamDecl`, casi tipici + 1-2 edge,
e negativi che **devono** restare vuoti (gerarchia incompleta, tipo non risolto → skip).

---

## 5. Perché l'indicizzazione dipendenze (appena aggiunta) è un moltiplicatore

Prima, il resolver di validazione vedeva **solo JDK + progetto** (`for_project` costruiva un classpath
solo-JDK). Ora c'è il tier dipendenze (`ClasspathIndex` a due livelli): i tipi di libreria
(`java.util.List`, servlet, Spring, Hibernate) risolvono, quindi:

- i loro **type-param e bound** diventano leggibili → B1 ha su cosa lavorare nel codice reale (la
  stragrande maggioranza dei generici in un progetto legacy viene da librerie, non dal progetto);
- la **sostituzione** B2 conta davvero (`repository.findAll()` che ritorna `List<Entity>` →
  navigazione/validazione sull'elemento);
- molti "unknown member/type" oggi *saltati* perché il tipo del receiver era una libreria non
  risolta diventano **verdetti reali**.

In pratica: senza dipendenze, gran parte dei check generici non avrebbe tipi concreti su cui esprimere
un verdetto (skip perpetuo). Con le dipendenze, P1–P2 rendono immediatamente.

---

## 6. Rischi trasversali

- **Falsi positivi da gerarchia parziale.** Ogni verdetto positivo ("non è sottotipo", "ambiguo",
  "bound violato") solo su `hierarchy_fully_known`. Confermato dal pattern dei check esistenti.
- **Erasure vs generics.** Alcuni dati (firme) sopravvivono all'erasure nel bytecode via
  `Signature`; altri no. Dove la firma generica manca, degradare a raw → skip, mai indovinare.
- **Costo.** L'inference è già il collo di bottiglia (batch 11–12: domina `unknown_members`/
  `argument_type`). B2/B3/B4 aggiungono lavoro per-sito: mantenere tutto dentro `InferCache`
  (memoizzazione per posizione) e misurare con `BENNU_PROFILE` prima/dopo. La leva parallela
  (`validate_project.rs`) resta ortogonale.
- **Superficie del seam.** B1/B5/B6 cambiano `ClassMembers`/`Member`/`TypeRef` (serde): usare
  `#[serde(default)]` sui campi nuovi (come già fatto per `throws`/`flags`) così un indice persistito
  vecchio continua a deserializzare.
