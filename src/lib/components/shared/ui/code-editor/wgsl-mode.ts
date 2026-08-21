/**
 * WGSL highlighting — the CodeMirror stream mode, and nothing else.
 *
 * CodeMirror ships no WGSL mode and the neighbouring ones are each wrong in a way that
 * shows: the C-family modes know nothing about `@group(0) @binding(0)` (which is most of
 * what the top of a shader is) and paint `vec4<f32>` as a comparison, while the Rust mode
 * colours `fn` and `let` correctly and then calls `textureSample` a plain identifier and
 * `f32` nothing at all.
 *
 * So the three distinctions this draws, which are the ones a shader is read by:
 *
 *   * **attributes** (`@vertex`, `@group`, `@location`) apart from everything else, because
 *     they are the interface — where the stage starts and which slot a binding sits in;
 *   * **types** apart from names, since `vec4<f32>` and `texture_2d<f32>` are half the
 *     tokens in a fragment shader and reading them as identifiers flattens it;
 *   * **the standard library** (`textureSample`, `mix`, `dot`) apart from the functions you
 *     wrote, which is how you tell at a glance what a line is doing.
 *
 * Token names are the legacy-mode vocabulary the editor's injection host already maps onto
 * its own classes; nothing here needs to know what colour anything ends up.
 */

import { StreamLanguage, type StreamParser } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import type { Extension } from '@codemirror/state';

const KEYWORDS = new Set([
  'fn', 'let', 'var', 'const', 'override', 'struct', 'alias', 'return', 'if', 'else',
  'loop', 'for', 'while', 'break', 'continue', 'continuing', 'switch', 'case', 'default',
  'discard', 'enable', 'requires', 'diagnostic', 'else_if', 'fallthrough',
]);

const ATOMS = new Set(['true', 'false']);

/** Address spaces, access modes and the `@builtin(…)` values — the closed vocabulary that
 *  appears inside angle brackets and attribute parentheses. */
const QUALIFIERS = new Set([
  'function', 'private', 'workgroup', 'uniform', 'storage', 'read', 'write', 'read_write',
  'position', 'vertex_index', 'instance_index', 'front_facing', 'frag_depth',
  'local_invocation_id', 'local_invocation_index', 'global_invocation_id', 'workgroup_id',
  'num_workgroups', 'sample_index', 'sample_mask', 'perspective', 'linear', 'flat',
  'center', 'centroid', 'sample',
]);

/** The type vocabulary. Prefix-matched for the families whose names are open-ended
 *  (`texture_*`, `mat<N>x<M>`, the `vecNf` shorthands). */
const TYPES = new Set([
  'bool', 'i32', 'u32', 'f32', 'f16', 'sampler', 'sampler_comparison',
  'array', 'atomic', 'ptr',
  'vec2', 'vec3', 'vec4',
  'vec2f', 'vec3f', 'vec4f', 'vec2i', 'vec3i', 'vec4i', 'vec2u', 'vec3u', 'vec4u',
  'vec2h', 'vec3h', 'vec4h',
]);

function isType(word: string): boolean {
  if (TYPES.has(word)) return true;
  if (word.startsWith('texture_')) return true;
  return /^mat[2-4]x[2-4][fhi]?$/.test(word);
}

/** The standard library, as a prefix test rather than a list of two hundred names: every
 *  WGSL builtin that is not one of the short maths functions below begins with one of these,
 *  and a closed list would be wrong the day the spec grows. */
const BUILTIN_PREFIXES = ['texture', 'atomic', 'subgroup', 'pack', 'unpack', 'quad'];
const BUILTIN_FUNCTIONS = new Set([
  'abs', 'acos', 'acosh', 'asin', 'asinh', 'atan', 'atanh', 'atan2', 'ceil', 'clamp',
  'cos', 'cosh', 'countLeadingZeros', 'countOneBits', 'countTrailingZeros', 'cross',
  'degrees', 'determinant', 'distance', 'dot', 'exp', 'exp2', 'extractBits',
  'faceForward', 'firstLeadingBit', 'firstTrailingBit', 'floor', 'fma', 'fract', 'frexp',
  'insertBits', 'inverseSqrt', 'ldexp', 'length', 'log', 'log2', 'max', 'min', 'mix',
  'modf', 'normalize', 'pow', 'radians', 'reflect', 'refract', 'reverseBits', 'round',
  'saturate', 'select', 'sign', 'sin', 'sinh', 'smoothstep', 'sqrt', 'step', 'tan',
  'tanh', 'transpose', 'trunc', 'all', 'any', 'arrayLength', 'bitcast', 'dpdx', 'dpdxCoarse',
  'dpdxFine', 'dpdy', 'dpdyCoarse', 'dpdyFine', 'fwidth', 'fwidthCoarse', 'fwidthFine',
  'workgroupBarrier', 'storageBarrier', 'textureBarrier', 'workgroupUniformLoad',
]);

function isBuiltinFunction(word: string): boolean {
  return BUILTIN_FUNCTIONS.has(word) || BUILTIN_PREFIXES.some((p) => word.startsWith(p));
}

/** Words after which the next identifier is a **declaration** rather than a mention. */
const DECLARES = new Set(['fn', 'struct', 'alias', 'let', 'var', 'const', 'override']);

interface WgslState {
  /** Depth of `/* … *\/` nesting — WGSL block comments nest, and counting is the only way
   *  an inner `*\/` does not end the outer one. */
  comment: number;
  /** Brace depth inside a multi-line `#import pkg::{ … }`.
   *
   *  naga_oil's braced import spans lines, and without this only its first line reads as a
   *  directive: the names inside it are then coloured as if they were code in this file,
   *  which is the opposite of what they are — they are what this file is borrowing. */
  importDepth: number;
  /** Set when the previous word declares a name, so the next identifier is the thing being
   *  declared. Survives `var<uniform>`'s qualifiers, which sit between the two. */
  expectDecl: boolean;
}

