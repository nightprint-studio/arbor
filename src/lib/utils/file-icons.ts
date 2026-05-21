/**
 * Shared file/folder icon resolver — VS Code icon set via Iconify.
 *
 * Used by the file tree panel and the file/folder picker so the visual
 * vocabulary stays consistent across the app. Icons are statically imported
 * (one bundle entry per icon) so there's no CDN dependency at runtime.
 */
import type { IconifyIcon } from '@iconify/svelte';

// ── File-type icons ──────────────────────────────────────────────────────────
import rustIcon        from '@iconify-icons/vscode-icons/file-type-rust';
import tsIcon          from '@iconify-icons/vscode-icons/file-type-typescript-official';
import tsDefIcon       from '@iconify-icons/vscode-icons/file-type-typescriptdef';
import jsIcon          from '@iconify-icons/vscode-icons/file-type-js-official';
import svelteIcon      from '@iconify-icons/vscode-icons/file-type-svelte';
import vueIcon         from '@iconify-icons/vscode-icons/file-type-vue';
import pythonIcon      from '@iconify-icons/vscode-icons/file-type-python';
import goIcon          from '@iconify-icons/vscode-icons/file-type-go';
import javaIcon        from '@iconify-icons/vscode-icons/file-type-java';
import kotlinIcon      from '@iconify-icons/vscode-icons/file-type-kotlin';
import csharpIcon      from '@iconify-icons/vscode-icons/file-type-csharp';
import cIcon           from '@iconify-icons/vscode-icons/file-type-c';
import cppIcon         from '@iconify-icons/vscode-icons/file-type-cpp';
import rubyIcon        from '@iconify-icons/vscode-icons/file-type-ruby';
import phpIcon         from '@iconify-icons/vscode-icons/file-type-php';
import swiftIcon       from '@iconify-icons/vscode-icons/file-type-swift';
import luaIcon         from '@iconify-icons/vscode-icons/file-type-lua';
import tomlIcon        from '@iconify-icons/vscode-icons/file-type-toml';
import yamlIcon        from '@iconify-icons/vscode-icons/file-type-yaml';
import jsonIcon        from '@iconify-icons/vscode-icons/file-type-json';
import xmlIcon         from '@iconify-icons/vscode-icons/file-type-xml';
import htmlIcon        from '@iconify-icons/vscode-icons/file-type-html';
import cssIcon         from '@iconify-icons/vscode-icons/file-type-css';
import scssIcon        from '@iconify-icons/vscode-icons/file-type-scss';
import sassIcon        from '@iconify-icons/vscode-icons/file-type-sass';
import markdownIcon    from '@iconify-icons/vscode-icons/file-type-markdown';
import mdxIcon         from '@iconify-icons/vscode-icons/file-type-mdx';
import shellIcon       from '@iconify-icons/vscode-icons/file-type-shell';
import powershellIcon  from '@iconify-icons/vscode-icons/file-type-powershell';
import graphqlIcon     from '@iconify-icons/vscode-icons/file-type-graphql';
import svgFileIcon     from '@iconify-icons/vscode-icons/file-type-svg';
import imageIcon       from '@iconify-icons/vscode-icons/file-type-image';
import zipIcon         from '@iconify-icons/vscode-icons/file-type-zip';
import sqlIcon         from '@iconify-icons/vscode-icons/file-type-sql';
import textIcon        from '@iconify-icons/vscode-icons/file-type-text';
import dotenvIcon      from '@iconify-icons/vscode-icons/file-type-dotenv';
import cargoIcon       from '@iconify-icons/vscode-icons/file-type-cargo';
import npmIcon         from '@iconify-icons/vscode-icons/file-type-npm';
import yarnIcon        from '@iconify-icons/vscode-icons/file-type-yarn';
import dockerIcon      from '@iconify-icons/vscode-icons/file-type-docker';
import gitFileIcon     from '@iconify-icons/vscode-icons/file-type-git';
import makefileIcon    from '@iconify-icons/vscode-icons/file-type-makefile';

