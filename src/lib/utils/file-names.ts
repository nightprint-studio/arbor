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

/**
 * Whether a file is one of Struts's own configuration files.
 *
 * `struts.xml` and the `struts-<module>.xml` a real project splits it into, plus the two a plugin
 * ships (`struts-plugin.xml`, `struts-default.xml`) and `struts.properties`.
 *
 * Deliberately NOT every XML that is Struts's: a `validation.xml` beside them belongs to Struts
 * too, and it is *also* what Bean Validation calls its config — a file that could be either is
 * better left generic than labelled with the wrong framework. Same reasoning as `isDockerfile`
 * living here: the icon resolver and the editor's language registry both ask, and cannot import
 * from each other.
 */
export function isStrutsConfig(name: string): boolean {
  const lower = (name.split(/[\\/]/).pop() ?? '').toLowerCase();
  if (lower === 'struts.properties') return true;
  if (!lower.endsWith('.xml')) return false;
  return lower === 'struts.xml' || lower.startsWith('struts-');
}

/**
 * Whether a file is one of Tomcat's own configuration files, **by name**.
 *
 * The fallback tier: the names are generic — `context.xml` and `server.xml` could belong to
 * anything — so what really decides is the root element, and this only answers where the content
 * is not to hand (a tab strip has a path and no bytes).
 *
 * `web.xml` is deliberately absent. It belongs to the servlet specification and is the same file
 * under Jetty, WebSphere or nothing at all; a mark naming the wrong container is worse than the
 * generic one.
 */
export function isTomcatConfig(name: string): boolean {
  const lower = (name.split(/[\\/]/).pop() ?? '').toLowerCase();
  return lower === 'context.xml' || lower === 'server.xml' || lower === 'tomcat-users.xml';
}
