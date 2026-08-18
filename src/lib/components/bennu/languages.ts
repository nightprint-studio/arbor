/**
 * Bennu editor language registry — picks the right {@link LanguageDescriptor} for a
 * file by extension.
 *
 * Three tiers, in descending order of what they can do:
 *
 * 1. **tree-sitter grammars** — Java ({@link javaLanguage}: semantic highlight, folding,
 *    go-to, backend completion), JSP ({@link jspLanguage}: namespaced taglibs,
 *    scriptlets, EL/OGNL) and geode's `.dig` ({@link digLanguage}: highlight, folding,
 *    and completion/hover over its closed vocabulary — all local, no backend).
 * 2. **Lezer languages** — HTML (`@codemirror/lang-html`, with embedded JS/CSS and tag
 *    folding), JSON, Markdown.
 * 3. **language-server backed** — **Rust** ({@link lspLanguage}): a legacy stream mode for the
 *    instant local colour, plus completion / hover / signature help from the server, and
 *    semantic tokens layered over the mode by the editor host. Go-to, find-usages, diagnostics
 *    and rename ride the shared handlers, so they need nothing here.
 * 4. **legacy stream modes** — XML, YAML, `.properties`, CSS/SCSS/LESS, JS/TS, shell and
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
  sqlHighlight, dtdLanguage, javascriptStream, type SqlDialect,
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
import { javaLanguage } from './java-lang';
import { lspLanguage } from './lsp-lang';
import { isSpringPropertyFile, springPropsLang } from './spring-props-lang';
import { cargoTomlLang, isCargoManifest } from './cargo-toml-lang';
import { xmlSchemaLang } from './xml-schema-lang';
import { jspLanguage } from './jsp-lang';
import { digLanguage } from './dig/dig-lang';
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

/** A descriptor from a CodeMirror legacy-mode stream parser. */
function streamLang(id: string, parser: StreamParser<unknown>): LanguageDescriptor {
  return cmLang(id, StreamLanguage.define(parser));
}

// Module-singleton descriptors (built once).
// Same colouring as any XML, plus what a document with a schema behind it can have and a
// generic one cannot: element/attribute/value completion, ghost text, and a hover carrying the
// schema's own documentation. Silent when no schema resolves, which is most files.
const xmlLang = xmlSchemaLang('xml', xml);
// HTML: the real lang-html tree (embedded JS/CSS highlight + tag folding).
const htmlLang = cmLang('html', html(), true);
// A DTD is not XML — `<!ELEMENT` is a malformed tag to an XML mode — and it is exactly what
// the `.tld`s and the `struts.xml`s of a legacy project are written against, so it gets its
// own mode rather than the closest-looking one.
const dtdLang = cmLang('dtd', dtdLanguage);
// JSP/JSPF/tag files use the custom tree-sitter-jsp grammar (jsp-lang.ts) — namespaced
// taglib tags, scriptlets, EL/OGNL all parse + colour natively.
const cssLang = streamLang('css', css);
const scssLang = streamLang('scss', sCSS);
const lessLang = streamLang('less', less);
// The same tokenizer the JSP `<script>` bodies get — object keys, members, call sites, `this`
// and every shape of number, which the CM5 port left flat. See `js-mode.ts`.
const jsLang = streamLang('javascript', javascriptStream as unknown as StreamParser<unknown>);
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
 * RON — geode's content format, and the shape a debugger value is dumped in.
 *
 * Coloured by the Rust mode (same comments, strings, numbers, and the identifier-before-`(` shape)
 * but **not Rust to a language server**: asking rust-analyzer about a `.ron` would be asking about a
 * file it has never heard of, and a dumped value is not a file at all. So it keeps a plain descriptor
 * of its own, and one instance is shared by both uses — the identity has to be stable or an editor
 * remounts on every keystroke.
 */
export const ronLanguage = streamLang('ron', rust);
/**
 * Rust source that is **not a file** — a macro expansion.
 *
 * Same colouring, deliberately none of the intelligence: the server produced the text and does not
 * know it as a document, so completion, hover and go-to would all be asking about something that
 * exists nowhere. Its own descriptor rather than the `.rs` one for exactly that reason.
 */
export const rustTextLanguage = streamLang('rust-text', rust);
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
    case 'xml': case 'xsd': case 'wsdl': case 'xsl': case 'xslt': case 'tld':
    case 'pom': case 'iml': case 'fxml': case 'svg': return xmlLang;
    // `.ent` / `.mod` are the conventional names for a DTD split across files.
    case 'dtd': case 'ent': case 'mod': return dtdLang;
    case 'jsp': case 'jspf': case 'tag': case 'tagx': return jspLanguage;
    case 'html': case 'htm': case 'xhtml': return htmlLang;
    case 'css': return cssLang;
    case 'scss': return scssLang;
    case 'less': return lessLang;
    case 'js': case 'mjs': case 'cjs': case 'jsx':
    case 'ts': case 'tsx': return jsLang;
    case 'json': case 'json5': return jsonDesc;
    case 'md': case 'markdown': return markdownDesc;
    case 'yml': case 'yaml': return yamlLang;
    case 'properties': case 'ini': case 'conf': case 'cfg': return propsLang;
    // RON (geode's content format) is close enough to a struct literal that the Rust mode
    // colours it well: same comments, strings, numbers and identifier-before-`(` shape. Its own
    // descriptor, though — see `ronLang`.
    case 'ron': return ronLanguage;
    case 'sql': return sqlLangFor(bennuSettingsStore.sqlDialect);
    case 'sh': case 'bash': case 'zsh': return shellLang;
    default: return plainLang;
  }
}
