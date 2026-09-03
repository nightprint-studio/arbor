/**
 * Prism grammar for **mermaid** — replacing the one Prism ships.
 *
 * ## Why replace it rather than style it
 *
 * Prism's own mermaid grammar tokenises correctly and then names its tokens things no theme in
 * this application has ever heard of: a node label comes out as `text`, an edge as `arrow`, an
 * edge label as `label`. Nothing styles those, so a diagram rendered with it is one orange
 * keyword and a page of white — which is what it looked like.
 *
 * The alternative was to add `.token.arrow` / `.token.label` / `.token.text` to the global
 * stylesheet, and `text` is the one that stops it: it is a name generic enough that another
 * grammar may emit it for something that is not prose, and a fence carries no language class to
 * scope the rule by. So the grammar is the thing that changes, and it emits the **standard**
 * names the app already colours.
 *
 * ## The same three distinctions as the editor's mode
 *
 * `shared/ui/code-editor/mermaid-mode.ts` colours a `.mmd` buffer, this colours a ```mermaid
 * fence, and they must agree or one file reads two ways: the **arrows** (the edges, and the
 * direction is the meaning), the **labels** (the prose), and the **diagram type** on the first
 * line (which decides what every line under it means). Everything else is a name.
 */

import Prism from 'prismjs';

/** The word that opens a diagram. Only at the start of a line — further down, `graph` is
 *  somebody's node id. */
const DIAGRAM =
  /^[ \t]*(?:flowchart|graph|sequenceDiagram|classDiagram|stateDiagram-v2|stateDiagram|erDiagram|journey|gantt|pie|gitGraph|mindmap|timeline|quadrantChart|requirementDiagram|sankey-beta|xychart-beta|block-beta|packet-beta|architecture-beta|C4(?:Context|Container|Component|Dynamic|Deployment)|zenuml)\b/m;

Prism.languages.mermaid = {
  // `%%{ init: … }%%` is configuration and can span lines; a bare `%%` is a comment. The
  // directive first, or its opening `%%` is eaten as one.
  directive: { pattern: /%%\{[\s\S]*?\}%%/, greedy: true },
  comment: { pattern: /%%.*/, greedy: true },

  // A label — the prose of a diagram. Every bracket shape mermaid gives a node, plus the `|…|`
  // an edge carries. Greedy and before everything else: a label may contain arrows, brackets and
  // HTML (`<br/>`), and each of those would otherwise be tokenised as itself.
  string: [
    {
      pattern: /\|[^|\r\n]*\||\[\[[^\]\r\n]*\]\]|\(\((?:[^)\r\n]|\)(?!\)))*\)\)|\{\{[^}\r\n]*\}\}|\[\((?:[^)\r\n])*\)\]|\(\[[^\]\r\n]*\]\)|\[\/[^\]\r\n]*[/\\]\]|\[[^\]\r\n]*\]|\((?:[^)\r\n])*\)|\{[^}\r\n]*\}|"(?:[^"\\\r\n]|\\.)*"/,
      greedy: true,
    },
    {
      // The asymmetric node (`A>label]`), and it needs the lookbehind: without it the `>` of an
      // arrow starts a label, so `B -->|yes| C[(Ship it)]` came out as `--` and then one string
      // running to the end of the line. A `>` that closes an arrow is never a node shape.
      pattern: /(^|[^-=.<>|])>[^\]\r\n]*\]/,
      lookbehind: true,
      greedy: true,
    },
  ],

  // The diagram type, and then the rest of the reserved vocabulary across the dialects.
  keyword: [
    { pattern: DIAGRAM, greedy: true },
    /\b(?:subgraph|end|direction|namespace|participant|actor|activate|deactivate|loop|alt|else|opt|par|and|critical|option|break|rect|autonumber|box|class|state|note|link|callback|call|href|title|section|dateFormat|axisFormat|excludes|includes|todayMarker|tickInterval|weekday|milestone|style|classDef|linkStyle|click|accTitle|accDescr|default|showData)\b/,
  ],

  // The edges. Longest first — `->>` must not be cut to `->`, `<-->` must not lose its head,
  // and `.->` (the closing half of `A -. text .-> B`) is an arrow rather than a dot and a `>`.
  operator:
    /<<-{1,2}>>|<<-{1,2}|-{1,2}>>|<-{2,3}>|<\|?-{2,3}\|?>|x-{2,3}x|o-{2,3}o|-{1,3}[>xo)]|-\.{1,3}-?[>xo]?|\.{1,3}-+[>xo]?|={2,3}[>xo]?|-{2,3}|\.{2,3}>|~{3}|:::|<\||\|>|\*--|--\*/,

  /** `flowchart LR` — a direction is a value, not a statement. */
  constant: /\b(?:TB|TD|BT|RL|LR)\b/,
  number: /\b\d+(?:\.\d+)?\b/,
  // The node ids and everything else somebody named. Last, so it never steals a keyword.
  variable: /\b[A-Za-z_]\w*\b/,
  punctuation: /[:;,&]/,
};
