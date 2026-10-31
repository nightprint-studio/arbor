/**
 * The files waiting to go with Imber's next message.
 *
 * Draft state of the composer, like the text: not saved, and gone once the message is sent. Two ways in — a file
 * chosen in the explorer (kept as a path; the backend reads it when the message is sent, so a file that changed
 * since is sent as it is then) and a picture pasted from the clipboard (kept as base64, it has no file).
 *
 * The limits are the backend's (`attachments.rs`): it re-checks everything and refuses the whole message with a
 * reason; the count is also held here so the chips never promise more than will be sent.
 */
import { convertFileSrc } from '@tauri-apps/api/core';
import type { AttachmentParam } from '$lib/ipc/bennu/assistant';
import type { AttachmentItem } from '$lib/components/bennu/assistant/assistant-types';

/** The most files one message carries. */
export const MAX_ATTACHMENTS = 8;
const PICTURE = /\.(png|jpe?g|gif|webp|svg)$/i;

interface Pending {
  name: string;
  /** The file to read; absent for a pasted picture. */
  path?: string;
  /** Base64 of a pasted picture. */
  data?: string;
  kind: AttachmentItem['kind'];
  size: number;
  preview?: string;
}

function createAttachmentsStore() {
  let list = $state<Pending[]>([]);
  /** Bumped by the palette: the composer opens its file picker when this changes. */
  let pickerRequest = $state(0);

  const room = () => MAX_ATTACHMENTS - list.length;

  return {
    get count() { return list.length; },
    get full() { return list.length >= MAX_ATTACHMENTS; },
    get pickerRequest() { return pickerRequest; },
    /** The chips, as the thread would draw them. */
    get items(): AttachmentItem[] {
      return list.map((p) => ({ name: p.name, kind: p.kind, size: p.size, preview: p.preview }));
    },

    requestPicker() { pickerRequest += 1; },

    /** Add files chosen in the explorer. Returns how many were left out for want of room. */
    addPaths(paths: string[]): number {
      const fresh = paths.filter((p) => !list.some((x) => x.path === p));
      const take = fresh.slice(0, Math.max(room(), 0));
      list = [
        ...list,
        ...take.map((path): Pending => {
          const name = path.split(/[\\/]/).pop() || path;
          const picture = PICTURE.test(name);
          return { name, path, kind: picture ? 'image' : 'text', size: 0, preview: picture ? convertFileSrc(path) : undefined };
        }),
      ];
      return fresh.length - take.length;
    },

    /** Add a picture from the clipboard. False when the message is already full. */
    addPasted(name: string, base64: string, size: number, preview: string): boolean {
      return this.addData(name, base64, size, 'image', preview);
    },

    /** Add a file carried as bytes (pasted, or dropped — a dropped file has no path). False when full. */
    addData(name: string, base64: string, size: number, kind: AttachmentItem['kind'], preview?: string): boolean {
      if (room() <= 0) return false;
      list = [...list, { name, data: base64, kind, size, preview }];
      return true;
    },

    remove(index: number) { list = list.filter((_, i) => i !== index); },
    removeLast() { list = list.slice(0, -1); },
    clear() { list = []; },

    /** What `assistant_send` takes, and the chips for the message as it appears in the thread. */
    snapshot(): { params: AttachmentParam[]; items: AttachmentItem[] } {
      return {
        params: list.map((p) => ({ name: p.name, path: p.path, data: p.data })),
        items: list.map((p) => ({ name: p.name, kind: p.kind, size: p.size, preview: p.preview })),
      };
    },
  };
}

export const assistantAttachmentsStore = createAttachmentsStore();
