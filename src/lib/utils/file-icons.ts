/**
 * Shared file/folder icon resolver — VS Code icon set via Iconify.
 *
 * Used by the file tree panel and the file/folder picker so the visual
 * vocabulary stays consistent across the app. Icons are statically imported
 * (one bundle entry per icon) so there's no CDN dependency at runtime.
 */
import type { IconifyIcon } from '@iconify/svelte';
import { isDockerfile, isStrutsConfig, isTomcatConfig } from './file-names';

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
import jspIcon         from '@iconify-icons/vscode-icons/file-type-jsp';
// The Apache feather, not a Struts mark: neither icon set has one, and the feather is what the
// file's own DOCTYPE calls it ("Apache Software Foundation//DTD Struts…"). Honest, and it reads
// apart from the forty other XMLs at a glance, which is the whole job of an icon here.
import apacheIcon      from '@iconify-icons/simple-icons/apache';
import tomcatIcon      from '@iconify-icons/simple-icons/apachetomcat';
import htmlIcon        from '@iconify-icons/vscode-icons/file-type-html';
import cssIcon         from '@iconify-icons/vscode-icons/file-type-css';
import scssIcon        from '@iconify-icons/vscode-icons/file-type-scss';
import sassIcon        from '@iconify-icons/vscode-icons/file-type-sass';
import markdownIcon    from '@iconify-icons/vscode-icons/file-type-markdown';
import mdxIcon         from '@iconify-icons/vscode-icons/file-type-mdx';
import shellIcon       from '@iconify-icons/vscode-icons/file-type-shell';
import powershellIcon  from '@iconify-icons/vscode-icons/file-type-powershell';
import batIcon         from '@iconify-icons/vscode-icons/file-type-bat';
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
// The JVM / CI vocabulary. A legacy Java project's root is mostly these, and telling a
// `pom.xml` from any other XML at a glance is most of what an icon is for.
import mavenIcon       from '@iconify-icons/vscode-icons/file-type-maven';
import gradleIcon      from '@iconify-icons/vscode-icons/file-type-gradle';
import jarIcon         from '@iconify-icons/vscode-icons/file-type-jar';
import gitlabIcon      from '@iconify-icons/vscode-icons/file-type-gitlab';
import jenkinsIcon     from '@iconify-icons/vscode-icons/file-type-jenkins';
import editorconfigIcon from '@iconify-icons/vscode-icons/file-type-editorconfig';
// JUnit ships a real glyph in simple-icons, which vscode-icons has no entry for. It is
// monochrome (`currentColor`), and the file tree around it is not — see `junitIcon` below.
import junit5Glyph    from '@iconify-icons/simple-icons/junit5';
import helmIcon        from '@iconify-icons/vscode-icons/file-type-helm';
import logIcon         from '@iconify-icons/vscode-icons/file-type-log';
import keyIcon         from '@iconify-icons/vscode-icons/file-type-key';
// RON — geode's content format, and what a Bevy asset is written in. It has its own mark in
// the set, so it no longer has to borrow Rust's and read as a source file.
import ronIcon         from '@iconify-icons/vscode-icons/file-type-ron';
// Office documents. Bennu renders `.docx` in a tab (`BennuDocxView`); the rest are listed so a
// folder of hand-offs reads as what it is instead of as a wall of plain-text marks.
import wordIcon        from '@iconify-icons/vscode-icons/file-type-word';
import excelIcon       from '@iconify-icons/vscode-icons/file-type-excel';
import powerpointIcon  from '@iconify-icons/vscode-icons/file-type-powerpoint';
import pdfIcon         from '@iconify-icons/vscode-icons/file-type-pdf2';
// Shaders. `.wgsl` is what a Bevy material is written in, and until now it fell through to
// the plain-text mark in a folder where every file is one.
import wgslIcon        from '@iconify-icons/vscode-icons/file-type-wgsl';
import fontIcon        from '@iconify-icons/vscode-icons/file-type-font';
import glslIcon        from '@iconify-icons/vscode-icons/file-type-glsl';
import hlslIcon        from '@iconify-icons/vscode-icons/file-type-hlsl';
// Jinja — the code templates Bennu generates from. By the last extension, so `Order.java.jinja`
// wears the template's mark rather than Java's: it is edited as a template, and nothing compiles it.
import jinjaIcon       from '@iconify-icons/vscode-icons/file-type-jinja';

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

