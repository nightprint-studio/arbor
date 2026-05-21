// RON-specific types — FE-only. The IPC layer (`$lib/ipc/studio-format`)
// is format-agnostic and exposes `kind: string`; this file declares
// the RON kind union so the RON modal and stores can narrow at the
// call site without losing the format-agnostic wire types.
//
// Kept separate from the IPC layer because (a) these have no Tauri
// counterpart — they're pure FE narrowing helpers — and (b) future
// `ron-studio.svelte.ts` / `RonStudioModal.svelte` callers can import
// them without touching the multi-format IPC surface.

import type {
  DiffTreeNode,
  StudioDocSnapshot,
  StudioNodeView,
  StudioQueryHit,
} from '$lib/ipc/studio-format';

export type RonNodeKind =
  | 'struct' | 'named_struct'
  | 'tuple'  | 'named_tuple'
  | 'unit_variant'
  | 'map' | 'list'
  | 'string' | 'char' | 'number' | 'bool'
  | 'option' | 'unit';

/** Type-tagged primitive value sent to the host for `set_primitive`
 *  mutations. The discriminant chooses which RON AST variant the
 *  backend installs — the FE is expected to send the right variant
 *  for the target node's current kind. */
export type RonPrimitiveValue =
  | { type: 'bool';   value: boolean }
  | { type: 'int';    value: number  }
  | { type: 'float';  value: number  }
  | { type: 'string'; value: string  }
  | { type: 'char';   value: string  };

/** Narrowing aliases over the format-agnostic IPC shapes. The wire
 *  format is identical — only the `kind` union narrows. */
export type RonNodeView    = StudioNodeView<RonNodeKind>;
export type RonQueryHit    = StudioQueryHit<RonNodeKind>;
export type RonDocSnapshot = StudioDocSnapshot<RonNodeKind>;
export type RonDiffTree    = DiffTreeNode<RonNodeKind>;
