/**
 * Bennu editor language registry — picks the right {@link LanguageDescriptor} for a
 * file by extension.
 *
 * Java uses the real tree-sitter descriptor ({@link javaLanguage} — semantic
 * highlight, folding, go-to, completion). Every other file type Bennu opens (XML,
 * JSP/HTML, YAML, `.properties`, JSON, Markdown, CSS/SCSS, JS, SQL, shell) uses a
 * CodeMirror built-in / legacy-mode language via the descriptor's `cmExtension` seam,
 * highlighted by the shared Lezer style. Unknown types get a plain (no-highlight)
 * descriptor so they're still fully editable.
 *
 * All languages here are already dependencies (`@codemirror/legacy-modes`,
 * `lang-json`, `lang-markdown`) — no new libraries.
 */

import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import type { Extension } from '@codemirror/state';
import { StreamLanguage, type StreamParser } from '@codemirror/language';
import { xml, html } from '@codemirror/legacy-modes/mode/xml';
import { css, sCSS, less } from '@codemirror/legacy-modes/mode/css';
import { javascript } from '@codemirror/legacy-modes/mode/javascript';
import { properties } from '@codemirror/legacy-modes/mode/properties';
import { yaml } from '@codemirror/legacy-modes/mode/yaml';
import { standardSQL } from '@codemirror/legacy-modes/mode/sql';
import { shell } from '@codemirror/legacy-modes/mode/shell';
import { json as jsonLang } from '@codemirror/lang-json';
import { markdown } from '@codemirror/lang-markdown';
import { javaLanguage } from './java-lang';
import { jsp } from './jsp-mode';

/** A CM-language descriptor: no tree-sitter parser, highlight from `cmExtension`. */
function cmLang(id: string, ext: Extension): LanguageDescriptor {
  return {
    id,
    createParser: () => Promise.reject(new Error(`cm-language:${id} has no tree-sitter parser`)),
    classify: () => null,
    cmExtension: ext,
  };
}

/** A descriptor from a CodeMirror legacy-mode stream parser. */
function streamLang(id: string, parser: StreamParser<unknown>): LanguageDescriptor {
  return cmLang(id, StreamLanguage.define(parser));
}

// Module-singleton descriptors (built once).
const xmlLang = streamLang('xml', xml);
const htmlLang = streamLang('html', html);
// JSP/JSPF/tag files: the HTML mode plus JSP `<% … %>` scriptlet/comment handling.
const jspLang = streamLang('jsp', jsp);
const cssLang = streamLang('css', css);
const scssLang = streamLang('scss', sCSS);
const lessLang = streamLang('less', less);
const jsLang = streamLang('javascript', javascript);
const propsLang = streamLang('properties', properties);
const yamlLang = streamLang('yaml', yaml);
const sqlLang = streamLang('sql', standardSQL);
const shellLang = streamLang('shell', shell);
const jsonDesc = cmLang('json', jsonLang());
const markdownDesc = cmLang('markdown', markdown());
const plainLang = cmLang('text', []);

/** Resolve the editor language for a file path. Falls back to a plain (no-highlight,
 *  still editable) descriptor for unknown types. */
export function languageForPath(path: string | null): LanguageDescriptor {
  if (!path) return plainLang;
  const name = path.split(/[\\/]/).pop() ?? path;

  // Dot-file configs with no extension.
  if (name === '.gitignore' || name === '.gitattributes' || name === '.editorconfig') return propsLang;

  const dot = name.lastIndexOf('.');
  const ext = dot >= 0 ? name.slice(dot + 1).toLowerCase() : '';
  switch (ext) {
    case 'java': return javaLanguage;
    case 'xml': case 'xsd': case 'wsdl': case 'xsl': case 'xslt': case 'tld':
    case 'pom': case 'iml': case 'fxml': case 'svg': return xmlLang;
    case 'jsp': case 'jspf': case 'tag': case 'tagx': return jspLang;
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
    case 'sql': return sqlLang;
    case 'sh': case 'bash': case 'zsh': return shellLang;
    default: return plainLang;
  }
}