// ── Icone di casa ────────────────────────────────────────────────────────────
//
// Tre estensioni che nessun set generico conosce, disegnate qui. Sono oggetti `IconifyIcon`
// come gli altri — `body` è il markup interno di un `<svg viewBox="0 0 32 32">` — quindi
// stanno nella stessa mappa e si comportano allo stesso modo.
//
// ⚠️ Vincolo di progetto: si guardano a **16 px**, nell'albero dei file e sulle linguette.
// Niente tratti sottili, niente dettaglio fine, e una silhouette che si riconosca prima del
// colore. Le prime versioni sbagliavano proprio lì — un cristallo troppo stretto diventava una
// scheggia, e il pulsante del cronometro si staccava e sembrava una stella.

/**
 * `.dig` — il linguaggio di **Nobody Digs Anymore**.
 *
 * Un cristallo sfaccettato, che nel gioco è tutto: si semina, cresce, si raccoglie e si vende —
 * l'intera economia passa di lì, e il titolo parla di scavare per trovarli. I viola sono quelli
 * veri dell'ametista (`content/core/crystals/amethyst.ron`), così il file e la gemma che il
 * programma pianta hanno lo stesso colore.
 */
const digIcon: IconifyIcon = {
  width: 32,
  height: 32,
  body:
    '<path d="M16 3 4.5 12.5 16 29 27.5 12.5z" fill="#8047cc"/>' +
    '<path d="M16 3 4.5 12.5 16 29z" fill="#5b2e96"/>' +
    '<path d="M10.4 12.5 16 29l5.6-16.5z" fill="#a273e6"/>' +
    '<path d="M16 3l-5.6 9.5h11.2z" fill="#c9a4f2"/>' +
    '<path d="M4.5 12.5 10.4 12.5 16 3z" fill="#b98bee"/>' +
    '<path d="M21.6 12.5h5.9L16 3z" fill="#9b63de"/>' +
    '<path d="M4.5 12.5h23L16 29z" fill="#000" opacity=".07"/>',
};

/**
 * `.dev` — uno **scenario di playtest**: un banco che prepara il mondo, lancia un programma e
 * chiede di essere misurato.
 *
 * Un cronometro, e non qualcosa di geode: il formato è quello della console di fulcrum, quindi
 * l'icona deve valere per qualunque gioco ci giri sopra. Un banco è una **corsa misurata** —
 * `watch 30m --every 2m` è letteralmente la sua ultima riga — e il cronometro lo dice senza
 * sapere niente di talpe né di cristalli.
 */
const devIcon: IconifyIcon = {
  width: 32,
  height: 32,
  body:
    '<rect x="13" y="1.8" width="6" height="3.8" rx="1.5" fill="#b96f10"/>' +
    '<path d="M23.4 5.4l3.1-2.2 2.1 2.9-3.1 2.2z" fill="#b96f10"/>' +
    '<circle cx="16" cy="19" r="12" fill="#f0a030"/>' +
    '<circle cx="16" cy="19" r="9.4" fill="#2a2017"/>' +
    '<path d="M16 19V11.9" stroke="#ffd582" stroke-width="2.4" stroke-linecap="round"/>' +
    '<path d="M16 19l4.6 3.1" stroke="#ffd582" stroke-width="2.4" stroke-linecap="round"/>' +
    '<circle cx="16" cy="19" r="1.4" fill="#fff"/>',
};

/**
 * `lombok.config` — le regole con cui Lombok genera codice che nel sorgente non c'è.
 *
 * Un **peperoncino**, e non è una scelta decorativa: *lombok* in giavanese vuol dire peperoncino,
 * ed è da lì che il progetto prende sia il nome sia il proprio marchio. Disegnato qui invece che
 * importato perché nessuno dei due set di icone installati (vscode-icons, simple-icons) ne ha uno:
 * è un marchio nostro che cita il loro, non una copia del loro.
 *
 * Il rosso scende dall'alto verso la punta come in un peperoncino vero, e il picciolo verde è
 * l'unica cosa che a 16 px lo distingue da una goccia — quindi è tenuto spesso.
 */
