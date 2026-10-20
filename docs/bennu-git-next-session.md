# Bennu × Git — piano per la prossima sessione

> Da incollare come primo messaggio di una sessione nuova. Italiano in chat, codice/commenti in inglese,
> regole del `CLAUDE.md` valgono tutte (in particolare: niente build/test finché non lo chiedo, niente nuove
> librerie senza chiedere, niente `localStorage` per i settings, UI solo con widget shared, keyboard-first).

## Dove siamo

Ho integrato Git in Bennu appoggiandomi a Corvus (`corvus-be`) **per path, senza tab**. Il lavoro precedente
**compila in release** (backend + `tauri build --no-bundle`) ma **non è stato provato a mano** e **i test non sono
mai stati eseguiti**. Niente è committato.

### Già fatto — da riusare, non rifare

**Corvus (backend)**
- `crates/products/corvus/core/src/state.rs` — repo "headless" con TTL (`register_ephemeral`, `sweep_ephemeral`,
  `ephemeral_tabs`; esclusi da `open_tabs()`). Tab id sintetico `path:<workdir>`.
- `corvus/be/src/ephemeral.rs` — `adopt`, `tab_for(state, path)`, `repo_tab_for_path`, sweeper. TTL in
  `cache.headless_repo_ttl_secs` (default 600 s; riga nelle impostazioni Cache di Corvus).
- `corvus/be/src/repo.rs` — `repo_path()` riadotta da solo un `path:` scaduto.
  **Ogni handler esistente accetta già `tab_id = "path:<dir>"`** (stage_paths, unstage_paths, commit, status…).
- `corvus/be/src/mcp_tools.rs` — tool MCP `corvus_status/log/blame/commit` (`#[handler(mcp(...))]`);
  `relative_to`, `walk_log`, `ymd` riusabili. `corvus` è in `EXPOSABLE` (`src-tauri/src/mcp/catalog.rs`).
- `corvus/be/src/file_history.rs` — `file_history_by_path`, `line_history_by_path` (`git log -L`),
  `file_commit_patch_by_path`. `corvus/be/src/diff.rs` — `get_file_blame_by_path`.
- `corvus/be/src/tree_status.rs` — `git_tree_status_by_path` (tutti i file cambiati + badge).
- `src-tauri/src/mcp/grants.rs` + pagina "Grants" (`McpGrantsSection.svelte`): grant in RAM a usi/scadenza.
- `src-tauri/.../rpc_commands.rs::ensure_product_backend` + `ensureBackend()` in `src/lib/ipc/rpc.ts`
  (Bennu avvia `corvus-be` quando serve).

**Frontend**
- Shared: `ColumnPanes`, `PaneHeader`, `DiffModeToggle` (`shared/ui/`), `utils/git-badge.ts` (glifi, etichette,
  colori dei badge: modified ambra, added verde, untracked blu, renamed accent, conflicted rosso),
  `utils/unified-patch.ts`, `utils/selection-delta.ts`.
- Editor condiviso: `code-editor/line-annotations.ts` (barretta blame + testo inline) e prop `lineAnnotations`;
  `EditorDiagnostic.emphasis: 'text'` (nomi indefiniti in rosso); `selectionLines()` in `CodeEditor`.
- Bennu: `stores/bennu/blame.svelte.ts`, `git-history.svelte.ts`, `git-status.svelte.ts`,
  `BennuGitHistoryModal`, `BennuLocalHistoryModal` (ora su `ColumnPanes`, con "for selection"),
  sottomenu *Git* e *Local History* nel menu contestuale dell'editor, voci di palette, colori/glifo sull'albero
  (`BennuSidebar`), setting `blame_show_message`.
- Tyto: `prune_unused_ffmpeg_tools()` cancella `ffprobe`/`ffplay` dopo il download di ffmpeg.

### Da validare a mano (mai provato)
Margine blame su tema GitHub dark · colori dell'albero · *Show History for Selection* (Git e Local) ·
pagina Grants · pulizia ffprobe/ffplay · `schemars` aggiunto a `corvus-be` (versione del workspace, **da
confermare** perché la regola chiede di chiedere prima).

### Test mai eseguiti
Rust: `state.rs`, `ephemeral.rs`, `mcp_tools.rs`, `file_history.rs`, `tree_status.rs`, `grants.rs`.
Vitest: `unified-patch.test.ts`, `selection-delta.test.ts`. Eseguirli solo se lo chiedo.

## Da fare, in quest'ordine

### 1. `git_summary_by_path` (backend, piccolo)
Un handler in `corvus-be` per `root`: branch corrente, `ahead`/`behind`, e **staged / unstaged / untracked
separati** (oggi `git_tree_status_by_path` li fonde in una mappa sola). Basta `corvus_git::get_status`
(`RepoStatus` ha già `current_branch`, `ahead`, `behind`, liste per categoria). Test unitario sulla parte pura.

### 2. Stage panel: estrarre un componente **senza store** (decisione presa)
Riusare la UI di staging di Corvus (`src/lib/components/corvus/stage/StageArea.svelte` 1151 righe,
`CommitForm.svelte` 339), **non** scriverne una nuova. Oggi leggono i singleton di Corvus
(`tabsStore.activeTab`, `repoStore.status`, `diffStore` ×28, `uiStore` ×17; `CommitForm` anche `graphStore`,
`cacheStore`, `commitConfigStore`) e passano `tab.id` a ogni IPC.

- Creare un `StagePanel` **props-driven** (probabilmente in `shared/internal/`, è Arbor-specific): riceve un
  contesto `{ tabId, status, reload }` e callback (apri diff, dopo commit, push opzionale), nessuno store.
  Dentro: lista/albero, accordion Unstaged/Staged, stage/unstage/discard per riga e per gruppo, area messaggio,
  *Amend last*, split button del commit.
