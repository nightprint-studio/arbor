/**
 * Searching the **dependency jars** — the classes and files that are on the classpath but
 * nowhere in the tree.
 *
 * Unlike every other navigator source, these are queried rather than fetched. A legacy
 * classpath is two or three hundred jars holding hundreds of thousands of entries; handing
 * that over to filter in the page would cost tens of megabytes per opening of a dialog, to
 * answer a question about twenty of them. So the query goes to the backend and only the
 * candidates come back — the ranking and the highlighting still happen here, in the one place
 * that does them.
 *
 * Same convention as the rest of the bennu IPC: one `args` object, snake_case fields.
 */

import { bennu } from '../rpc';

/** A class found in a dependency jar. */
export interface LibraryClassDto {
  /** Dot form, nested types included (`java.util.Map.Entry`). */
  fqcn: string;
  /** The type's own name, without its package. */
  simple: string;
  /** The package — what tells four `Service`s apart. */
  package: string;
  /** The artifact it came from: the jar's file name, version and all. */
  jar: string;
  /** Type-kind slug — `class` | `interface` | `enum` | `record` | `annotation`. The same
   *  vocabulary a project type carries, so one icon rule serves both lists. Empty when the class
   *  file could not be read; render it as an ordinary class rather than guessing. */
  kind?: string;
}

/** A non-class entry found in a dependency jar. */
export interface LibraryFileDto {
  /** `<jar file name>!/<entry>` — what {@link openLibraryFile} takes back. */
  id: string;
  /** The entry's last path segment. */
  name: string;
  /** Its full path inside the jar, which tells two `web.xml`s apart. */
  entry: string;
  /** The artifact it came from. */
  jar: string;
}

/** Classes on the project's dependency classpath matching `query`. Empty query → empty result:
 *  "every class on the classpath" is not an answer. Wire: `bennu_library_classes`. */
export function libraryClasses(root: string, query: string): Promise<LibraryClassDto[]> {
  return bennu('bennu_library_classes', { args: { root, query } });
}

/** Non-class files on the dependency classpath matching `query` — the `struts-default.xml`, the
 *  schemas, the bundled `.properties`. Wire: `bennu_library_files`. */
export function libraryFiles(root: string, query: string): Promise<LibraryFileDto[]> {
  return bennu('bennu_library_files', { args: { root, query } });
}

/**
 * Extract a jar entry to the read-only view cache and return the path to open.
 *
 * A resource inside a jar has no path of its own, so there is nothing to hand the editor until
 * something writes it out. The extension is kept, which is the point — an `.xml` arriving as
 * `.txt` loses its highlighting and its structure.
 *
 * Wire: `bennu_library_file` — `{ root, id }`.
 */
export function openLibraryFile(root: string, id: string): Promise<string> {
  return bennu('bennu_library_file', { args: { root, id } });
}

/** Whether a find-in-files hit came from inside a jar rather than from a file on disk — the
 *  `<jar>!/<entry>` form the backend uses for both. */
export function isJarEntry(file: string): boolean {
  return file.includes('!/');
}
