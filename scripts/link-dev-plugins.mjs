// Link a plugin package you are DEVELOPING into Arbor's profiles, instead of copying it.
//
// A release build reads a profile's plugin pools on disk; a debug build reads the workspace's
// `plugins/`. So a package whose source lives in some other checkout has to be put into the
// profile somehow, and copying it means every `build.sh` is followed by a copy you will forget.
// Symlinks remove that step: rebuild, restart Arbor, done.
//
// ## Why per-file links and not one link to the package directory
//
// A source tree is not a package. `src/`, `wit/`, `Cargo.toml`, `build.sh`, `app/target` — none
// of it is shipped, and a directory link drags all of it in, including build outputs measured in
// gigabytes. What an installed package holds is the manifest, the docs, the modules the manifest
// names and whatever the plugin loads at runtime; that is what gets linked, one entry at a time,
// into a real directory. The listing then reads like an install rather than like a checkout.
//
// The exclusion list below is what a build produces or consumes, never what it ships. A Lua-only
// package matches none of it and is linked whole, which is right: its source IS its shipped form.
//
// ## Usage
//
//   node scripts/link-dev-plugins.mjs <path>...
//   node scripts/link-dev-plugins.mjs --profile dev <path>...
//   node scripts/link-dev-plugins.mjs --unlink <path>...
//
// Each `<path>` is either a package directory (one holding a `plugin.toml`) or a directory of
// them. No path is baked in: which checkouts you develop is yours, not this repo's.
//
// Links land in `<profile>/plugins/marketplace_plugins/<name>/` — the one pool both a debug and a
// release build scan. `--profile` limits which profiles; the default is every profile that exists.

import { existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, rmSync, symlinkSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve, basename } from 'node:path';

/** Build inputs and build outputs. Everything here is absent from a real installation. */
const NOT_SHIPPED = new Set([
  'src', 'wit', 'provider', 'app', 'target', 'tests', 'benches',
  'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
  '.git', '.gitignore', '.DS_Store',
]);
const NOT_SHIPPED_RE = /^(build.*\.(sh|ps1|bat)|.*\.rs)$/;

const shipped = (name) => !NOT_SHIPPED.has(name) && !NOT_SHIPPED_RE.test(name);

/** Arbor's config root — the same three locations `arbor_config_dir()` resolves to. */
function arborConfigDir() {
  if (process.platform === 'darwin') return join(homedir(), 'Library', 'Application Support', 'arbor');
  if (process.platform === 'win32') return join(process.env.APPDATA ?? join(homedir(), 'AppData', 'Roaming'), 'arbor');
  return join(process.env.XDG_CONFIG_HOME ?? join(homedir(), '.config'), 'arbor');
}

// ── Arguments ────────────────────────────────────────────────────────────────
const argv = process.argv.slice(2);
const unlink = argv.includes('--unlink');
let only = null;
const paths = [];
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === '--unlink') continue;
  if (argv[i] === '--profile') { only = (argv[++i] ?? '').split(',').filter(Boolean); continue; }
  paths.push(argv[i]);
}

if (paths.length === 0) {
  console.error('usage: node scripts/link-dev-plugins.mjs [--profile a,b] [--unlink] <package-or-parent-dir>...');
  process.exit(2);
}

const profilesRoot = join(arborConfigDir(), 'profiles');
if (!existsSync(profilesRoot)) {
  console.error(`[link-dev-plugins] no profiles at ${profilesRoot} — run Arbor once first.`);
  process.exit(1);
}
const profiles = readdirSync(profilesRoot)
  // A directory, and only a directory: `readdir` also hands back whatever the OS scattered in
  // there (`.DS_Store`), and treating one of those as a profile fails several packages later
  // with an error naming a path nobody wrote.
  .filter((p) => lstatSync(join(profilesRoot, p)).isDirectory())
  .filter((p) => !only || only.includes(p));

if (profiles.length === 0) {
  console.error(`[link-dev-plugins] no matching profile in ${profilesRoot}`);
  process.exit(1);
}

/** Every package directory under `p` — `p` itself when it holds a manifest. */
function packagesIn(p) {
  const dir = resolve(p);
  if (!existsSync(dir)) { console.error(`[link-dev-plugins] not found: ${dir}`); return []; }
  if (existsSync(join(dir, 'plugin.toml'))) return [dir];
  return readdirSync(dir)
    .map((n) => join(dir, n))
    .filter((d) => existsSync(join(d, 'plugin.toml')));
}

/** The package's own name, from its manifest — the folder it must land in. A folder named after
 *  the checkout instead would be discovered under the wrong name and shadow nothing it should. */
function manifestName(dir) {
  const m = /^\s*name\s*=\s*"([^"]+)"/m.exec(readFileSync(join(dir, 'plugin.toml'), 'utf8'));
  return m ? m[1] : basename(dir);
}

const packages = paths.flatMap(packagesIn);
if (packages.length === 0) process.exit(1);

for (const profile of profiles) {
  for (const src of packages) {
    const name = manifestName(src);
    const dst = join(profilesRoot, profile, 'plugins', 'marketplace_plugins', name);

    // Always start clean: the previous state may be a copy, an older link set, or a package
    // that has since dropped a file. Rewriting the directory is the only way the answer stays
    // "what the source ships today".
    rmSync(dst, { recursive: true, force: true });
    if (unlink) { console.log(`  ${profile}/${name}: removed`); continue; }

    const all = readdirSync(src);
    const entries = all.filter(shipped);

    // Nothing to leave behind → link the package directory itself, and prefer that.
    //
    // A Lua package has no build inputs: its source IS its shipped form. Linking it whole is
    // both simpler and the only shape that works today, because the `require` sandbox used to
    // resolve each module and demand it sit physically inside the plugin directory — with
    // per-file links every `require` in the package failed at once. That guard is fixed (it
    // checks the NAME now, see `sandbox.rs`), but a directory link needs no fix to be right,
    // and a package that ships everything it has is exactly what it describes.
    if (entries.length === all.length) {
      try {
        symlinkSync(src, dst, 'dir');
        console.log(`  ${profile}/${name}: dir → ${src}`);
      } catch (e) {
        console.error(`  ${profile}/${name}: could not link — ${e.message}`);
      }
      continue;
    }

    mkdirSync(dst, { recursive: true });
    for (const entry of entries) {
      try {
        symlinkSync(join(src, entry), join(dst, entry), lstatSync(join(src, entry)).isDirectory() ? 'dir' : 'file');
      } catch (e) {
        console.error(`  ${profile}/${name}: could not link ${entry} — ${e.message}`);
      }
    }
    console.log(`  ${profile}/${name}: ${entries.length} link → ${src}`);
  }
}

if (!unlink) {
  console.log('\nRestart Arbor to pick the packages up. Rebuilds are live from now on — the links');
  console.log('point at the source, so only Arbor itself needs restarting, never this script.');
}
