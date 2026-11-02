/**
 * The picture the OS shows under the cursor while files are dragged out of an explorer window.
 *
 * The OS drag takes an image file, not a DOM node, so the label ("eclipse", "3 items") is drawn
 * onto a canvas as a pill in the accent colour and handed over as PNG bytes. Drawn at twice the
 * density so it stays sharp on a high-DPI screen.
 */

const FALLBACK_ACCENT = '#4c8dff';
const SCALE = 2;
const HEIGHT = 30;
const PAD_X = 14;
const MAX_WIDTH = 320;

export async function renderDragIcon(label: string): Promise<number[]> {
  const style = getComputedStyle(document.documentElement);
  const accent = style.getPropertyValue('--accent').trim() || FALLBACK_ACCENT;
  const font = `600 13px ${style.getPropertyValue('font-family') || 'sans-serif'}`;

  const probe = document.createElement('canvas').getContext('2d');
  if (!probe) throw new Error('canvas is not available');
  probe.font = font;
  const width = Math.min(MAX_WIDTH, Math.ceil(probe.measureText(label).width) + PAD_X * 2);

  const canvas = document.createElement('canvas');
  canvas.width = width * SCALE;
  canvas.height = HEIGHT * SCALE;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('canvas is not available');
  ctx.scale(SCALE, SCALE);
  ctx.fillStyle = accent;
  ctx.beginPath();
  ctx.roundRect(0, 0, width, HEIGHT, 8);
  ctx.fill();
  ctx.fillStyle = '#fff';
  ctx.font = font;
  ctx.textBaseline = 'middle';
  ctx.fillText(label, PAD_X, HEIGHT / 2 + 0.5, width - PAD_X * 2);

  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'));
  if (!blob) throw new Error('could not encode the drag image');
  return Array.from(new Uint8Array(await blob.arrayBuffer()));
}