export const wgslMode: StreamParser<WgslState> = {
  name: 'wgsl',

  startState: () => ({ comment: 0, importDepth: 0, expectDecl: false }),

  token(stream, state) {
    // The body of a braced `#import`, which is not code in this file.
    if (state.importDepth > 0) {
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === '{') state.importDepth += 1;
        else if (ch === '}') {
          state.importDepth -= 1;
          if (state.importDepth === 0) break;
        }
      }
      return 'meta';
    }
    if (state.comment > 0) {
      while (!stream.eol()) {
        if (stream.match(/^\/\*/)) { state.comment += 1; continue; }
        if (stream.match(/^\*\//)) {
          state.comment -= 1;
          if (state.comment === 0) break;
          continue;
        }
        stream.next();
      }
      return 'comment';
    }
    if (stream.eatSpace()) return null;

    // A `// @preview` line is a comment to the compiler and a declaration to the preview: it
    // is where a `vec4` packing four unrelated quantities says which lane is which, since WGSL
    // gives the four of them one name and `naga` rejects any attribute that would carry more.
    // Marked apart so it reads as something that DOES something — a comment that is load-
    // bearing and looks like prose is one people delete while tidying.
    if (stream.match(/^\/\/\s*@preview\b/)) { stream.skipToEnd(); return 'preview'; }
    if (stream.match(/^\/\//)) { stream.skipToEnd(); return 'comment'; }
    if (stream.match(/^\/\*/)) { state.comment = 1; return 'comment'; }

    // naga_oil's preprocessor. Not WGSL at all, which is exactly why it is marked as
    // something else: a Bevy shader's `#import` lines are directives to a composer, and
    // colouring them like code hides that the file is a fragment of a larger module.
    if (stream.sol() && stream.match(/^\s*#[A-Za-z_]*/)) {
      const from = stream.pos;
      stream.skipToEnd();
      // `#import pkg::{` opens a block that runs over the following lines. Counted rather
      // than assumed to close on this one — the braced form is written across several.
      const rest = stream.string.slice(from);
      state.importDepth += (rest.match(/\{/g) ?? []).length - (rest.match(/\}/g) ?? []).length;
      if (state.importDepth < 0) state.importDepth = 0;
      return 'meta';
    }
    // `#{SHADER_DEF}` — naga_oil substituting a value in mid-expression, e.g. inside an
    // `@group(…)`. Not at line start, so the rule above never sees it.
    if (stream.match(/^#\{[^}]*\}?/)) return 'meta';

    // `@vertex`, `@group`, `@workgroup_size` — the interface of the shader.
    if (stream.match(/^@[A-Za-z_]\w*/)) return 'attribute';

    if (stream.match(/^0[xX][0-9a-fA-F]+[iuh]?/)) return 'number';
    if (stream.match(/^\d+\.\d*([eE][+-]?\d+)?[fh]?/)) return 'number';
    if (stream.match(/^\.\d+([eE][+-]?\d+)?[fh]?/)) return 'number';
    if (stream.match(/^\d+([eE][+-]?\d+)?[fhiu]?/)) return 'number';

    if (stream.match(/^(->|&&|\|\||<<|>>|[+\-*/%<>=!&|^~]=?)/)) return 'operator';
    if (stream.match(/^[()[\]{}]/)) return 'bracket';
    if (stream.match(/^[;,:.]/)) return 'punct';

    const word = stream.match(/^[A-Za-z_]\w*/) as RegExpMatchArray | null;
    if (word) {
      const w = word[0];
      if (KEYWORDS.has(w)) {
        state.expectDecl = DECLARES.has(w);
        return 'keyword';
      }
      if (ATOMS.has(w)) return 'atom';
      // A qualifier sits BETWEEN `var` and the name it declares (`var<uniform> params`), so
      // it must not consume the expectation the way an ordinary identifier does.
      if (QUALIFIERS.has(w)) return 'atom';
      if (isType(w)) return 'type';
      if (state.expectDecl) {
        state.expectDecl = false;
        return 'def';
      }
      if (isBuiltinFunction(w)) return 'builtin';
      const after = stream.string.slice(stream.pos);
      // A name followed by `:` is being given a type — a struct member, a parameter, an
      // annotated binding. In a shader that is most of the left-hand column, and leaving it
      // the same colour as every other identifier is what makes a file read as a wall.
      if (/^\s*:/.test(after)) return 'property';
      // A name applied to something is a call or a constructor; everything else is a value,
      // and a value is left in the editor's ordinary text colour rather than given one of
      // its own — a file where everything is highlighted is a file where nothing is.
      return /^\s*\(/.test(after) ? 'callee' : null;
    }

    stream.next();
    return null;
  },

  // Two names the CM5 vocabulary has no entry for, so they are declared against real tags:
  // a call reads like a call, and punctuation recedes instead of competing with the code.
  tokenTable: {
    callee: t.function(t.variableName),
    punct: t.punctuation,
    // Against `meta`, which is what a theme already colours for annotations and pragmas —
    // visible as machine-read rather than shouting like a keyword.
    preview: t.meta,
  },

  languageData: {
    commentTokens: { line: '//', block: { open: '/*', close: '*/' } },
    closeBrackets: { brackets: ['(', '[', '{'] },
    indentOnInput: /^\s*[)\]}]$/,
  },
};

/** The WGSL language extension, allocated once — a fresh `StreamLanguage` per mount would
 *  reconfigure the editor for nothing. */
export const wgslLanguageExtension: Extension = StreamLanguage.define(wgslMode);
