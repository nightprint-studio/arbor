import { invoke } from '@tauri-apps/api/core';

export interface FsEntry {
  name:     string;
  path:     string;
  is_dir:   boolean;
  size:     number | null;
  modified: number | null;  // Unix timestamp ms
}

export interface FsRoot {
  name: string;
  path: string;
  kind: 'home' | 'desktop' | 'documents' | 'downloads' | 'drive';
}

/** Read a directory — returns entries with metadata. Dot-prefixed entries
 *  are skipped unless `showHidden` is set. */
export const fsReadDir = (path: string, showHidden = false) =>
  invoke<FsEntry[]>('fs_read_dir', { path, showHidden });

/** Return quick-access roots (common dirs + drives). */
export const listFsRoots = () =>
  invoke<FsRoot[]>('list_fs_roots');

export const fsCreateDir      = (path: string)                      => invoke<void>('fs_create_dir',        { path });
export const fsCreateFile     = (path: string)                      => invoke<void>('fs_create_file',       { path });
export const fsWriteTextFile  = (path: string, content: string)     => invoke<void>('fs_write_text_file',   { path, content });
export const fsReadTextFile   = (path: string)                      => invoke<string>('fs_read_text_file', { path });
export const fsRename         = (oldPath: string, newPath: string)  => invoke<void>('fs_rename',            { oldPath, newPath });
export const fsDelete         = (path: string)                      => invoke<void>('fs_delete',            { path });