// ── Folder icons ─────────────────────────────────────────────────────────────
import folderDefault       from '@iconify-icons/vscode-icons/default-folder';
import folderDefaultOpen   from '@iconify-icons/vscode-icons/default-folder-opened';
import folderGit           from '@iconify-icons/vscode-icons/folder-type-git';
import folderGitOpen       from '@iconify-icons/vscode-icons/folder-type-git-opened';
import folderSrc           from '@iconify-icons/vscode-icons/folder-type-src';
import folderSrcOpen       from '@iconify-icons/vscode-icons/folder-type-src-opened';
import folderLib           from '@iconify-icons/vscode-icons/folder-type-library';
import folderLibOpen       from '@iconify-icons/vscode-icons/folder-type-library-opened';
import folderNode          from '@iconify-icons/vscode-icons/folder-type-node';
import folderNodeOpen      from '@iconify-icons/vscode-icons/folder-type-node-opened';
import folderDist          from '@iconify-icons/vscode-icons/folder-type-dist';
import folderDistOpen      from '@iconify-icons/vscode-icons/folder-type-dist-opened';
import folderDocs          from '@iconify-icons/vscode-icons/folder-type-docs';
import folderDocsOpen      from '@iconify-icons/vscode-icons/folder-type-docs-opened';
import folderTest          from '@iconify-icons/vscode-icons/folder-type-test';
import folderTestOpen      from '@iconify-icons/vscode-icons/folder-type-test-opened';
import folderAsset         from '@iconify-icons/vscode-icons/folder-type-asset';
import folderAssetOpen     from '@iconify-icons/vscode-icons/folder-type-asset-opened';
import folderComponent     from '@iconify-icons/vscode-icons/folder-type-component';
import folderComponentOpen from '@iconify-icons/vscode-icons/folder-type-component-opened';
import folderConfig        from '@iconify-icons/vscode-icons/folder-type-config';
import folderConfigOpen    from '@iconify-icons/vscode-icons/folder-type-config-opened';
import folderPublic        from '@iconify-icons/vscode-icons/folder-type-public';
import folderPublicOpen    from '@iconify-icons/vscode-icons/folder-type-public-opened';
import folderPlugin        from '@iconify-icons/vscode-icons/folder-type-plugin';
import folderPluginOpen    from '@iconify-icons/vscode-icons/folder-type-plugin-opened';
import folderScript        from '@iconify-icons/vscode-icons/folder-type-script';
import folderScriptOpen    from '@iconify-icons/vscode-icons/folder-type-script-opened';
import folderStyle         from '@iconify-icons/vscode-icons/folder-type-style';
import folderStyleOpen     from '@iconify-icons/vscode-icons/folder-type-style-opened';
import folderView          from '@iconify-icons/vscode-icons/folder-type-view';
import folderViewOpen      from '@iconify-icons/vscode-icons/folder-type-view-opened';
import folderHook          from '@iconify-icons/vscode-icons/folder-type-hook';
import folderHookOpen      from '@iconify-icons/vscode-icons/folder-type-hook-opened';
import folderModel         from '@iconify-icons/vscode-icons/folder-type-model';
import folderModelOpen     from '@iconify-icons/vscode-icons/folder-type-model-opened';
import folderController    from '@iconify-icons/vscode-icons/folder-type-controller';
import folderControllerOpen from '@iconify-icons/vscode-icons/folder-type-controller-opened';
import folderImages        from '@iconify-icons/vscode-icons/folder-type-images';
import folderImagesOpen    from '@iconify-icons/vscode-icons/folder-type-images-opened';
import folderTypes         from '@iconify-icons/vscode-icons/folder-type-typescript';
import folderTypesOpen     from '@iconify-icons/vscode-icons/folder-type-typescript-opened';

// ── Lookup maps ─────────────────────────────────────────────────────────────

