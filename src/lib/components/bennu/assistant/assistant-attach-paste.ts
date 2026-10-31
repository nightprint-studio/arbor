/**
 * Pictures on the clipboard, read as attachments.
 *
 * A screenshot pasted into the composer arrives as a `File` in `clipboardData`, with no path to read it from, so it
 * is carried as base64 and previewed through a data URL. The backend checks what it is by its bytes.
 */

export interface PastedPicture {
  name: string;
  base64: string;
  size: number;
  /** A data URL, for the thumbnail in the composer and in the thread. */
  preview: string;
}

const EXTENSION: Record<string, string> = { 'image/png': 'png', 'image/jpeg': 'jpg', 'image/gif': 'gif', 'image/webp': 'webp' };

function read(file: File, index: number): Promise<PastedPicture> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error);
    reader.onload = () => {
      const preview = String(reader.result);
      const ext = EXTENSION[file.type] ?? 'png';
      const name = file.name && file.name !== 'image.png' ? file.name : `pasted-${Date.now().toString(36)}${index ? `-${index}` : ''}.${ext}`;
      resolve({ name, base64: preview.slice(preview.indexOf(',') + 1), size: file.size, preview });
    };
    reader.readAsDataURL(file);
  });
}

/** The pictures of a paste event (raster only: the provider reads no others), each still being read. */
export function pastedPictures(e: ClipboardEvent): Promise<PastedPicture>[] {
  const files = Array.from(e.clipboardData?.files ?? []).filter((f) => f.type in EXTENSION);
  return files.map(read);
}

/** A file dropped on the composer, read whole: a WebView gives a dropped file no path, only its bytes. */
export interface DroppedFile {
  name: string;
  base64: string;
  size: number;
  kind: 'image' | 'text';
  /** A data URL for a picture's thumbnail. */
  preview?: string;
}

/** Larger than this is refused before it is read — the backend's own ceiling is lower still. */
export const MAX_DROPPED_BYTES = 4_000_000;

const PICTURE_TYPE = /^image\/(png|jpeg|gif|webp|svg\+xml)$/;

/** Whether a drag carries files (and not, say, selected text). */
export function dragHasFiles(e: DragEvent): boolean {
  return Array.from(e.dataTransfer?.types ?? []).includes('Files');
}

function readDropped(file: File): Promise<DroppedFile> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error);
    reader.onload = () => {
      const url = String(reader.result);
      const picture = PICTURE_TYPE.test(file.type);
      resolve({
        name: file.name || 'dropped-file',
        base64: url.slice(url.indexOf(',') + 1),
        size: file.size,
        kind: picture ? 'image' : 'text',
        preview: picture ? url : undefined,
      });
    };
    reader.readAsDataURL(file);
  });
}

/** The files of a drop: those small enough to read, and the names of those that were not. */
export function droppedFiles(e: DragEvent): { read: Promise<DroppedFile>[]; tooLarge: string[] } {
  const all = Array.from(e.dataTransfer?.files ?? []);
  return {
    read: all.filter((f) => f.size <= MAX_DROPPED_BYTES).map(readDropped),
    tooLarge: all.filter((f) => f.size > MAX_DROPPED_BYTES).map((f) => f.name),
  };
}
