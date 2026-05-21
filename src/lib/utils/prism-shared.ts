/**
 * Shared Prism language registrations.
 *
 * Importing this file (for its side-effects) makes every grammar arbor
 * supports available on the global `Prism.languages` map. Used by:
 *   - `diff-formatter.ts` (DiffViewer line highlighting)
 *   - `highlight.ts`      (code blocks in plugin forms, JSON Studio modal)
 *
 * Add new languages here once — both consumers pick them up automatically.
 * Module-level imports in JS/TS are deduplicated by the bundler, so the
 * grammars are loaded a single time even if both consumers are present.
 */

import 'prismjs/components/prism-typescript';
import 'prismjs/components/prism-javascript';
import 'prismjs/components/prism-rust';
import 'prismjs/components/prism-python';
import 'prismjs/components/prism-json';
import 'prismjs/components/prism-css';
import 'prismjs/components/prism-scss';
import 'prismjs/components/prism-bash';
import 'prismjs/components/prism-batch';
import 'prismjs/components/prism-toml';
import 'prismjs/components/prism-markdown';
import 'prismjs/components/prism-yaml';
import 'prismjs/components/prism-java';
import 'prismjs/components/prism-swift';
import 'prismjs/components/prism-go';
import 'prismjs/components/prism-sql';
// `clike` is the shared base several others extend — must come before c/cpp/kotlin/csharp.
import 'prismjs/components/prism-clike';
import 'prismjs/components/prism-c';
import 'prismjs/components/prism-cpp';
import 'prismjs/components/prism-kotlin';
import 'prismjs/components/prism-csharp';
import 'prismjs/components/prism-lua';
import 'prismjs/components/prism-glsl';
import 'prismjs/components/prism-powershell';
import 'prismjs/components/prism-markup';             // HTML / XML / SVG (also the base for markup-templating)
import 'prismjs/components/prism-markup-templating';  // base for template languages (JSP, PHP…)
import 'prismjs/components/prism-docker';

// Custom grammars (Svelte, …). Each module registers itself into
// `Prism.languages` on import.
import './prism-languages';