/** lowercase extension → icon */
const EXT_ICONS: Record<string, IconifyIcon> = {
  rs: rustIcon,
  ts: tsIcon, tsx: tsIcon,
  js: jsIcon, jsx: jsIcon, mjs: jsIcon, cjs: jsIcon,
  svelte: svelteIcon,
  vue: vueIcon,
  py: pythonIcon, pyi: pythonIcon,
  go: goIcon,
  java: javaIcon,
  kt: kotlinIcon, kts: kotlinIcon,
  cs: csharpIcon,
  c: cIcon, h: cIcon,
  cpp: cppIcon, cc: cppIcon, cxx: cppIcon, hpp: cppIcon, hxx: cppIcon,
  rb: rubyIcon,
  php: phpIcon,
  swift: swiftIcon,
  lua: luaIcon,
  toml: tomlIcon,
  yaml: yamlIcon, yml: yamlIcon,
  json: jsonIcon, json5: jsonIcon, jsonc: jsonIcon,
  xml: xmlIcon, plist: xmlIcon,
  html: htmlIcon, htm: htmlIcon,
  css: cssIcon,
  scss: scssIcon,
  sass: sassIcon,
  md: markdownIcon, mdx: mdxIcon, markdown: markdownIcon,
  sh: shellIcon, bash: shellIcon, zsh: shellIcon, fish: shellIcon,
  ps1: powershellIcon, psm1: powershellIcon,
  graphql: graphqlIcon, gql: graphqlIcon,
  svg: svgFileIcon,
  png: imageIcon, jpg: imageIcon, jpeg: imageIcon, gif: imageIcon,
  webp: imageIcon, ico: imageIcon, bmp: imageIcon, tiff: imageIcon,
  avif: imageIcon,
  zip: zipIcon, gz: zipIcon, tar: zipIcon, bz2: zipIcon,
  xz: zipIcon, rar: zipIcon, '7z': zipIcon, tgz: zipIcon,
  sql: sqlIcon, sqlite: sqlIcon, sqlite3: sqlIcon,
  txt: textIcon, log: textIcon, rtf: textIcon,
};

/** Exact filename (lowercased) → icon. Takes priority over extension. */
const FILENAME_ICONS: Record<string, IconifyIcon> = {
  'cargo.toml': cargoIcon,
  'cargo.lock': cargoIcon,
  'package.json': npmIcon,
  'package-lock.json': npmIcon,
  'yarn.lock': yarnIcon,
  '.yarnrc': yarnIcon,
  '.yarnrc.yml': yarnIcon,
  'dockerfile': dockerIcon,
  '.dockerignore': gitFileIcon,
  '.gitignore': gitFileIcon,
  '.gitattributes': gitFileIcon,
  '.gitmodules': gitFileIcon,
  'makefile': makefileIcon,
  'gnumakefile': makefileIcon,
  'rakefile': rubyIcon,
  'gemfile': rubyIcon,
  'gemfile.lock': rubyIcon,
  'procfile': shellIcon,
};

