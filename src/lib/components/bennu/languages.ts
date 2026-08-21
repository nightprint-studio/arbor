/**
 * Bennu editor language registry — picks the right {@link LanguageDescriptor} for a
 * file by extension.
 *
 * Three tiers, in descending order of what they can do:
 *
 * 1. **tree-sitter grammars** — Java ({@link javaLanguage}: semantic highlight, folding,
 *    go-to, backend completion), JSP ({@link jspLanguage}: namespaced taglibs,
 *    scriptlets, EL/OGNL), geode's `.dig` ({@link digLanguage}: highlight, folding,
 *    and completion/hover over its closed vocabulary — all local, no backend) and
 *    merula's `.merula` ({@link merulaLanguage}: highlight + folding, sharing the very
 *    grammar wasm the Merula window parses with).
 * 2. **Lezer languages** — HTML (`@codemirror/lang-html`, with embedded JS/CSS and tag
 *    folding), JSON, Markdown.
 * 3. **language-server backed** — **Rust** ({@link lspLanguage}), plus **TypeScript**,
 *    **JavaScript**, **Svelte** and **HTML** ({@link lspLanguageFrom}): a base highlighter for
 *    the instant local colour — a legacy stream mode for Rust, a real Lezer grammar for the
 *    JS family and the two markup ones — plus completion and hover from the server, and
 *    semantic tokens layered on top by the editor host. Go-to, find-usages, diagnostics and
 *    rename ride the shared handlers, so they need nothing here. HTML is in this tier for
 *    **Angular**, whose server serves a project's templates; where no server serves the file
 *    the two hooks answer nothing and the tier costs nothing.
 * 4. **legacy stream modes** — XML, YAML, `.properties`, CSS/SCSS/LESS, shell and
 *    **TOML**, plus SQL through the shared per-dialect modes. Colour only — except a
 *    `Cargo.toml`, which gets the manifest schema's completion and diagnostics on top
 *    ({@link cargoTomlLang}).
 *
 * Unknown types get a plain (no-highlight) descriptor so they're still fully editable.
 *
 * A language moves between tiers 3 and 4 by changing one line here: the descriptor is the seam,
 * which is what made adding Rust intelligence a new module rather than a rewrite of this one.
 */

import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import {
  sqlHighlight, dtdLanguage, ronLanguageExtension, wgslLanguageExtension,
  type SqlDialect,
} from '$lib/components/shared/ui/code-editor';
import type { Extension } from '@codemirror/state';
import { StreamLanguage, type StreamParser } from '@codemirror/language';
import { xml } from '@codemirror/legacy-modes/mode/xml';
import { css, sCSS, less } from '@codemirror/legacy-modes/mode/css';
import { properties } from '@codemirror/legacy-modes/mode/properties';
import { yaml } from '@codemirror/legacy-modes/mode/yaml';
import { rust } from '@codemirror/legacy-modes/mode/rust';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { shell } from '@codemirror/legacy-modes/mode/shell';
import { json as jsonLang } from '@codemirror/lang-json';
import { markdown } from '@codemirror/lang-markdown';
import { html } from '@codemirror/lang-html';
import { javascript } from '@codemirror/lang-javascript';
import { javaLanguage } from './java-lang';
import {
  lspLanguage, lspLanguageFrom, backendCompletionSource, backendHoverSource,
} from './lsp-lang';
import { isSpringPropertyFile, springPropsLang } from './spring-props-lang';
import { cargoTomlLang, isCargoManifest } from './cargo-toml-lang';
import { packageJsonLanguage, isPackageManifest } from './package-json-lang';
import { xmlSchemaLang } from './xml-schema-lang';
import { jspLanguage } from './jsp-lang';
import { digLanguage } from './dig/dig-lang';
import { merulaLanguage } from './merula-lang';
import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

/** A CM-language descriptor: no tree-sitter parser, highlight from `cmExtension`.
 *  `fold` opts into the Lezer fold gutter (only for languages whose grammar carries
 *  `foldNodeProp`, e.g. lang-html / lang-json — not the fold-less legacy modes). */
function cmLang(id: string, ext: Extension, fold = false): LanguageDescriptor {
  return {
    id,
    createParser: () => Promise.reject(new Error(`cm-language:${id} has no tree-sitter parser`)),
    classify: () => null,
    cmExtension: ext,
    cmFold: fold,
  };
}

