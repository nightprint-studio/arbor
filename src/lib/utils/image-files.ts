/**
 * Which file names are images — one list, because there were three.
 *
 * The same set had been written out separately in the markdown editor's link recogniser and twice
 * inside the file explorer, and the copies had already drifted (one knows `heic`, one knows `tif`,
 * one knows neither). An image is an image in every window, so the answer lives here.
 *
 * ## What counts, and what deliberately does not
 *
 * Raster formats a WebView can decode, plus `svg` — which the WebView renders natively, so a
 * preview costs nothing. `svgz` is out: it is gzip, and an `<img>` will not decode it from a file
 * URL. Camera formats (`heic`, `raw`, `cr2`) are out of {@link IMAGE_EXTENSIONS} for previewing
 * because Chromium cannot decode them, and a preview that shows a broken-image glyph is worse than
 * none — the explorer's thumbnail list keeps its own wider set for *listing*.
 */

/** Extensions a WebView can render in an `<img>`, lower-case and without the dot. */
export const IMAGE_EXTENSIONS: readonly string[] = [
  'png', 'jpg', 'jpeg', 'jfif', 'gif', 'bmp', 'webp', 'ico', 'avif', 'apng', 'tif', 'tiff', 'svg',
];

/** Lower-cased extension of `name` (no dot), or `''`. Accepts a bare name or a full path. */
export function extensionOf(name: string): string {
  const base = name.split(/[\\/]/).pop() ?? name;
  const dot = base.lastIndexOf('.');
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : '';
}

/** Whether `path` names an image the app can display. */
export function isImageFile(path: string | null | undefined): boolean {
  if (!path) return false;
  return IMAGE_EXTENSIONS.includes(extensionOf(path));
}

/**
 * A short, honest label for an image's format — what the preview's status line says.
 *
 * `jpg` and `jpeg` are one format and read as `JPEG`; the rest are their own extension upper-cased,
 * which is what everyone calls them.
 */
export function imageFormatLabel(path: string): string {
  const ext = extensionOf(path);
  switch (ext) {
    case 'jpg':
    case 'jpeg':
    case 'jfif':
      return 'JPEG';
    case 'tif':
    case 'tiff':
      return 'TIFF';
    case 'apng':
      return 'APNG';
    case 'svg':
      return 'SVG';
    default:
      return ext.toUpperCase();
  }
}