/** Folder name → [closed icon, open icon]. */
const FOLDER_ICONS: Record<string, [IconifyIcon, IconifyIcon]> = {
  '.git':         [folderGit,        folderGitOpen],
  'src':          [folderSrc,        folderSrcOpen],
  'source':       [folderSrc,        folderSrcOpen],
  'lib':          [folderLib,        folderLibOpen],
  'library':      [folderLib,        folderLibOpen],
  'node_modules': [folderNode,       folderNodeOpen],
  'dist':         [folderDist,       folderDistOpen],
  'build':        [folderDist,       folderDistOpen],
  'out':          [folderDist,       folderDistOpen],
  'output':       [folderDist,       folderDistOpen],
  'target':       [folderDist,       folderDistOpen],
  'release':      [folderDist,       folderDistOpen],
  'docs':         [folderDocs,       folderDocsOpen],
  'doc':          [folderDocs,       folderDocsOpen],
  'documentation':[folderDocs,       folderDocsOpen],
  'test':         [folderTest,       folderTestOpen],
  'tests':        [folderTest,       folderTestOpen],
  '__tests__':    [folderTest,       folderTestOpen],
  'spec':         [folderTest,       folderTestOpen],
  '__spec__':     [folderTest,       folderTestOpen],
  'e2e':          [folderTest,       folderTestOpen],
  'assets':       [folderAsset,      folderAssetOpen],
  'asset':        [folderAsset,      folderAssetOpen],
  'resources':    [folderAsset,      folderAssetOpen],
  'res':          [folderAsset,      folderAssetOpen],
  'components':   [folderComponent,  folderComponentOpen],
  'component':    [folderComponent,  folderComponentOpen],
  'config':       [folderConfig,     folderConfigOpen],
  'configs':      [folderConfig,     folderConfigOpen],
  'configuration':[folderConfig,     folderConfigOpen],
  'settings':     [folderConfig,     folderConfigOpen],
  '.config':      [folderConfig,     folderConfigOpen],
  'public':       [folderPublic,     folderPublicOpen],
  'static':       [folderPublic,     folderPublicOpen],
  'www':          [folderPublic,     folderPublicOpen],
  'plugins':      [folderPlugin,     folderPluginOpen],
  'plugin':       [folderPlugin,     folderPluginOpen],
  'extensions':   [folderPlugin,     folderPluginOpen],
  'scripts':      [folderScript,     folderScriptOpen],
  'script':       [folderScript,     folderScriptOpen],
  'bin':          [folderScript,     folderScriptOpen],
  'styles':       [folderStyle,      folderStyleOpen],
  'style':        [folderStyle,      folderStyleOpen],
  'css':          [folderStyle,      folderStyleOpen],
  'sass':         [folderStyle,      folderStyleOpen],
  'scss':         [folderStyle,      folderStyleOpen],
  'views':        [folderView,       folderViewOpen],
  'view':         [folderView,       folderViewOpen],
  'pages':        [folderView,       folderViewOpen],
  'page':         [folderView,       folderViewOpen],
  'hooks':        [folderHook,       folderHookOpen],
  'hook':         [folderHook,       folderHookOpen],
  'models':       [folderModel,      folderModelOpen],
  'model':        [folderModel,      folderModelOpen],
  'entities':     [folderModel,      folderModelOpen],
  'entity':       [folderModel,      folderModelOpen],
  'controllers':  [folderController, folderControllerOpen],
  'controller':   [folderController, folderControllerOpen],
  'images':       [folderImages,     folderImagesOpen],
  'image':        [folderImages,     folderImagesOpen],
  'img':          [folderImages,     folderImagesOpen],
  'photos':       [folderImages,     folderImagesOpen],
  'icons':        [folderImages,     folderImagesOpen],
  'types':        [folderTypes,      folderTypesOpen],
  'typings':      [folderTypes,      folderTypesOpen],
  '@types':       [folderTypes,      folderTypesOpen],
};

// ── Public API ──────────────────────────────────────────────────────────────

/** Resolve a file icon from its (base)name. Falls back to a generic text icon. */
export function getFileIcon(name: string): IconifyIcon {
  const lower = name.toLowerCase();
  if (FILENAME_ICONS[lower]) return FILENAME_ICONS[lower];
  if (lower.startsWith('.env')) return dotenvIcon;
  if (lower.endsWith('.d.ts')) return tsDefIcon;
  const ext = lower.split('.').pop() ?? '';
  return EXT_ICONS[ext] ?? textIcon;
}

/** Resolve a folder icon from its name and open/closed state. */
export function getFolderIcon(name: string, isOpen: boolean): IconifyIcon {
  const pair = FOLDER_ICONS[name.toLowerCase()];
  if (pair) return isOpen ? pair[1] : pair[0];
  return isOpen ? folderDefaultOpen : folderDefault;
}