/**
 * A CodeMirror-highlighted language whose **completion and hover come from the backend**, with
 * no language server anywhere in it.
 *
 * The distinction {@link lspLanguage} draws is not the one that matters here. Completion and
 * hover go through `bennu_completion` / `bennu_hover`, which the backend answers with whichever
 * engine owns the file — so a language Bennu serves itself needs exactly the same two hooks and
 * none of the rest of `lspLanguage` (its semantic-token layer and its `serverFold`, both of
 * which describe a server that is not there).
 *
 * Leaving them off is what took the hover card off shaders while go-to and find-usages worked:
 * those are host gestures routed by path, and these two are language-descriptor hooks. A
 * language can have one and not the other, silently.
 */
function backendIntelLang(id: string, ext: Extension): LanguageDescriptor {
  return {
    ...cmLang(id, ext),
    intel: { completion: backendCompletionSource, hover: backendHoverSource },
  };
}

/** A descriptor from a CodeMirror legacy-mode stream parser. */
function streamLang(id: string, parser: StreamParser<unknown>): LanguageDescriptor {
  return cmLang(id, StreamLanguage.define(parser));
}

// Module-singleton descriptors (built once).
// Same colouring as any XML, plus what a document with a schema behind it can have and a
// generic one cannot: element/attribute/value completion, ghost text, and a hover carrying the
// schema's own documentation. Silent when no schema resolves, which is most files.
const xmlLang = xmlSchemaLang('xml', xml);
/**
 * HTML — the real lang-html tree (embedded JS/CSS highlight + tag folding), **plus** whatever a
 * language server has to say about it.
 *
 * The server, when there is one, is Angular's: `ngserver` serves an Angular project's templates,
 * which is where a component's own properties, its directives and its pipes are completed. In
 * every other project nothing serves `.html`, the two hooks answer nothing, and the file is
 * coloured and folded exactly as it was.
 *
 * Folding stays **Lezer's**, not the server's: lang-html folds tag bodies the instant the file
 * opens, with no process behind it, and `foldingRange` would replace that with a gutter that
 * appears once a server has started — a worse fold gutter in the projects that have a server and
 * no fold gutter at all in the ones that do not.
 */
const htmlLang = lspLanguageFrom('html', html(), { cmFold: true, serverFold: false });

/**
 * Svelte — markup with `<script>` and `<style>` in it, which is exactly what lang-html parses,
 * so the base colour is HTML's tree rather than a flat stream mode. What that base cannot know —
 * that a name in the template is a component, a prop, a store subscription — arrives as the
 * server's **semantic tokens**, layered on top by the editor host, the same two-layer scheme Rust
 * uses.
 *
 * Its template syntax (`{#each}`, `{@html}`, `{expr}`) is left uncoloured by the base tree: a
 * `.svelte` file is not HTML and no HTML grammar can be told that it is. Honest rather than
 * wrong — a real Svelte grammar is a wasm to build and vendor, which is a bigger decision than a
 * language entry.
 *
 * Comment tokens are lang-html's, deliberately unset here: a `.svelte` file needs `//` inside
 * `<script>` and `<!-- -->` in the markup, and only the nested grammar knows which the caret is
 * in. Spelling one of them here would make `Ctrl`/`Cmd`+`/` wrong in half the file.
 */
const svelteLang = lspLanguageFrom('svelte', html(), { cmFold: true, serverFold: false });
// A DTD is not XML — `<!ELEMENT` is a malformed tag to an XML mode — and it is exactly what
// the `.tld`s and the `struts.xml`s of a legacy project are written against, so it gets its
// own mode rather than the closest-looking one.
const dtdLang = cmLang('dtd', dtdLanguage);
// JSP/JSPF/tag files use the custom tree-sitter-jsp grammar (jsp-lang.ts) — namespaced
// taglib tags, scriptlets, EL/OGNL all parse + colour natively.
const cssLang = streamLang('css', css);
const scssLang = streamLang('scss', sCSS);
const lessLang = streamLang('less', less);
/**
 * JavaScript and TypeScript, in four flavours because the grammar has to be told which.
 *
 * A real Lezer grammar rather than the stream tokenizer the JSP `<script>` bodies get. That
 * tokenizer exists because a *fragment* of JavaScript inside markup is regularly not valid on
 * its own and a parser gives up on it (see `js-mode.ts`); a whole `.ts` file is not a fragment,
 * and everything the tokenizer cannot have — `interface` and generics, decorators, JSX,
 * brace folding, real auto-indent — a grammar has for free.
 *
 * Four and not one because `typescript` and `jsx` are parse-time flags: `<T>(x) => x` is a type
 * parameter in TypeScript and an opening tag in JSX, and no single parser can be right about
 * both. The extension picks, which is exactly what the extension means.
 *
 * All four are **server-backed**: `typescript-language-server` serves every one of them, so
 * completion, hover and the resolve-on-highlight documentation come through the shared backend
 * hooks, and its semantic tokens refine the grammar's colouring the way they do for Rust.
 * Folding stays the grammar's — immediate, and right whether or not a server ever starts.
 */
