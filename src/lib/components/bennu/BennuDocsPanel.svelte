<script lang="ts">
  /**
   * Bennu's manual — the navigation, and nothing else. Search, highlighting, group state
   * and the Markdown / HTML export all belong to the shared `DocsShell`, the same surface
   * Arbor's and merula's docs go through.
   *
   * ## Why so many pages
   *
   * The manual used to be twelve, and two of them carried more than half of it — a single
   * *Editing & navigation* page ran to forty headings, from rainbow brackets to Struts
   * action resolution. A page that long is not found by reading: you land on it from the
   * search box, in the middle, with no idea what section you are in or what else is nearby.
   *
   * So a page here is **one topic**, named after the question it answers, and the groups
   * below are the answer to "where would I look for this" rather than a table of contents
   * for the code. Small pages also make the search results useful, because the page title
   * that comes back with a hit is specific enough to choose between.
   */
  import {
    BookOpen, Rocket, Boxes, PenLine, Keyboard, FolderGit2, FlaskConical, Play, Replace,
    ServerCog, Cog, Languages, Network, Gamepad2, Search, Coffee, FileCode2, Bug, History,
    FileText, Compass, ListTree, Sparkles, ShieldCheck, Wrench, Download, Package, Braces,
    Database, Layers, Palette, Flame, ScrollText, FileType2,
  } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';

  import GettingStarted    from './docs/GettingStarted.svelte';
  // Projects
  import Projects          from './docs/Projects.svelte';
  import ProjectTree       from './docs/ProjectTree.svelte';
  import Jdk               from './docs/Jdk.svelte';
  import Dependencies      from './docs/Dependencies.svelte';
  import PomEditing        from './docs/PomEditing.svelte';
  import ModuleGraph       from './docs/ModuleGraph.svelte';
  import Encodings         from './docs/Encodings.svelte';
  // Editor
  import Editing           from './docs/Editing.svelte';
  import Navigation        from './docs/Navigation.svelte';
  import GoTo              from './docs/GoTo.svelte';
  import Structure         from './docs/Structure.svelte';
  import Completion        from './docs/Completion.svelte';
  import Refactoring       from './docs/Refactoring.svelte';
  import Validation        from './docs/Validation.svelte';
  import LocalHistory      from './docs/LocalHistory.svelte';
  import FileTypes         from './docs/FileTypes.svelte';
  // Search
  import Structural        from './docs/Structural.svelte';
  import StructuralPages   from './docs/StructuralPages.svelte';
  import StructuralRun     from './docs/StructuralRun.svelte';
  // Java & JSP
  import JspEditing        from './docs/JspEditing.svelte';
  import Taglibs           from './docs/Taglibs.svelte';
  import StrutsNavigation  from './docs/StrutsNavigation.svelte';
  import MyBatis           from './docs/MyBatis.svelte';
  import Spring            from './docs/Spring.svelte';
  import Jpa               from './docs/Jpa.svelte';
  import FormAnalysis      from './docs/FormAnalysis.svelte';
  import MessageBundles    from './docs/MessageBundles.svelte';
  import I18n              from './docs/I18n.svelte';
  import XmlSchemas        from './docs/XmlSchemas.svelte';
  import Tomcat            from './docs/Tomcat.svelte';
  // Rust
  import Cargo             from './docs/Cargo.svelte';
  import Bevy              from './docs/Bevy.svelte';
  import Shaders           from './docs/Shaders.svelte';
  // Build, run & test
  import Running           from './docs/Running.svelte';
  import Debugging         from './docs/Debugging.svelte';
  import DebugValues       from './docs/DebugValues.svelte';
  import Testing           from './docs/Testing.svelte';
  // Reference
  import LanguageServers   from './docs/LanguageServers.svelte';
  import LspSetup          from './docs/LspSetup.svelte';
  import TheIndex          from './docs/TheIndex.svelte';
  import Shortcuts         from './docs/Shortcuts.svelte';
  import Appearance        from './docs/Appearance.svelte';

  let { onClose, initialSection = 'getting-started' }: {
    onClose: () => void;
    /** Topic to land on — a caller that opens the docs *about* something passes it here. */
    initialSection?: string;
  } = $props();

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
  ];

  const navGroups: DocsNavGroup[] = [
    { id: 'projects', label: 'Projects', icon: Boxes, items: [
      { id: 'projects',     label: 'Projects',         icon: FolderGit2 },
      { id: 'project-tree', label: 'The project tree', icon: ListTree   },
      { id: 'jdk',          label: 'The JDK',          icon: Coffee     },
      { id: 'dependencies', label: 'Dependencies',     icon: Package    },
      { id: 'pom',          label: 'Editing a pom.xml', icon: FileCode2 },
      { id: 'module-graph', label: 'The module graph', icon: Network    },
      { id: 'encodings',    label: 'Encodings',        icon: FileType2  },
    ] },
    { id: 'editor', label: 'Editor', icon: PenLine, items: [
      { id: 'editing',       label: 'The editor',              icon: PenLine     },
      { id: 'navigation',    label: 'Navigation',              icon: Compass     },
      { id: 'goto',          label: 'Go to class, file, symbol', icon: Search    },
      { id: 'structure',     label: 'Structure & trees',       icon: ListTree    },
      { id: 'completion',    label: 'Completion',              icon: Sparkles    },
      { id: 'refactoring',   label: 'Refactoring & intentions', icon: Wrench     },
      { id: 'validation',    label: 'Validation & problems',   icon: ShieldCheck },
      { id: 'local-history', label: 'Local history',           icon: History     },
      { id: 'file-types',    label: 'Other file types',        icon: FileText    },
    ] },
    { id: 'search', label: 'Structural search', icon: Replace, items: [
      { id: 'structural',       label: 'Structural search',     icon: Replace },
      { id: 'structural-pages', label: 'Searching pages',       icon: FileCode2 },
      { id: 'structural-run',   label: 'Searching & replacing', icon: Search  },
    ] },
    { id: 'java', label: 'Java & JSP', icon: Coffee, items: [
      { id: 'jsp',              label: 'JSP pages',         icon: FileCode2 },
      { id: 'taglibs',          label: 'Tag libraries',     icon: Braces    },
      { id: 'struts',           label: 'Struts navigation', icon: Compass   },
      { id: 'mybatis',          label: 'MyBatis mappers',   icon: Database  },
      { id: 'spring',           label: 'Spring',            icon: Layers    },
      { id: 'jpa',              label: 'JPA',               icon: Database  },
      { id: 'forms',            label: 'Form analysis',     icon: ScrollText },
      { id: 'message-bundles',  label: 'Message bundles',   icon: Languages },
      { id: 'i18n',             label: 'i18n labels',       icon: Languages },
      { id: 'xml-schemas',      label: 'XML schemas',       icon: FileCode2 },
      { id: 'tomcat',           label: 'Tomcat hot-swap',   icon: Flame     },
    ] },
    { id: 'rust', label: 'Rust', icon: Cog, items: [
      { id: 'cargo',   label: 'Rust & Cargo',   icon: Cog      },
      { id: 'bevy',    label: 'Bevy ECS',       icon: Gamepad2 },
      { id: 'shaders', label: 'Shaders (WGSL)', icon: Palette  },
    ] },
    { id: 'run', label: 'Build, run & test', icon: Play, items: [
      { id: 'running',      label: 'Building & running',     icon: Play         },
      { id: 'debugging',    label: 'Debugging',              icon: Bug          },
      { id: 'debug-values', label: 'Frames, values & watches', icon: ListTree   },
      { id: 'testing',      label: 'Testing',                icon: FlaskConical },
    ] },
    { id: 'reference', label: 'Reference', icon: Keyboard, items: [
      { id: 'lsp',       label: 'Language servers',           icon: ServerCog },
      { id: 'lsp-setup', label: 'Installing a language server', icon: Download },
      { id: 'index',     label: 'The index',                  icon: Boxes     },
      { id: 'appearance', label: 'Appearance',                icon: Palette   },
      { id: 'shortcuts', label: 'Keyboard shortcuts',         icon: Keyboard  },
    ] },
  ];

  const sections = {
    'getting-started':   GettingStarted,
    'projects':          Projects,
    'project-tree':      ProjectTree,
    'jdk':               Jdk,
    'dependencies':      Dependencies,
    'pom':               PomEditing,
    'module-graph':      ModuleGraph,
    'encodings':         Encodings,
    'editing':           Editing,
    'navigation':        Navigation,
    'goto':              GoTo,
    'structure':         Structure,
    'completion':        Completion,
    'refactoring':       Refactoring,
    'validation':        Validation,
    'local-history':     LocalHistory,
    'file-types':        FileTypes,
    'structural':        Structural,
    'structural-pages':  StructuralPages,
    'structural-run':    StructuralRun,
    'jsp':               JspEditing,
    'taglibs':           Taglibs,
    'struts':            StrutsNavigation,
    'mybatis':           MyBatis,
    'spring':            Spring,
    'jpa':               Jpa,
    'forms':             FormAnalysis,
    'message-bundles':   MessageBundles,
    'i18n':              I18n,
    'xml-schemas':       XmlSchemas,
    'tomcat':            Tomcat,
    'cargo':             Cargo,
    'bevy':              Bevy,
    'shaders':           Shaders,
    'running':           Running,
    'debugging':         Debugging,
    'debug-values':      DebugValues,
    'testing':           Testing,
    'lsp':               LanguageServers,
    'lsp-setup':         LspSetup,
    'index':             TheIndex,
    'appearance':        Appearance,
    'shortcuts':         Shortcuts,
  };
</script>

<DocsShell
  {topItems}
  {navGroups}
  {sections}
  {onClose}
  {initialSection}
  initialOpenGroup={null}
  title="Bennu Documentation"
  headerIcon={BookOpen}
  product="Bennu"
  fileBase="bennu-docs"
  width="1100px"
  height="720px"
  prebuildSearchIndex
/>