const lombokIcon: IconifyIcon = {
  width: 32,
  height: 32,
  body:
    // Il picciolo: due tratti, perché uno solo a questa dimensione legge come un gambo di ciliegia.
    '<path d="M15.5 7.5c0-2.6 1.4-4.3 3.6-4.9" stroke="#4f9b3a" stroke-width="2.6" '
    + 'stroke-linecap="round" fill="none"/>'
    + '<path d="M15.6 8.2c-1.7-1.2-3.6-1.3-5.2-.4" stroke="#3f8330" stroke-width="2.2" '
    + 'stroke-linecap="round" fill="none"/>'
    // Il baccello.
    + '<path d="M15.4 7.4c3.6 0 6.4 2.7 6.4 6.6 0 6.2-4.6 11.6-10.6 13.4-1.5.5-2.6-1.2-1.5-2.3 '
    + '3.6-3.5 5.2-7.4 5.2-11.6 0-2.3-1.1-3.6-1.1-4.6 0-1 .7-1.5 1.6-1.5z" fill="#d1352b"/>'
    // La luce sul lato interno della curva, come su un peperoncino lucido.
    + '<path d="M15.4 9.6c1.9.2 3.2 1.7 3.2 4.4 0 4.1-2 7.9-5 10.8 2.3-3.4 3.4-6.9 3.4-10.4 '
    + '0-2.4-1-3.7-1.6-4.8z" fill="#ee6a52"/>',
};

/**
 * `junit-platform.properties` — i parametri con cui la piattaforma JUnit lancia i test.
 *
 * Il glifo vero di JUnit 5 (simple-icons), tinto del verde del progetto invece che lasciato a
 * `currentColor`: nell'albero dei file sta in mezzo a icone a colori, e un'unica icona che assume
 * il colore del testo lì legge come disabilitata, non come monocromatica.
 */
const junitIcon: IconifyIcon = {
  ...junit5Glyph,
  body: junit5Glyph.body.replace(/currentColor/g, '#25a162'),
};

/**
 * `.merula` — un **pattern** della DAW live-coding.
 *
 * Non un uccello, anche se il nome è quello del merlo: il marchio del prodotto
 * (`static/products/merula.svg`) è un **sequencer**, una griglia di step accesi in ambra su viola
 * scuro, e un'icona di file che partisse da un'altra idea farebbe sembrare i due oggetti di due
 * applicazioni diverse. Questa è la stessa griglia, ridotta a quel che sopravvive a 16 px.
 *
 * Tre per tre e non cinque per cinque come il marchio: a 16 px una cella di una griglia 5×5 è
 * meno di tre pixel, e la M che il marchio ci disegna diventa poltiglia. Con nove celle si può
 * dire l'altra cosa che un sequencer è — **una colonna accesa e degli step dopo**, cioè una
 * testina di riproduzione e un ritmo — e quella si legge anche piccola.
 *
 * I colori sono quelli veri del marchio: `#f2600c → #ffd166` per gli step accesi, il viola quasi
 * nero del pannello. Lo spento è tenuto al 16% e non al 7% del marchio grande: lì è
 * controforma, qui a 16 px sotto quella soglia sparisce e restano tre puntini che galleggiano.
 */
const merulaIcon: IconifyIcon = {
  width: 32,
  height: 32,
  body:
    '<rect x="1" y="1" width="30" height="30" rx="7" fill="#1b0e27"/>'
    + '<rect x="1" y="1" width="30" height="30" rx="7" fill="none" stroke="#3a1f4a" stroke-width="1.5"/>'
    // Gli step spenti: la griglia su cui si legge il ritmo.
    + '<g fill="#ffd166" fill-opacity="0.16">'
    + '<rect x="3.5" y="3.5" width="7" height="7" rx="2"/><rect x="12.5" y="3.5" width="7" height="7" rx="2"/>'
    + '<rect x="21.5" y="3.5" width="7" height="7" rx="2"/><rect x="3.5" y="12.5" width="7" height="7" rx="2"/>'
    + '<rect x="12.5" y="12.5" width="7" height="7" rx="2"/><rect x="21.5" y="12.5" width="7" height="7" rx="2"/>'
    + '<rect x="3.5" y="21.5" width="7" height="7" rx="2"/><rect x="12.5" y="21.5" width="7" height="7" rx="2"/>'
    + '<rect x="21.5" y="21.5" width="7" height="7" rx="2"/>'
    + '</g>'
    // La colonna che sta suonando, e i due colpi che seguono. L'ambra scende come nel marchio:
    // il più caldo in basso, il più chiaro in alto.
    + '<g>'
    + '<rect x="3.5" y="3.5" width="7" height="7" rx="2" fill="#ffd166"/>'
    + '<rect x="3.5" y="12.5" width="7" height="7" rx="2" fill="#ff8a1f"/>'
    + '<rect x="3.5" y="21.5" width="7" height="7" rx="2" fill="#f2600c"/>'
    + '<rect x="21.5" y="3.5" width="7" height="7" rx="2" fill="#ffb347"/>'
    + '<rect x="12.5" y="21.5" width="7" height="7" rx="2" fill="#ff8a1f"/>'
    + '</g>',
};

