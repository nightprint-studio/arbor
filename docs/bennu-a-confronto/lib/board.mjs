// The scorecard: every total, subtotal and percentage on the page comes from here, from `scorecard` in data.json.

const TONES = { hi: 'var(--hi)', mid: 'var(--mid)', lo: 'var(--lo)' };

// The best score in the row is "hi", 0–1 is "lo", the rest "mid".
const toneFor = (v, row) => {
  const max = Math.max(...row);
  if (v === max && max > 0) return TONES.hi;
  if (v <= 1) return TONES.lo;
  return TONES.mid;
};

const pips = (v, tone) =>
  '<span class="pips" aria-hidden="true">' +
  [0, 1, 2, 3, 4].map((i) => `<i class="${i < v ? 'on' : ''}" style="--tone:${tone}"></i>`).join('') +
  '</span>';

const scoresOf = (data, row) => data.tools.map((t) => row.scores[t.key]);

export const stats = (data) => {
  const rows = data.scorecard.flatMap((s) => s.rows);
  const max = rows.length * 5;
  const totals = {};
  const pct = {};
  const weighted = { legacy: {}, boot: {} };
  for (const t of data.tools) {
    totals[t.key] = rows.reduce((sum, r) => sum + r.scores[t.key], 0);
    pct[t.key] = Math.round((totals[t.key] / max) * 100);
    for (const w of Object.keys(weighted)) {
      const top = rows.reduce((sum, r) => sum + r.weight[w] * 5, 0);
      weighted[w][t.key] = Math.round((rows.reduce((sum, r) => sum + r.weight[w] * r.scores[t.key], 0) / top) * 100);
    }
  }
  return { rows, max, totals, pct, weighted };
};

export const board = (data, b) => {
  const { rows, max, totals, pct } = stats(data);
  const cols = data.tools.length + 1;
  const own = (t, extra = '') => `${extra}${t.own ? ' own' : ''}`.trim();

  const head = data.tools
    .map(
      (t) =>
        `<th class="${t.own ? 'own' : ''}" style="text-align:center">${t.name}<br>` +
        `<span style="font-family:var(--f-mono);font-size:10px;font-weight:400;color:var(--ink-3)">${t.sub}</span></th>`,
    )
    .join('');

  const body = data.scorecard
    .map((section) => {
      const lines = section.rows.map((row) => {
        const scores = scoresOf(data, row);
        const cells = scores.map((v, i) => {
          const t = data.tools[i];
          let extra = '';
          if (t.own && row.then === null) extra = '<span class="then" style="color:var(--accent)">nuova</span>';
          else if (t.own && row.then !== v) extra = `<span class="then">era ${row.then}</span>`;
          return `<td class="score${t.own ? ' own' : ''}">${pips(v, toneFor(v, scores))}<span class="num">${v}</span>${extra}</td>`;
        });
        return `<tr><td class="feat"><b>${row.area}</b><small class="why">${row.note}</small></td>${cells.join('')}</tr>`;
      });
      const subs = data.tools.map((t) => `<td class="${t.own ? 'own' : ''}">${section.rows.reduce((a, r) => a + r.scores[t.key], 0)}</td>`);
      return (
        `<tr class="grp"><td colspan="${cols}">${section.title}</td></tr>\n${lines.join('\n')}\n` +
        `<tr class="sub"><td>Subtotale · ${section.rows.length} × 5</td>${subs.join('')}</tr>`
      );
    })
    .join('\n');

  const foot = data.tools.map((t) => `<td class="${t.own ? 'own' : ''}">${totals[t.key]}<small>${pct[t.key]}%</small></td>`).join('');

  return (
    `<div class="tw"><table id="board" class="board"><caption class="fine" style="caption-side:bottom;text-align:left;padding:10px 12px">${b.caption}</caption>\n` +
    `<thead><tr><th>Area</th>${head}</tr></thead><tbody>\n${body}\n</tbody>` +
    `<tfoot><tr><td>Totale su ${max}</td>${foot}</tr></tfoot>\n</table></div>`
  );
};

// A weighted profile: how much of the weight each tool covers, best first.
export const profiles = (data, b) => {
  const { weighted } = stats(data);
  const articles = b.items.map((item) => {
    const ranked = data.tools
      .map((t) => ({ t, pct: weighted[item.weight][t.key] }))
      .sort((x, y) => y.pct - x.pct);
    const bars = ranked
      .map(
        ({ t, pct }) =>
          `<div class="prow${t.own ? ' own' : ''}"><span>${t.name} ${t.own ? '' : t.sub}</span>` +
          `<span class="bar"><i style="width:${pct}%"></i></span><span class="pct">${pct}%</span></div>`,
      )
      .join('');
    return (
      `<article class="profile">\n<h4>${item.title}</h4>\n<p class="who">${item.who}</p>\n` +
      `<div id="prof-${item.weight}">${bars}</div>\n<p class="weights">${item.weights}</p>\n</article>`
    );
  });
  return `<div class="profiles">\n${articles.join('\n')}\n</div>`;
};
