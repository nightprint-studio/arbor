/**
 * Imber's context as a list. It is stored as one string — a Markdown bullet list — so the profile and the project
 * file keep the shape they had, the prompt reads a tidy list, and a context written before the list editor existed
 * (free text) still loads: each paragraph becomes an item.
 *
 *   - First note
 *   - A second note that runs
 *     over two lines
 */

const BULLET = /^[-*]\s+/;

/** The items a stored context holds. */
export function parseContext(text: string): string[] {
  const lines = text.replace(/\r\n?/g, '\n').split('\n');
  const bulleted = lines.some((l) => BULLET.test(l));
  const items: string[] = [];
  let open = false; // whether the next continuation line still belongs to the last item
  for (const line of lines) {
    if (bulleted) {
      if (BULLET.test(line)) {
        items.push(line.replace(BULLET, ''));
        open = true;
      } else if (open && line.trim() !== '') {
        items[items.length - 1] += `\n${line.replace(/^ {1,2}/, '')}`;
      } else if (line.trim() !== '' && !open) {
        items.push(line.trim());
        open = true;
      } else {
        open = false;
      }
    } else if (line.trim() === '') {
      open = false;
    } else if (open) {
      items[items.length - 1] += `\n${line}`;
    } else {
      items.push(line);
      open = true;
    }
  }
  return items.map((i) => i.trim()).filter(Boolean);
}

/** The string stored for `items`; empty items are dropped. */
export function serializeContext(items: readonly string[]): string {
  return items
    .map((i) => i.trim())
    .filter(Boolean)
    .map((i) => `- ${i.split('\n').join('\n  ')}`)
    .join('\n');
}