const jsFlavour = (id: string, opts: Parameters<typeof javascript>[0]) =>
  lspLanguageFrom(id, javascript(opts), { cmFold: true, serverFold: false });
const jsLang = jsFlavour('javascript', {});
const jsxLang = jsFlavour('javascriptreact', { jsx: true });
const tsLang = jsFlavour('typescript', { typescript: true });
const tsxLang = jsFlavour('typescriptreact', { typescript: true, jsx: true });
const propsLang = streamLang('properties', properties);
const yamlLang = streamLang('yaml', yaml);
// Same colouring, plus the intelligence a Spring config file can have and a generic one
// cannot: key/value completion, ghost text, and a hover that knows the type. Built once —
// the identity has to be stable or the editor remounts on every keystroke.
const springYamlLang = springPropsLang('spring-yaml', yaml);
const springPropertiesLang = springPropsLang('spring-properties', properties);
const shellLang = streamLang('shell', shell);
// Rust: the legacy mode for the instant local colour, plus everything a language server adds.
// Built once — the identity has to be stable or the editor remounts on every keystroke.
const rustLang = lspLanguage('rust', rust, {
  line: '//',
  block: { open: '/*', close: '*/' },
});
/**
 * RON — geode's content format, what a Bevy asset is written in, and the shape a debugger value
 * is dumped in.
 *
 * It used to borrow the **Rust** mode, on the grounds that RON borrows Rust's syntax. It does,
 * and that is exactly why the result was wrong: RON has none of Rust's vocabulary but very
 * plausibly has fields called `type:`, `mod:` or `ref:`, every one of which came out as a
 * keyword — and the one thing a RON file is mostly made of, the **field names**, had no
 * colour at all. It now has a mode of its own (`ron-mode.ts`).
 *
 * Still **not Rust to a language server**: asking rust-analyzer about a `.ron` would be asking
 * about a file it has never heard of, and a dumped value is not a file at all. One instance is
 * shared by both uses — the identity has to be stable or an editor remounts on every keystroke.
 */
export const ronLanguage = cmLang('ron', ronLanguageExtension);
/**
 * Rust source that is **not a file** — a macro expansion.
 *
 * Same colouring, deliberately none of the intelligence: the server produced the text and does not
 * know it as a document, so completion, hover and go-to would all be asking about something that
 * exists nowhere. Its own descriptor rather than the `.rs` one for exactly that reason.
 */
export const rustTextLanguage = streamLang('rust-text', rust);
/**
 * WGSL — the shader language, and what a Bevy material is written in.
 *
 * A descriptor of its own rather than one borrowed from the C family: `@group(0) @binding(0)`
 * is most of what the top of a shader is and no C-family mode has heard of it, while the
 * Rust mode gets `fn` right and then has nothing to say about `vec4<f32>` or `textureSample`.
 *
 * **Not** language-server backed here, deliberately: `wgsl-analyzer` serves the file when it
 * is installed (it is in the LSP catalogue, and Bennu can install it), and when it is not,
 * the backend answers completion, hover, find-usages and diagnostics itself — naga for the
 * diagnostics, a scanner for the rest. Either way the colour comes from here, and the
 * completion and hover hooks are the same two every other language uses.
 */
const wgslLang = backendIntelLang('wgsl', wgslLanguageExtension);
const tomlLang = streamLang('toml', toml);
// A `Cargo.toml` is TOML the backend has a great deal to say about: the manifest schema behind its
// completion and its diagnostics. Built once — the identity has to be stable or the editor remounts
// on every keystroke.
const cargoManifestLang = cargoTomlLang(toml);
const jsonDesc = cmLang('json', jsonLang());
const markdownDesc = cmLang('markdown', markdown());
const plainLang = cmLang('text', []);

