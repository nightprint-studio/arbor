/**
 * Build-time `simple-icons` glyphs.
 *
 * `@iconify/svelte` renders an `<Icon icon="prefix:name" />` by hitting
 * `api.iconify.design` at runtime. Arbor must work fully offline, so every
 * simple-icon we use is imported here as an `IconifyIcon` object — the
 * `<Icon icon={obj} />` form short-circuits the network path.
 *
 * The split-per-file `@iconify-icons/simple-icons` package handles most of
 * the catalogue. A few recently-added editor brands (Cursor, Zed, RubyMine)
 * aren't published there yet, so their bodies are inlined verbatim from
 * `@iconify-json/simple-icons`. RustRover has no entry in simple-icons at
 * all and falls back to the generic `jetbrains` mark.
 */
import type { IconifyIcon } from '@iconify/svelte';

// ── Provider brands (used by BrandIcon / BrandTile / pipeline + MR widgets) ──
import githubIcon    from '@iconify-icons/simple-icons/github';
import gitlabIcon    from '@iconify-icons/simple-icons/gitlab';
import bitbucketIcon from '@iconify-icons/simple-icons/bitbucket';
import linearIcon    from '@iconify-icons/simple-icons/linear';
import jiraIcon      from '@iconify-icons/simple-icons/jira';

export const PROVIDER_ICON = {
  github:    githubIcon,
  gitlab:    gitlabIcon,
  bitbucket: bitbucketIcon,
  linear:    linearIcon,
  jira:      jiraIcon,
} as const;

export type ProviderBrand = keyof typeof PROVIDER_ICON;

// ── IDE brands (Settings → IDE Integration) ──────────────────────────────────
import vsCodeIcon     from '@iconify-icons/simple-icons/visualstudiocode';
import intellijIcon   from '@iconify-icons/simple-icons/intellijidea';
import webstormIcon   from '@iconify-icons/simple-icons/webstorm';
import pycharmIcon    from '@iconify-icons/simple-icons/pycharm';
import riderIcon      from '@iconify-icons/simple-icons/rider';
import clionIcon      from '@iconify-icons/simple-icons/clion';
import golandIcon     from '@iconify-icons/simple-icons/goland';
import phpstormIcon   from '@iconify-icons/simple-icons/phpstorm';
import sublimeIcon    from '@iconify-icons/simple-icons/sublimetext';
import vimIcon        from '@iconify-icons/simple-icons/vim';
import neovimIcon     from '@iconify-icons/simple-icons/neovim';
import jetbrainsIcon  from '@iconify-icons/simple-icons/jetbrains';

// Not yet shipped in @iconify-icons/simple-icons (lags behind the canonical
// JSON catalogue) — bodies copied from @iconify-json/simple-icons@1.2.81.
// 24×24 viewbox, single-path, `currentColor` fill — drop-in IconifyIcon objects.
const cursorIcon: IconifyIcon = {
  body: '<path fill="currentColor" d="M11.503.131L1.891 5.678a.84.84 0 0 0-.42.726v11.188c0 .3.162.575.42.724l9.609 5.55a1 1 0 0 0 .998 0l9.61-5.55a.84.84 0 0 0 .42-.724V6.404a.84.84 0 0 0-.42-.726L12.497.131a1.01 1.01 0 0 0-.996 0M2.657 6.338h18.55c.263 0 .43.287.297.515L12.23 22.918c-.062.107-.229.064-.229-.06V12.335a.59.59 0 0 0-.295-.51l-9.11-5.257c-.109-.063-.064-.23.061-.23"/>',
  width:  24,
  height: 24,
};
const zedIcon: IconifyIcon = {
  body: '<path fill="currentColor" d="M2.25 1.5a.75.75 0 0 0-.75.75v16.5H0V2.25A2.25 2.25 0 0 1 2.25 0h20.095c1.002 0 1.504 1.212.795 1.92L10.764 14.298h3.486V12.75h1.5v1.922a1.125 1.125 0 0 1-1.125 1.125H9.264l-2.578 2.578h11.689V9h1.5v9.375a1.5 1.5 0 0 1-1.5 1.5H5.185L2.562 22.5H21.75a.75.75 0 0 0 .75-.75V5.25H24v16.5A2.25 2.25 0 0 1 21.75 24H1.655C.653 24 .151 22.788.86 22.08L13.19 9.75H9.75v1.5h-1.5V9.375A1.125 1.125 0 0 1 9.375 8.25h5.314l2.625-2.625H5.625V15h-1.5V5.625a1.5 1.5 0 0 1 1.5-1.5h13.19L21.438 1.5z"/>',
  width:  24,
  height: 24,
};
const rubymineIcon: IconifyIcon = {
  body: '<path fill="currentColor" d="M0 0v24h24V0Zm3.056 3H6.92q.945 0 1.665.347t1.106.977c.262.42.392.902.392 1.46q0 .835-.399 1.478a2.6 2.6 0 0 1-1.125.99a2 2 0 0 1-.297.103l-.13.04L10.276 12H8.264l-1.94-3.4H4.811V12H3.056Zm8.51 0h2.444l1.851 5.907l.154.773l.136-.773L17.937 3h2.482v9h-1.736V5.578l.026-.47L16.613 12H15.34l-2.07-6.846l.026.424V12h-1.73ZM4.812 4.459V7.14h1.993q.444-.001.771-.161q.335-.167.515-.47c.12-.205.18-.439.18-.713q0-.411-.18-.707a1.17 1.17 0 0 0-.515-.462a1.7 1.7 0 0 0-.77-.168ZM2.996 19.2h9.6V21h-9.6z"/>',
  width:  24,
  height: 24,
};

