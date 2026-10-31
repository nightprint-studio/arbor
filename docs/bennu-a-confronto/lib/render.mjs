// Turns data.json into the page body. Pure functions, no DOM: it runs in Node (build.mjs) and nowhere else, so the
// published page carries no script — a viewer that freezes the DOM or blocks scripts still shows the whole page.
//
// Prose is trusted HTML held in data.json (<code>, <b>, entities as written). Anything that is a number the page also
// computes can be written as a token and is filled in at the end: see `tokens`.

import { board, profiles, stats } from './board.mjs';

const attr = (name, value) => (value ? ` ${name}="${value}"` : '');
const list = (items, one) => items.map(one).join('\n');

// --- blocks: one function per `t`. A new kind of block is one entry here and one rule in style.css. ---------------------

const cell = (c, tag = 'td') => {
  const o = typeof c === 'string' ? { h: c } : c;
  return `<${tag}${attr('class', o.cls)}${attr('colspan', o.colspan)}>${o.h}</${tag}>`;
};

const GLYPH = { y: ['✓', 'y'], p: ['◐', 'p'], n: ['✗', 'n'], q: ['?', 'p'] };

const matrixRow = (cols, r) => {
  if (r.group !== undefined) return `<tr class="grp"><td colspan="${cols.length + 1}">${r.group}</td></tr>`;
  if (r.note !== undefined) return `<tr class="note"><td colspan="${cols.length + 1}">${r.note}</td></tr>`;
  const tag = r.tag ? ` <span class="tag new">${r.tag}</span>` : '';
  const why = r.why ? `<small class="why">${r.why}</small>` : '';
  const marks = [...r.v].map((code, i) => {
    const [glyph, tone] = GLYPH[code];
    return `<td class="c${cols[i].own ? ' own' : ''} ${tone}">${glyph}</td>`;
  });
  return `<tr${attr('class', r.chg && 'chg')}><td>${r.f}${tag}${why}</td>${marks.join('')}</tr>`;
};

export const BLOCKS = {
  p: (b) => `<p${attr('class', b.cls)}>${b.html}</p>`,
  h3: (b) => `<h3>${b.html}</h3>`,
  callout: (b) => `<div class="callout${b.tone ? ` ${b.tone}` : ''}">${b.html}</div>`,
  legend: (b) => `<div class="legend">${b.html}</div>`,
  list: (b) => `<ul${attr('class', b.cls)}>\n${list(b.items, (i) => `<li>${i}</li>`)}\n</ul>`,
  verdict: (b) => `<div class="verdict3">\n${list(b.items, (i) => `<div>${i}</div>`)}\n</div>`,
  facts: (b) => `<div class="facts">\n${list(b.items, (i) => `<div><b>${i.b}</b><small>${i.small}</small></div>`)}\n</div>`,
  steps: (b) => `<ol class="next">\n${list(b.items, (i) => `<li><div><b>${i.b}</b><span>${i.span}</span></div></li>`)}\n</ol>`,

  details: (b, ctx) => `<details${attr('class', b.cls)}>\n<summary>${b.summary}</summary>\n${blocks(b.blocks, ctx)}\n</details>`,

  axes: (b) =>
    `<div class="axes">\n${list(
      b.items,
      (a) => `<article class="axis">\n<header><b>${a.title}</b><span${attr('class', a.tone)}>${a.delta}</span></header>\n${a.html}\n</article>`,
    )}\n</div>`,

  table: (b) => {
    const head = b.head ? `<thead>\n${list(b.head, (tr) => `<tr>${tr.map((c) => cell(c, 'th')).join('')}</tr>`)}\n</thead>` : '';
    const body = `<tbody>\n${list(b.rows, (r) => `<tr${attr('class', r.cls)}>${r.cells.map((c) => cell(c)).join('')}</tr>`)}\n</tbody>`;
    const caption = b.caption ? `\n${b.caption}` : '';
    return `<div class="tw"><table${attr('id', b.id)}${attr('class', b.cls)}>${caption}\n${head}\n${body}\n</table></div>`;
  },

  // Rows are `f` (the feature), an optional `why`, an optional `tag` (the label of the draft that added it) and `v`, one
  // letter per column: y = ✓, p = ◐, n = ✗, q = ? (nothing reliable found).
  matrix: (b) => {
    const cols = b.cols.map((c) => `<th${attr('class', c.own && 'own')}>${c.h}</th>`).join('');
    return `<div class="tw"><table>\n<thead><tr><th>Funzione</th>${cols}</tr></thead>\n<tbody>\n${list(b.rows, (r) => matrixRow(b.cols, r))}\n</tbody>\n</table></div>`;
  },

  chart: (b) => {
    const delta = (r) => (r.now === r.prev ? 'invariato' : `da ${r.prev.toFixed(1)} a ${r.now.toFixed(1)}`);
    const aria = `${b.aria}: ${b.rows.map((r) => `${r.aria} ${delta(r)}`).join(', ')}`;
    const grid = [20, 40, 60, 80].map((p) => `<span class="grid" style="left:${p}%"></span>`).join('');
    const rows = list(b.rows, (r) => {
      const up = r.now > r.prev ? ` <span class="up">+${(r.now - r.prev).toFixed(1)}</span>` : '';
      return (
        `<div class="crow"><span class="lab">${r.lab}</span><div class="track"><span class="prev" style="width:${r.prev * 10}%"></span>` +
        `<span class="now" style="width:${r.now * 10}%"></span>${grid}</div>` +
        `<span class="val">${r.prev.toFixed(1)} → <b>${r.now.toFixed(1)}</b>${up}</span></div>`
      );
    });
    const ticks = [0, 2, 4, 6, 8, 10].map((n) => `<span style="left:${n * 10 === 0 ? '0' : `${n * 10}%`}">${n}</span>`).join('');
    return (
      `<div class="chart" role="img" aria-label="${aria}">\n<div class="chart-title">${b.title}</div>\n${rows}\n` +
      `<div class="scale"><span></span><div class="ticks">${ticks}</div><span></span></div>\n` +
      `<div class="chart-key"><span><i style="background:var(--line)"></i>${b.from}</span><span><i style="background:var(--accent)"></i>${b.to}</span></div>\n</div>`
    );
  },

  // Computed from `scorecard`: the page never holds a total that someone typed.
  board: (b, ctx) => board(ctx.data, b),
  profiles: (b, ctx) => profiles(ctx.data, b),
};

