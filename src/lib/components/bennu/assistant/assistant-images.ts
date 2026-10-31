/**
 * Pictures of the thread, as `<img>` sources.
 *
 * The thread carries a path into the assistant's pictures folder and never the bytes (a screenshot, a shown
 * image, an attached file); the bytes are fetched when something is drawn, once per path. The answer is a data
 * URL, which an `<img>` draws without running anything — an SVG included, whatever it contains.
 */
import { getAssistantImage } from '$lib/ipc/bennu/assistant';

const cache = new Map<string, Promise<string>>();

/** The data URL of the kept picture at `path`. Rejects when the file is gone; a rejection is not remembered. */
export function loadPicture(path: string): Promise<string> {
  let hit = cache.get(path);
  if (!hit) {
    hit = getAssistantImage(path).then((img) => `data:${img.media_type};base64,${img.data}`);
    hit.catch(() => cache.delete(path));
    cache.set(path, hit);
  }
  return hit;
}

/** A picture of a message as the gallery sees it. */
export interface Picture {
  path: string;
  caption: string;
}