/**
 * SQL descriptors, one per dialect, built on first use and cached.
 *
 * The identity matters: `CodeEditor` builds its extensions from the descriptor at mount,
 * so a fresh object per read would remount the editor on every keystroke. Caching also
 * means switching the dialect setting DOES hand out a different descriptor, which is
 * exactly the remount that makes the new colouring take effect.
 *
 * The dialect is a **setting** and not a detection: a `.sql` file under a Java project's
 * resources carries nothing that says which engine it targets, and the engines disagree
 * about string quoting (`q'[…]'` vs `$$ … $$`), so guessing wrong paints half a file as
 * one broken string. The default, `portable`, uses the rules valid on both. (Picus, which
 * *does* know the engine — it has the connection — resolves it from there instead, over
 * the same shared modes.)
 */
const sqlDescriptors = new Map<SqlDialect, LanguageDescriptor>();
function sqlLangFor(dialect: SqlDialect): LanguageDescriptor {
  const cached = sqlDescriptors.get(dialect);
  if (cached) return cached;
  const desc = cmLang(`sql-${dialect}`, sqlHighlight(dialect));
  sqlDescriptors.set(dialect, desc);
  return desc;
}

/** Resolve the editor language for a file path. Falls back to a plain (no-highlight,
 *  still editable) descriptor for unknown types.
 *
 *  Reads `bennuSettingsStore.sqlDialect` for `.sql` files, so call this from a `$derived`
 *  (as `BennuEditor` does) and the editor re-mounts when the setting changes. */
export function languageForPath(path: string | null): LanguageDescriptor {
  if (!path) return plainLang;
  const name = path.split(/[\\/]/).pop() ?? path;

  // Dot-file configs with no extension.
  if (name === '.gitignore' || name === '.gitattributes' || name === '.editorconfig') return propsLang;
  // `Cargo.lock` is TOML; `.lock` in general is not (`yarn.lock` isn't), so match the name.
  if (name === 'Cargo.lock') return tomlLang;
  // The manifest, by NAME: `rustfmt.toml` and `.cargo/config.toml` are not manifests, and applying
  // the manifest schema to one would flag every key in it.
  if (isCargoManifest(name)) return cargoManifestLang;

  const dot = name.lastIndexOf('.');
  const ext = dot >= 0 ? name.slice(dot + 1).toLowerCase() : '';
  // An `application*.yml` is a YAML file the backend has a great deal to say about; a
  // `messages.properties` is not. The name is the whole test, matching the backend's.
  if (isSpringPropertyFile(name)) {
    return ext === 'properties' ? springPropertiesLang : springYamlLang;
  }
  switch (ext) {
    case 'java': return javaLanguage;
    case 'rs': return rustLang;
    case 'toml': return tomlLang;
    // geode's mole scripts — the one non-Java tree-sitter language here.
    case 'dig': return digLanguage;
    // Merula's own grammar, not a second one — see `merula-lang.ts`. Highlight, folding and
    // comment toggle only: completion and hover need the DSL catalogue, which lives behind
    // `merula-be`, and spawning that backend to open a text file is a bigger decision than
    // colouring one.
    case 'merula': return merulaLanguage;
    case 'xml': case 'xsd': case 'wsdl': case 'xsl': case 'xslt': case 'tld':
    case 'pom': case 'iml': case 'fxml': case 'svg': return xmlLang;
    // `.ent` / `.mod` are the conventional names for a DTD split across files.
    case 'dtd': case 'ent': case 'mod': return dtdLang;
    case 'jsp': case 'jspf': case 'tag': case 'tagx': return jspLanguage;
    case 'html': case 'htm': case 'xhtml': return htmlLang;
    case 'svelte': return svelteLang;
    case 'css': return cssLang;
    case 'scss': return scssLang;
    case 'less': return lessLang;
    case 'js': case 'mjs': case 'cjs': return jsLang;
    case 'jsx': return jsxLang;
    case 'ts': case 'mts': case 'cts': return tsLang;
    case 'tsx': return tsxLang;
    // A `package.json` before any other JSON: it is not a document with a schema, it is a
    // manifest whose sections mean things, and which one it is has to be decided by NAME —
    // `tsconfig.json` given the manifest's rules would have every key in it coloured as a
    // dependency. See `package-json-lang.ts`.
    case 'json': case 'json5':
      return isPackageManifest(path) ? packageJsonLanguage : jsonDesc;
    case 'md': case 'markdown': return markdownDesc;
    case 'yml': case 'yaml': return yamlLang;
    case 'properties': case 'ini': case 'conf': case 'cfg': return propsLang;
    case 'ron': return ronLanguage;
    case 'wgsl': return wgslLang;
    case 'sql': return sqlLangFor(bennuSettingsStore.sqlDialect);
    case 'sh': case 'bash': case 'zsh': return shellLang;
    default: return plainLang;
  }
}
