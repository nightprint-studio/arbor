/**
 * Prism grammar for **WGSL** (WebGPU Shading Language).
 *
 * Prism ships GLSL and not this, and the two are close enough to be misleading: WGSL declares
 * with `fn` / `let` / `var`, spells its types `vec4<f32>`, and hangs `@group(0) @binding(1)` in
 * front of everything — under the GLSL grammar all of that is plain text, which is most of what
 * a shader snippet in a document is made of.
 *
 * Bennu highlights a `.wgsl` buffer from its own mode; this is for the fenced block.
 */

import Prism from 'prismjs';

Prism.languages.wgsl = {
  comment: { pattern: /\/\/.*|\/\*[\s\S]*?\*\//, greedy: true },
  // `@vertex`, `@group(0)` — the attribute is the shape of a WGSL file, and it belongs to the
  // declaration below it rather than to the expression beside it.
  annotation: { pattern: /@[A-Za-z_]\w*/, alias: 'attr-name' },
  'class-name': /\b(?:vec[234]|mat[234]x[234]|atomic|array|ptr|texture_\w+|sampler(?:_comparison)?)\b/,
  builtin: /\b(?:[iu]32|f16|f32|bool)\b/,
  keyword:
    /\b(?:alias|break|case|const|const_assert|continue|continuing|default|diagnostic|discard|else|enable|fallthrough|fn|for|if|let|loop|override|requires|return|struct|switch|var|while)\b/,
  'storage-class': {
    pattern: /\b(?:uniform|storage|workgroup|private|function|read|write|read_write)\b/,
    alias: 'keyword',
  },
  boolean: /\b(?:true|false)\b/,
  function: /\b[A-Za-z_]\w*(?=\s*[(<])/,
  number: /\b0[xX][\da-fA-F]+[iuhf]?\b|\b\d+(?:\.\d*)?(?:[eE][-+]?\d+)?[iuhf]?\b/,
  operator: /->|&&|\|\||[<>]=?|[-+*/%!=&|^~]=?/,
  punctuation: /[(){}[\],;:.]/,
};