/** lowercase extension → icon */
const EXT_ICONS: Record<string, IconifyIcon> = {
  // Le tre di casa: vedi sopra.
  dig: digIcon,
  dev: devIcon,
  merula: merulaIcon,
  rs: rustIcon,
  ts: tsIcon, tsx: tsIcon,
  js: jsIcon, jsx: jsIcon, mjs: jsIcon, cjs: jsIcon,
  svelte: svelteIcon,
  vue: vueIcon,
  py: pythonIcon, pyi: pythonIcon,
  go: goIcon,
  java: javaIcon,
  // The rest of a JVM project's file kinds: a packaged artifact, a log to read, a
  // keystore not to open in a text editor.
  jar: jarIcon, war: jarIcon, ear: jarIcon,
  log: logIcon,
  jks: keyIcon, p12: keyIcon, keystore: keyIcon, pem: keyIcon,
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
  // JSP and its family. `.tag` / `.tagx` are tag files — JSP with a different entry point, so the
  // same mark — and a `.jspf` is a fragment, which is a JSP that cannot be requested on its own.
  jsp: jspIcon, jspf: jspIcon, jspx: jspIcon, tag: jspIcon, tagx: jspIcon,
  // A `*.pom` is a `pom.xml` under the name the repository files it as — the file you land
  // in by following a dependency. By extension here, because in `~/.m2` the name is the
  // coordinate and no name rule could ever match it.
  pom: mavenIcon,
  html: htmlIcon, htm: htmlIcon,
  css: cssIcon,
  scss: scssIcon,
  sass: sassIcon,
  md: markdownIcon, mdx: mdxIcon, markdown: markdownIcon,
  jinja: jinjaIcon, jinja2: jinjaIcon, j2: jinjaIcon,
  sh: shellIcon, bash: shellIcon, zsh: shellIcon, fish: shellIcon,
  ps1: powershellIcon, psm1: powershellIcon,
  // `.cmd` and `.bat` are the same file with two names — the extension only decides which
  // interpreter Windows picks, not what is in it.
  bat: batIcon, cmd: batIcon,
  graphql: graphqlIcon, gql: graphqlIcon,
  svg: svgFileIcon,
  png: imageIcon, jpg: imageIcon, jpeg: imageIcon, gif: imageIcon,
  webp: imageIcon, ico: imageIcon, bmp: imageIcon, tiff: imageIcon,
  avif: imageIcon,
  zip: zipIcon, gz: zipIcon, tar: zipIcon, bz2: zipIcon,
  xz: zipIcon, rar: zipIcon, '7z': zipIcon, tgz: zipIcon,
  sql: sqlIcon, sqlite: sqlIcon, sqlite3: sqlIcon,
  ron: ronIcon,
  wgsl: wgslIcon,
  glsl: glslIcon, vert: glslIcon, frag: glslIcon, comp: glslIcon,
  hlsl: hlslIcon,
  // The three the browser can load open in the font viewer; `.eot` gets the mark too — it is
  // still a font, it just cannot be shown.
  ttf: fontIcon, otf: fontIcon, woff: fontIcon, woff2: fontIcon, eot: fontIcon,
  docx: wordIcon, doc: wordIcon, dotx: wordIcon,
  xlsx: excelIcon, xlsm: excelIcon, xls: excelIcon, xlsb: excelIcon, xla: excelIcon,
  ods: excelIcon, csv: excelIcon,
  pptx: powerpointIcon, ppt: powerpointIcon,
  pdf: pdfIcon,
  // `log` is NOT repeated here: it is already mapped to the log mark above, and a second
  // entry in the same literal silently replaced it with the plain-text one.
  txt: textIcon, rtf: textIcon,
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
  // ── JVM build files ──────────────────────────────────────────────────────
  // By NAME and not by extension: `pom.xml` is the one XML in a Java project you
  // look for, and an extension rule would give it the same icon as the forty
  // others around it.
  // Tomcat's `context.xml` and `server.xml` are NOT here, though they look like they belong: they
  // are generic names, and what the file is is decided by its root element instead — see
  // `getFileIcon`. Only `catalina.properties` is unambiguous enough to key on the name.
  'catalina.properties': tomcatIcon,
  'pom.xml': mavenIcon,
  '.flattened-pom.xml': mavenIcon,
  'mvnw': mavenIcon,
  'mvnw.cmd': mavenIcon,
  'maven-wrapper.properties': mavenIcon,
  'settings.xml': mavenIcon,
  'build.gradle': gradleIcon,
  'build.gradle.kts': gradleIcon,
  'settings.gradle': gradleIcon,
  'settings.gradle.kts': gradleIcon,
  'gradle.properties': gradleIcon,
  // Lombok e la piattaforma JUnit: due file `.properties`-simili con un vocabolario documentato
  // dietro, e per questo con un'icona propria invece di quella generica dei `.properties`.
  'lombok.config': lombokIcon,
  'junit-platform.properties': junitIcon,
  'gradlew': gradleIcon,
  'gradlew.bat': gradleIcon,
  // ── CI / tooling ─────────────────────────────────────────────────────────
  '.gitlab-ci.yml': gitlabIcon,
  '.gitlab-ci.yaml': gitlabIcon,
  'jenkinsfile': jenkinsIcon,
  '.editorconfig': editorconfigIcon,
  'chart.yaml': helmIcon,
  'docker-compose.yml': dockerIcon,
  'docker-compose.yaml': dockerIcon,
  'compose.yml': dockerIcon,
  'compose.yaml': dockerIcon,
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

/**
 * What an XML's **root element** says the file is.
 *
 * The tier that exists because a name is not evidence. Tomcat's per-application context is
 * `context.xml` inside a `.war` and `<appname>.xml` under `conf/Catalina/localhost`; a Struts
 * module configuration is called whatever `struts.configuration.files` says. The root element is
 * the same in every spelling, so this is the rule that is actually true — which is why it sits
 * *above* every guess made from a name.
 *
 * Only the roots that identify a file unambiguously. `<project>` is Maven's and Ant's; `<beans>`
 * is Spring's and also CDI's; neither earns a mark here, and `pom.xml` gets its own from the
 * exact-name tier above where it is certain.
 */
const ROOT_TAG_ICONS: Record<string, IconifyIcon> = {
  // Tomcat: a server-wide configuration and a per-application context.
  Context: tomcatIcon,
  Server: tomcatIcon,
  'tomcat-users': tomcatIcon,
  // Struts 2 and Struts 1 — different root, same framework, same mark.
  struts: apacheIcon,
  'struts-config': apacheIcon,
};

/**
 * Resolve a file icon.
 *
 * Four tiers, most-certain first, and the order is the whole of it:
 *
 * 1. **an exact name** — `pom.xml` is a pom, and no content check improves on that;
 * 2. **an XML's root element**, when the caller knows it — what the file says it is;
 * 3. **a name pattern** — `Dockerfile.dev`, `struts-orders.xml`: a good guess, and the only thing
 *    left when the content is not to hand (a tab strip has a path and no bytes);
 * 4. **the extension**.
 *
 * `rootTag` comes from the project tree, which reads it while walking. Everywhere else it is
 * absent and the name tiers answer, which is why those still exist.
 */
export function getFileIcon(name: string, rootTag?: string): IconifyIcon {
  const lower = name.toLowerCase();
  if (FILENAME_ICONS[lower]) return FILENAME_ICONS[lower];
  if (rootTag && ROOT_TAG_ICONS[rootTag]) return ROOT_TAG_ICONS[rootTag];
  // Before the extension split, and that ordering is the point: `Dockerfile.dev` read by extension
  // is a `.dev`, which here is geode's playtest format.
  if (isDockerfile(lower)) return dockerIcon;
  // Same reason: read by extension a `struts-security.xml` is one XML among forty, and in a Struts
  // project the config files are the ones you go looking for. Below the root element, because a
  // file whose name says Struts and whose content says otherwise is the content's.
  if (isStrutsConfig(lower)) return apacheIcon;
  if (isTomcatConfig(lower)) return tomcatIcon;
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