// RustRover has no entry in simple-icons; fall back to the JetBrains mark.
const rustroverIcon = jetbrainsIcon;

export const IDE_ICON = {
  vscode:    vsCodeIcon,
  cursor:    cursorIcon,
  zed:       zedIcon,
  intellij:  intellijIcon,
  webstorm:  webstormIcon,
  pycharm:   pycharmIcon,
  rider:     riderIcon,
  clion:     clionIcon,
  goland:    golandIcon,
  rubymine:  rubymineIcon,
  phpstorm:  phpstormIcon,
  sublime:   sublimeIcon,
  rustrover: rustroverIcon,
  vim:       vimIcon,
  neovim:    neovimIcon,
} as const;

// ── Project-type brands (Settings → IDE by Language) ─────────────────────────
import rustIcon       from '@iconify-icons/simple-icons/rust';
import nodeIcon       from '@iconify-icons/simple-icons/nodedotjs';
import mavenIcon      from '@iconify-icons/simple-icons/apachemaven';
import gradleIcon     from '@iconify-icons/simple-icons/gradle';
import goIcon         from '@iconify-icons/simple-icons/go';
import pythonIcon     from '@iconify-icons/simple-icons/python';
import dotnetIcon     from '@iconify-icons/simple-icons/dotnet';
import cppIcon        from '@iconify-icons/simple-icons/cplusplus';
import rubyIcon       from '@iconify-icons/simple-icons/ruby';
import phpIcon        from '@iconify-icons/simple-icons/php';

export const PROJECT_ICON = {
  rust:        rustIcon,
  node_js:     nodeIcon,
  java_maven:  mavenIcon,
  java_gradle: gradleIcon,
  go:          goIcon,
  python:      pythonIcon,
  dot_net:     dotnetIcon,
  cpp:         cppIcon,
  ruby:        rubyIcon,
  php:         phpIcon,
} as const;

// ── Cloud-storage provider brands ────────────────────────────────────────────
// Exposed to plugins (via PluginIcon) so the cloud-storage connections-manager
// tree can show real provider marks instead of generic cloud puff. Bundled at
// build time — no runtime fetch to api.iconify.design.
import googleCloudIcon  from '@iconify-icons/simple-icons/googlecloud';
import amazonS3Icon     from '@iconify-icons/simple-icons/amazons3';
import microsoftAzureIcon from '@iconify-icons/simple-icons/microsoftazure';

export const CLOUD_PROVIDER_ICON = {
  google_cloud:    googleCloudIcon,
  amazon_s3:       amazonS3Icon,
  microsoft_azure: microsoftAzureIcon,
} as const;

export type CloudProviderBrand = keyof typeof CLOUD_PROVIDER_ICON;
