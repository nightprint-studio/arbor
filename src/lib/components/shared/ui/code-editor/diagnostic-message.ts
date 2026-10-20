/**
 * How a diagnostic's message is drawn in the lint tooltip.
 *
 * Backends write messages as plain sentences with `backticks` around anything that is code — a
 * coordinate, a version, an identifier. Shown raw the backticks are noise and the code is not
 * told apart from the prose, so they become `<code>` chips. Plain text only: every string goes in
 * through `textContent`, so a message quoting somebody's source cannot become markup.
 */

const CODE = /`([^`\n]+)`/g;

/** The message as DOM, or `null` when it has no code spans — the lint plugin then draws it itself. */
export function renderDiagnosticMessage(message: string): Node | null {
  if (!message.includes('`')) return null;
  const root = document.createElement('span');
  let last = 0;
  for (const m of message.matchAll(CODE)) {
    const at = m.index ?? 0;
    if (at > last) root.append(document.createTextNode(message.slice(last, at)));
    const code = document.createElement('code');
    code.className = 'cm-diag-code';
    code.textContent = m[1];
    root.append(code);
    last = at + m[0].length;
  }
  if (last < message.length) root.append(document.createTextNode(message.slice(last)));
  return root;
}