export const blocks = (list_, ctx) =>
  list_
    .map((b) => {
      const render = BLOCKS[b.t];
      if (!render) throw new Error(`data.json: no block of kind "${b.t}"`);
      return render(b, ctx);
    })
    .join('\n');

const section = (s, ctx) =>
  `<section id="${s.id}">\n<div class="wrap">\n<h2>${s.title} <span class="kicker">${s.kicker}</span></h2>\n${blocks(s.blocks, ctx)}\n</div>\n</section>`;

// --- tokens ------------------------------------------------------------------------------------------------------------
//   {{areas}}  {{max}}  {{total:iu}}  {{pct:bennu}}  {{legacy:bennu}} (weighted %, "legacy" or "boot")
const tokens = (data) => {
  const s = stats(data);
  return (key) => {
    const [a, b] = key.split(':');
    if (key === 'areas') return String(s.rows.length);
    if (key === 'max') return String(s.max);
    if (a === 'total') return String(s.totals[b]);
    if (a === 'pct') return String(s.pct[b]);
    if (a === 'legacy' || a === 'boot') return String(s.weighted[a][b]);
    throw new Error(`unknown token {{${key}}}`);
  };
};

const css = (text) => `<style>\n${text}</style>`;

export const renderPage = (data, styleText) => {
  const ctx = { data };
  const m = data.meta;
  const rail = data.sections.map((s) => `<a href="#${s.id}">${s.nav}</a>`).join('\n');
  const page = [
    `<title>${m.title}</title>`,
    '<link rel="preconnect" href="https://fonts.googleapis.com">',
    '<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>',
    `<link rel="stylesheet" href="${m.fonts}">`,
    css(styleText),
    '',
    `<header class="masthead">\n<div class="wrap">\n<div class="eyebrow">${m.eyebrow}</div>\n<h1>${m.h1}</h1>\n<p class="standfirst">${m.standfirst}</p>\n</div>\n</header>`,
    `<nav class="rail" aria-label="Sezioni">\n<div class="wrap rail-inner">\n${rail}\n</div>\n</nav>`,
    ...data.sections.map((s) => section(s, ctx)),
    `<footer>\n<div class="wrap">\n${list(m.footer, (p) => `<p>${p}</p>`)}\n</div>\n</footer>`,
  ].join('\n');
  const fill = tokens(data);
  return page.replace(/\{\{([^}]+)\}\}/g, (_, key) => fill(key.trim())) + '\n';
};
