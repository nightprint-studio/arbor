/**
 * A spreadsheet in the project, as a grid.
 *
 * The reading happens in the backend (`bennu-sheet`), not here: a spreadsheet is a container
 * format, and the alternative was a JavaScript parser running in the WebView. What crosses the
 * seam is already **rendered** — the serial number that is a date has become a date — because that
 * conversion is the one piece of real logic in reading a spreadsheet and it belongs where it is
 * tested rather than in a component.
 */

import { bennu } from '../rpc';

/** What a cell **is** — what the grid aligns and tints by. */
export type SheetCellKind = 'empty' | 'text' | 'number' | 'date' | 'bool' | 'error';

/** One cell, already rendered. */
export interface SheetCell {
  text: string;
  kind: SheetCellKind;
}

/** One sheet. Every row is padded to `columns`, so the grid draws a rectangle without measuring. */
export interface Sheet {
  name: string;
  columns: number;
  rows: SheetCell[][];
  /** The sheet has more rows or columns than were read. Said out loud rather than hidden: what is
   *  on screen must never be mistaken for all of it. */
  truncated: boolean;
}

/** A workbook, as much of it as is worth looking at. */
export interface Workbook {
  /** `xlsx` — what the bytes turned out to be, which is not always what the name said. */
  format: string;
  sheets: Sheet[];
  /** The workbook has more sheets than were read. */
  truncated: boolean;
}

/** Read a spreadsheet. Rejects with a sentence worth showing — an unreadable file, a format that
 *  is not supported yet (the old binary `.xls`), or one too large for a viewer.
 *  Wire: `bennu_read_sheet`. */
export function readSheet(file: string): Promise<Workbook> {
  return bennu('bennu_read_sheet', { args: { file } });
}
