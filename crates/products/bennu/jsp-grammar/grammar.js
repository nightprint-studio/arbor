/**
 * Tree-sitter grammar for JSP (JavaServer Pages) — Bennu's Java-editor highlighter.
 *
 * Goal: robust, forgiving highlighting for legacy Struts/Entando JSP (namespaced taglib
 * tags `<s:iterator>`, `<c:if>`, `<wp:action>`, scriptlets, EL `${…}`, deferred EL `#{…}`,
 * Struts OGNL `%{…}`) — the exact shapes `@codemirror/lang-html` mis-tags. This is FE-only
 * (compiled to `static/bennu/tree-sitter-jsp.wasm`); there is no Rust consumer, so no Cargo
 * crate — just `grammar.js` + the generated `src/`.
 *
 * Design notes:
 *   - The `<% … %>` family + `<%-- … --%>` comments are single LEAF tokens (the highlighter
 *     colours whole leaves), disambiguated by LEXICAL PRECEDENCE (comment > directive /
 *     declaration / expression > scriptlet) since they all share the `<%` prefix and match
 *     the same block length. Their bodies are matched with a "run of chars that isn't the
 *     terminator" pattern — no lazy quantifiers (tree-sitter's lexer has none) and no
 *     external C scanner.
 *   - Tags are STRUCTURED (`tag_name`, `attribute_name`, quoted values) so each part
 *     colours distinctly, and EL/OGNL inside an attribute value are their own leaves.
 *   - `extras: []` (no auto-skipped whitespace): tag whitespace is explicit (`_ws`), and
 *     `text` includes its own whitespace — deterministic, the tree-sitter-html approach.
 *   - Forgiving: a stray `<` that starts no construct is a `text` leaf; an unterminated
 *     block simply doesn't match and falls back to text — one bad line never breaks the file.
 */

// Body of a `<% … %>` block: any run of chars that doesn't contain the `%>` terminator.
const scriptletBody = repeat(choice(/[^%]/, /%[^>]/));
// Body of a `<%-- … --%>` comment / `<!-- … -->`: any run not containing the terminator.
const jspCommentBody = repeat(choice(/[^-]/, /-[^-]/, /--[^%]/, /--%[^>]/));
const htmlCommentBody = repeat(choice(/[^-]/, /-[^-]/, /--[^>]/));

