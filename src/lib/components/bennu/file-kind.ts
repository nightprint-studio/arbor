/**
 * Bennu editor file-kind predicates — the single source of truth for "which editor
 * actions apply to this file".
 *
 * Used to gate the command palette, the editor context menu, and the keyboard
 * shortcuts so Bennu never *offers* (or fires) an action that's meaningless for the
 * open file: no "Generate…" / Java intentions on a `.jsp` or `.xml`, no go-to /
 * rename / find-usages on a plain `.txt` / `.md` / `.properties`. One place to
 * change if a new navigable file type is added.
 *
 * Two engines answer these questions now — Bennu's own Java analyzers and whatever language
 * server owns the file — so the predicates below are unions. Which engine replies is the
 * backend's business; whether an action is *offered* is this file's.
 */

import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
import { isImageFile } from '$lib/utils/image-files';
import { isCargoManifest } from './cargo-toml-lang';

/** Lower-cased extension of `path` (without the dot), or '' when there is none. */
function ext(path: string | null | undefined): string {
  if (!path) return '';
  const name = path.split(/[\\/]/).pop() ?? path;
  const dot = name.lastIndexOf('.');
  return dot >= 0 ? name.slice(dot + 1).toLowerCase() : '';
}

/** A `.java` source — the only kind that supports Generate + the Java intentions. */
export function isJavaFile(path: string | null | undefined): boolean {
  return ext(path) === 'java';
}

/** A JSP-family file (page / fragment / tag file). */
export function isJspFile(path: string | null | undefined): boolean {
  return ['jsp', 'jspf', 'tag', 'tagx'].includes(ext(path));
}

/** An XML-family config file (struts / spring / tiles / mybatis / validation / pom / tld …). */
export function isXmlFile(path: string | null | undefined): boolean {
  return ['xml', 'xsd', 'wsdl', 'xsl', 'xslt', 'tld', 'pom', 'iml', 'fxml'].includes(ext(path));
}

/** A Rust source. Served by a language server — see {@link isLspFile}. */
export function isRustFile(path: string | null | undefined): boolean {
  return ext(path) === 'rs';
}

/**
 * True when a **language server** is the engine for this file.
 *
 * Answered from the backend catalogue (cached in {@link bennuLspStore}) rather than from a list
 * here, because the user's own `[[lsp.servers]]` config can add a language the frontend has never
 * heard of — and a hard-coded list would then withhold go-to from a file that has it.
 *
 * Deliberately independent of whether the server is *running*: a `.rs` file is Rust's whether
 * rust-analyzer has finished starting or has crashed. Gating on liveness would make the actions
 * flicker in and out of the menus during startup, and would route the file to the Java engine in
 * the meantime.
 */
export function isLspFile(path: string | null | undefined): boolean {
  return bennuLspStore.servesFile(path);
}

/**
 * An image — opened as a **preview** rather than refused as binary.
 *
 * Re-exported from the shared list rather than spelled out again here: the set is not Bennu's, and
 * it had already been written three times in the app before the list existed.
 */
export { isImageFile };

/** A geode `.dig` mole script. Highlighted, folded, and completed from its (closed)
 *  vocabulary — but its navigation is a later story, so not in {@link supportsCodeNav}. */
export function isDigFile(path: string | null | undefined): boolean {
  return ext(path) === 'dig';
}

/**
 * True when Bennu's **Java analyzers** understand the file at all — Java (symbols), JSP
 * (page vars + action refs + includes), and config XML (`class="…"` / bean ids / mapper
 * statements). This is the one list; the two predicates below name the two things callers
 * actually want to know, so the set can grow in a single place.
 *
 * `.dig`, `.sql` and the plain text kinds are out: they are *edited* in Bennu (highlighted, and
 * `.dig` completed) but no analyzer in `bennu-be` can say anything about them. `.rs` is out of
 * *this* set too — nothing Java can read it — and in the unions below, because a language server
 * can; so is a `Cargo.toml`, which has its own validator.
 */
function isAnalyzedFile(path: string | null | undefined): boolean {
  return isJavaFile(path) || isJspFile(path) || isXmlFile(path);
}

/**
 * True when the file supports semantic navigation — go-to declaration, find usages,
 * rename, file structure.
 *
 * Either engine qualifies: the Java analyzers for a `.java`/`.jsp`/config XML, a language
 * server for anything it serves. `.dig` stays out — it is highlighted and completed from its
 * own closed vocabulary, but resolving a declaration across files needs a cross-file pass it
 * does not have, and a go-to that silently does nothing is worse than an action that is not
 * offered.
 */
export function supportsCodeNav(path: string | null | undefined): boolean {
  return isAnalyzedFile(path) || isLspFile(path) || isWgslFile(path);
}

/** A WGSL shader.
 *
 *  In the navigation and diagnostics sets **whether or not** a language server is installed,
 *  which is what makes it different from every other non-Java entry here: the backend answers
 *  for a shader either way — `wgsl-analyzer` when it is there, and naga plus its own scanner
 *  when it is not. Gating this on {@link isLspFile} would withhold find-usages from a file
 *  that has it. */
export function isWgslFile(path: string | null | undefined): boolean {
  return ext(path) === 'wgsl';
}

/**
 * True when it is worth asking `bennu_diagnostics` about the file.
 *
 * Nearly the same set as {@link supportsCodeNav}, and named separately because the *reason* differs:
 * navigation is hidden for lack of an index, whereas a validation request for a file no engine
 * understands would spend a round-trip per keystroke on an answer that can only be empty.
 *
 * A **`Cargo.toml`** is in this set and not in the navigation one, which is the difference the two
 * names exist for: the manifest schema has plenty to say about whether the file is right, and
 * nothing to say about going to a declaration in it.
 */
export function supportsDiagnostics(path: string | null | undefined): boolean {
  return isAnalyzedFile(path) || isLspFile(path) || isCargoManifest(path) || isWgslFile(path);
}

/**
 * True when the file's diagnostics come from a language server rather than from Bennu's own
 * validators.
 *
 * The distinction the editor needs for its **debounce**: the Java path runs a fast syntactic
 * pass and then a slower resolver-backed one, because both are computed on demand. A server's
 * diagnostics are *pushed* — they already exist by the time they are asked for — so a
 * server-backed file wants one cheap read, not a two-tier schedule.
 */
export function hasPushedDiagnostics(path: string | null | undefined): boolean {
  // A manifest is computed on demand from the buffer, like the Java path — so it wants the ordinary
  // debounce and not the single cheap read a server-backed file wants.
  // A shader is computed on demand too, unless a server took it over — and when one has,
  // `isLspFile` is already true, so this needs no second clause.
  return !isAnalyzedFile(path) && !isCargoManifest(path) && isLspFile(path);
}
