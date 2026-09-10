/**
 * What a file is, from its **name alone** — for the cases where the name is the whole answer and
 * an extension is not.
 *
 * Small on purpose. Most files are classified by extension, and the few that are not usually have
 * their predicate beside the module that cares (`isCargoManifest` with the Cargo manifest language,
 * `isSpringPropertyFile` with the config-property one). What lands here is the predicate **two
 * layers need**: the icon resolver in `$lib/utils` and the editor's language registry under
 * `components/bennu` can't import from each other, and a file recognised by one and not the other
 * is exactly the state a Dockerfile was in — an icon in the tree, plain grey text in the editor.
 */

/**
 * Whether a file is a Dockerfile.
 *
 * Covers the three spellings in the wild: the bare `Dockerfile`, the suffixed `Dockerfile.dev` a
 * project ends up with once it builds more than one image, and the `api.dockerfile` form editors
 * key on — plus Podman's `Containerfile`, which is the same file under another name.
 *
 * The suffixed form is the one that actually bites: `Dockerfile.dev` read by extension is a `.dev`,
 * which in this app is geode's playtest-scenario format, so it came out with a stopwatch icon.
 */
export function isDockerfile(name: string): boolean {
  const lower = (name.split(/[\\/]/).pop() ?? '').toLowerCase();
  return (
    lower === 'dockerfile' || lower.startsWith('dockerfile.')
    || lower.endsWith('.dockerfile')
    || lower === 'containerfile' || lower.startsWith('containerfile.')
  );
}