module.exports = grammar({
  name: 'jsp',

  // No auto-skipped whitespace: tag whitespace is explicit and text keeps its own.
  extras: () => [],

  rules: {
    document: $ => repeat($._node),

    _node: $ => choice(
      $.jsp_comment,
      $.jsp_directive,
      $.jsp_declaration,
      $.jsp_expression,
      $.jsp_scriptlet,
      $.html_comment,
      $.cdata,
      $.doctype,
      $.script_element,
      $.style_element,
      $.end_tag,
      $.self_closing_tag,
      $.start_tag,
      $.el_expression,
      $.ognl_expression,
      $.text,
      $.stray,
    ),

    // ── JSP blocks (single leaf tokens, precedence-disambiguated) ─────────────
    jsp_comment:     _ => token(prec(5, seq('<%--', jspCommentBody, '--%>'))),
    jsp_directive:   _ => token(prec(4, seq('<%@', scriptletBody, '%>'))),
    jsp_declaration: _ => token(prec(4, seq('<%!', scriptletBody, '%>'))),
    jsp_expression:  _ => token(prec(4, seq('<%=', scriptletBody, '%>'))),
    jsp_scriptlet:   _ => token(prec(1, seq('<%', scriptletBody, '%>'))),

    html_comment: _ => token(prec(3, seq('<!--', htmlCommentBody, '-->'))),
    cdata:        _ => token(prec(3, seq('<![CDATA[', repeat(choice(/[^\]]/, /\][^\]]/, /\]\][^>]/)), ']]>'))),
    doctype:      _ => token(prec(2, seq('<!', /[^>]*/, '>'))),

    // ── Expression languages (leaf tokens) ────────────────────────────────────
    // EL `${ … }` and deferred EL `#{ … }`; Struts OGNL `%{ … }`. Bodies stop at `}`.
    el_expression:   _ => token(seq(choice('${', '#{'), repeat(/[^}]/), '}')),
    ognl_expression: _ => token(seq('%{', repeat(/[^}]/), '}')),

    // ── Raw-text elements (<script> / <style>) ────────────────────────────────
    // Their content is NOT markup — a `<` inside JS (`i<count`) or CSS is text, so we
    // consume it as one `raw_text` run up to the matching close tag (case-insensitive),
    // aliased to `script_content` / `style_content` so the highlighter can inject JS/CSS.
    script_element: $ => seq(
      '<', $.script_tag, repeat(seq($._ws, $.attribute)), optional($._ws), '>',
      optional(alias($.raw_text, $.script_content)),
      '</', $.script_tag, optional($._ws), '>',
    ),
    style_element: $ => seq(
      '<', $.style_tag, repeat(seq($._ws, $.attribute)), optional($._ws), '>',
      optional(alias($.raw_text, $.style_content)),
      '</', $.style_tag, optional($._ws), '>',
    ),
    // `script` / `style` (case-insensitive), higher lexical prec than a generic tag_name.
    script_tag: _ => token(prec(3, /[sS][cC][rR][iI][pP][tT]/)),
    style_tag: _ => token(prec(3, /[sS][tT][yY][lL][eE]/)),
    // A run of raw content up to the next `</` (the close tag). A `<` not before `/`
    // (e.g. `i<count`, `a < b`) stays inside the run.
    raw_text: _ => token(prec(-1, repeat1(choice(/[^<]/, /<[^/]/)))),

    // ── Tags (structured) ─────────────────────────────────────────────────────
    // Whitespace is the SEPARATOR before each attribute (and before the close), so the
    // "attribute with a value" vs "next attribute" ambiguity never arises.
    start_tag: $ => seq('<', $.tag_name, repeat(seq($._ws, $.attribute)), optional($._ws), '>'),
    self_closing_tag: $ => seq('<', $.tag_name, repeat(seq($._ws, $.attribute)), optional($._ws), '/>'),
    end_tag: $ => seq('</', $.tag_name, optional($._ws), '>'),

    // Plain or namespaced tag name (`div`, `s:iterator`, `jsp:include`).
    tag_name: _ => token(/[a-zA-Z][a-zA-Z0-9.\-]*(:[a-zA-Z][a-zA-Z0-9.\-]*)?/),

    // `name` or `name="value"` (no whitespace around `=` — standard in these trees).
    attribute: $ => seq(
      $.attribute_name,
      optional(seq('=', $._attribute_value)),
    ),
    // Attribute names may be namespaced / EL-ish (`aria-label`, `s:if`, `data-x`).
    attribute_name: _ => token(/[a-zA-Z_:@][a-zA-Z0-9_:.\-]*/),

    _attribute_value: $ => choice(
      $.quoted_value_double,
      $.quoted_value_single,
      $.unquoted_value,
    ),
    // A quoted attribute value may embed a nested JSP construct — a namespaced taglib tag
    // (Entando `<wp:action path="…"/>`, `<c:url value="…"/>`), a scriptlet/expression
    // (`value="<%= foo %>"`) or a comment — whose OWN quotes must not close the outer value.
    // So the value is a run of text fragments interleaved with those nested constructs.
    quoted_value_double: $ => seq(
      '"',
      repeat(choice(
        $.el_expression, $.ognl_expression,
        $.jsp_comment, $.jsp_expression, $.jsp_scriptlet,
        $.self_closing_tag, $.start_tag, $.end_tag,
        $.attribute_fragment,
      )),
      '"',
    ),
    quoted_value_single: $ => seq(
      "'",
      repeat(choice(
        $.el_expression, $.ognl_expression,
        $.jsp_comment, $.jsp_expression, $.jsp_scriptlet,
        $.self_closing_tag, $.start_tag, $.end_tag,
        $.attribute_fragment_sq,
      )),
      "'",
    ),
    // A run of value text that is neither the closing quote, an EL/OGNL start, nor the start
    // of a nested construct. A `<` that begins a tag / `<%` block / `<!` comment breaks the
    // run (so the nested-construct rule takes over); a stray `<` (before whitespace, a digit,
    // etc. — but NOT the closing quote) stays in the run. Named (not hidden) so the
    // highlighter's leaf classifier colours it as a string.
    attribute_fragment: _ => token.immediate(prec(1, /([^"$%#<]|\$[^{]|%[^{]|#[^{]|<[^a-zA-Z/%!"])+/)),
    attribute_fragment_sq: _ => token.immediate(prec(1, /([^'$%#<]|\$[^{]|%[^{]|#[^{]|<[^a-zA-Z/%!'])+/)),
    unquoted_value: _ => token.immediate(/[^\s'">][^\s>]*/),

    // Whitespace inside tags (hidden helper leaf).
    _ws: _ => token(/[ \t\r\n]+/),

    // ── Text + fallback ───────────────────────────────────────────────────────
    // Free text: a run of chars that starts no construct. `<`, and an EL/OGNL start
    // (`${`, `#{`, `%{`) break the run; a lone `$`/`#`/`%` stays in the text.
    text: _ => token(prec(-1, repeat1(choice(
      /[^<$#%]+/,
      // A `<` that begins no tag / jsp block / comment / close tag (a stray `<` in text,
      // e.g. `a < b`) — absorbed here so it never errors. A real `<div`/`</`/`<%`/`<!`
      // isn't matched (the char after `<` is a letter / `/` / `%` / `!`), so tags win.
      /<[^a-zA-Z%!/]/,
      /\$[^{<]/, /#[^{<]/, /%[^{<]/,
    )))),
    // A `<` (or `$`/`#`/`%`) that begins no valid construct — kept as a leaf so a
    // malformed fragment never produces an ERROR node that poisons highlighting.
    stray: _ => token(prec(-2, /[<$#%]/)),
  },
});