- Il pannello diff (`DiffViewer`, legato a `diffStore`), `ConflictResolver` e lo stash restano **di Corvus**:
  il `StagePanel` li espone come snippet/slot opzionali.
- **Corvus**: un wrapper sottile che alimenta il `StagePanel` dai suoi store — **stesso comportamento di oggi**,
  nessuna regressione. Spezzare `StageArea` è anche l'occasione per rientrare nel limite di ~300 righe.
- **Bennu**: un wrapper che usa `tabId = "path:<project root>"` (via `repo_tab_for_path`) e gli handler esistenti;
  click su un file → diff in modale (riusare `TextDiff` + `parseUnifiedPatch`, serve un handler di patch del file
  contro HEAD, simile a `file_commit_patch_by_path`); bottone **Apri in Corvus** (`open_corvus_window` /
  `fsOpenInArbor`); nessun cambio branch, per quello si va su Corvus.
- Sistemare di passaggio i `localStorage` (`arbor:stage-view-mode`, fallback del template commit): vanno nel
  config su filesystem (regola 11).
- **Attenzione agli hook:** un commit da Bennu fa scattare `on_commit` / `on_pre_commit` con
  `tab_id = "path:…"`. Prima di abilitare il commit controllare quali plugin Lua usano quel `tab_id` come tab vera.
- **Un bottone sull'activity bar di Bennu per aprirlo.** Le rail di Bennu sono derivate dal progetto, non fisse
  (`stores/bennu/rails.svelte.ts`, `rail-order.ts`, `BennuCustomizeRailsModal`): il bottone *Changes* compare solo se il
  progetto è dentro un repository git (`bennuGitStatusStore.inRepo`) e segue le regole di ordinamento/nascondimento
  esistenti. Il pannello si apre in basso, **un pannello per bottone, niente tab strip**. Con badge col numero dei
  file modificati, voce in palette (*Git: show changes*) ed eventuale shortcut (da decidere, mai `Ctrl+Alt+lettera`).
- Test per la logica pura estratta (costruzione dell'albero, selezione).
- Aggiornare la tavola 2 del canvas (vedi sotto) sul modello della UI di Corvus.

### 3. Status bar di Bennu
Branch corrente con `↑ahead ↓behind`, **solo lettura**; un click apre Corvus. Dati da `git_summary_by_path`;
aggiornare con lo stesso trigger di `bennuGitStatusStore`. Eventuale riepilogo "N modified · N staged · N untracked"
(facoltativo, segnalato come non richiesto).

### 4. Marcatura di tab e Problems panel
Riusare `bennuGitStatusStore.badgeFor()` e `utils/git-badge.ts` (stessi colori e glifi dell'albero) sui tab
dell'editor e sulle righe del Problems panel che nominano un file.

### 5. Gutter delle modifiche (rispetto a HEAD)
- Backend: `diff_buffer_against_head(file, text)` → intervalli di righe aggiunte / modificate / rimosse, con la
  crate `similar` (già nel workspace, nessuna libreria nuova). Calcolo dopo una pausa di digitazione (~300 ms),
  fuori dal thread UI; saltarlo oltre una soglia di dimensione del buffer. Testo di HEAD ricaricato al cambio di
  commit / focus.
- Frontend: estensione `code-editor/change-gutter.ts` sul modello di `line-annotations.ts`: barretta a destra dei
  numeri (verde aggiunto, blu modificato, tacca rossa per righe tolte), distinta dalla barretta del blame.
- Click sul marcatore → popup: blocco com'era in HEAD (`TextDiff`), **Revert block** (rimette il vecchio testo
  nel buffer, da salvare) e **Copy old text**, frecce modifica precedente/successiva.
- Setting persistente `show_change_markers` nel config di Bennu (come `blame_show_message`: Rust
  `bennu-core/config.rs` — i campi prima delle tabelle —, `ipc/bennu/config.ts`, `settings.svelte.ts`,
  `BennuEditorSettings.svelte`).
- Palette: *Next change* / *Previous change*. **Shortcut da decidere con me** (mai `Ctrl+Alt+lettera`; proposta
  `Alt+F3` / `Alt+Shift+F3`).

### 6. Chiusura
Docs in-app (`bennu/docs/Editing.svelte`, `ProjectTree.svelte`, `Shortcuts` se si aggiungono tasti),
`CHANGELOG.md` `[Unreleased]`, README dei crate toccati (`corvus-be`, `corvus-core`). Scrivere nel riepilogo cosa
devo validare a mano e cosa è stato volutamente lasciato fuori.

## Riferimento visivo
Canvas privato con la proposta (5 tavole: editor con Git, stage panel, popup del marcatore, menu, history for
selection): https://claude.ai/artifact/7VkikTGCv7tSQZemaaQVMF — la tavola 2 (*Changes panel*) è disegnata a mano e
**va rifatta** sul modello della UI di staging di Corvus (header Unstaged/Staged con conteggi, icone di stage per
riga, *Amend last*, *Commit* con menu a tendina).

## Debiti emersi, da segnalare a margine (non bloccanti)
- `BennuEditor.svelte` ha oltre 5400 righe.
- `GitCommitList.svelte` e `HistoryTimeline.svelte` usano `<button>` ad-hoc: manca un widget shared per le righe
  di una lista selezionabile.
- Il pannello Grants avvia tutti i backend esponibili per leggere l'elenco dei tool.

## Domande aperte per me
1. Amend nello stage panel di Bennu (c'è già in quello di Corvus, quindi gratis con il riuso)?
2. Shortcut per *Next/Previous change*.
3. Conferma di `schemars` come dipendenza di `corvus-be`.
