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
    BookOpen, Rocket, Boxes, PenLine, Keyboard, FolderGit2, FlaskConical, Beaker, Play, Replace,
    ServerCog, Cog, Languages, Network, Gamepad2, Search, Coffee, FileCode2, Bug, History,
    FileText, Compass, ListTree, Sparkles, ShieldCheck, Wrench, Download, Package, Braces,
    Database, Layers, Palette, Flame, ScrollText, FileType2, SlidersHorizontal, ArrowLeftRight,
    ListChecks, FolderPlus, Activity, Mic,
  } from 'lucide-svelte';
  import Voice from './docs/Voice.svelte';
  import type { Component } from 'svelte';
  import type { IconComponent } from '$lib/types/icon';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';

  import GettingStarted    from './docs/GettingStarted.svelte';
  // Projects
  import Projects          from './docs/Projects.svelte';
  import NewProject        from './docs/NewProject.svelte';
  import ProjectTree       from './docs/ProjectTree.svelte';
  import Jdk               from './docs/Jdk.svelte';
  import Dependencies      from './docs/Dependencies.svelte';
  import DependencyVersions from './docs/DependencyVersions.svelte';
  import DependencySecurity from './docs/DependencySecurity.svelte';
  import Maven             from './docs/Maven.svelte';
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
  import ToolConfig        from './docs/ToolConfig.svelte';
  // Search
  import Structural        from './docs/Structural.svelte';
  import StructuralPages   from './docs/StructuralPages.svelte';
  import StructuralRun     from './docs/StructuralRun.svelte';
  // Java & JSP
  import JspEditing        from './docs/JspEditing.svelte';
  import Taglibs           from './docs/Taglibs.svelte';
  import StrutsNavigation  from './docs/StrutsNavigation.svelte';
  import MyBatis           from './docs/MyBatis.svelte';
  import MyBatisStatements from './docs/MyBatisStatements.svelte';
  import MyBatisDynamicSql from './docs/MyBatisDynamicSql.svelte';
  import MyBatisChecks     from './docs/MyBatisChecks.svelte';
  import Spring            from './docs/Spring.svelte';
  import SpringConfiguration from './docs/SpringConfiguration.svelte';
  import SpringWeb         from './docs/SpringWeb.svelte';
  import SpringProxies     from './docs/SpringProxies.svelte';
  import Jpa               from './docs/Jpa.svelte';
  import FormAnalysis      from './docs/FormAnalysis.svelte';
  import MessageBundles    from './docs/MessageBundles.svelte';
  import BeanValidation    from './docs/BeanValidation.svelte';
  import Scheduled         from './docs/Scheduled.svelte';
  import JsonTypes         from './docs/JsonTypes.svelte';
  import MapStruct         from './docs/MapStruct.svelte';
  import JakartaEE         from './docs/JakartaEE.svelte';
  import JaxRs             from './docs/JaxRs.svelte';
  import Mockito           from './docs/Mockito.svelte';
  import AssertJ           from './docs/AssertJ.svelte';
  import DtoLab            from './docs/DtoLab.svelte';
  import Templates         from './docs/Templates.svelte';
  import TemplateLanguage  from './docs/TemplateLanguage.svelte';
  import TemplateReference from './docs/TemplateReference.svelte';
  import I18n              from './docs/I18n.svelte';
  import XmlSchemas        from './docs/XmlSchemas.svelte';
  import Tomcat            from './docs/Tomcat.svelte';
  // Rust
  import Cargo             from './docs/Cargo.svelte';
  import Bevy              from './docs/Bevy.svelte';
  import BevyRemote        from './docs/BevyRemote.svelte';
  import Shaders           from './docs/Shaders.svelte';
  import Profiler          from './docs/Profiler.svelte';
  import Live              from './docs/Live.svelte';
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

  // Imber's pages: one file each under docs/imber, listed (title, order, summary) in imber-pages.json, which
  // `scripts/build-help-index.mjs` also reads to write the Markdown copy under docs/imber and the assistant's help index.
  import imberPages from './docs/imber/imber-pages.json';
  const imberModules = import.meta.glob<{ default: Component }>('./docs/imber/*.svelte', { eager: true });
  const IMBER_ICONS: Record<string, IconComponent> = {
    'imber': Sparkles, 'imber-start': Rocket, 'imber-conversations': ScrollText, 'imber-permissions': ShieldCheck,
    'imber-review': History, 'imber-tools': Wrench, 'imber-running': Activity, 'imber-debugging': Bug,
    'imber-bevy': Gamepad2, 'imber-visuals': Palette, 'imber-skills': BookOpen, 'imber-subagents': Network,
    'imber-decisions': ListChecks, 'imber-cost': SlidersHorizontal, 'imber-editor': PenLine, 'imber-settings': Cog,
    'imber-shortcuts': Keyboard, 'imber-privacy': ShieldCheck, 'imber-faq': Compass,
  };
  const imberItems: DocsNavItem[] = imberPages.map((p) => ({ id: p.id, label: p.label, icon: IMBER_ICONS[p.id] ?? Sparkles }));
  const imberSections: Record<string, Component> = Object.fromEntries(
    imberPages.map((p) => [p.id, imberModules[`./docs/imber/${p.file}.svelte`].default]),
  );

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
  ];

  const navGroups: DocsNavGroup[] = [
    { id: 'projects', label: 'Projects', icon: Boxes, items: [
      { id: 'projects',     label: 'Projects',         icon: FolderGit2 },
      { id: 'new-project',  label: 'New Project',      icon: FolderPlus },
      { id: 'project-tree', label: 'The project tree', icon: ListTree   },
      { id: 'jdk',          label: 'The JDK',          icon: Coffee     },
      { id: 'encodings',    label: 'Encodings',        icon: FileType2  },
    ] },
    { id: 'maven', label: 'Maven & pom', icon: Package, items: [
      { id: 'maven',        label: 'The Maven window',  icon: Play      },
      { id: 'pom',          label: 'Editing a pom.xml', icon: FileCode2 },
      { id: 'dependencies', label: 'Dependencies',      icon: Package   },
      { id: 'dependency-versions', label: 'Dependency versions', icon: ListTree },
      { id: 'dependency-security', label: 'Dependency security', icon: ShieldCheck },
      { id: 'module-graph', label: 'The module graph',  icon: Network   },
    ] },
    { id: 'editor', label: 'Editor', icon: PenLine, items: [
      { id: 'editing',       label: 'The editor',              icon: PenLine     },
      { id: 'navigation',    label: 'Navigation',              icon: Compass     },
      { id: 'goto',          label: 'Go to class, file, symbol', icon: Search    },
      { id: 'structure',     label: 'Structure & trees',       icon: ListTree    },
      { id: 'completion',    label: 'Completion',              icon: Sparkles    },
      { id: 'templates',     label: 'Code templates',          icon: FileCode2   },
      { id: 'template-language',  label: 'The template language', icon: Braces   },
      { id: 'template-reference', label: 'Template reference',    icon: BookOpen },
      { id: 'refactoring',   label: 'Refactoring & intentions', icon: Wrench     },
      { id: 'voice',         label: 'Voice',                    icon: Mic         },
      { id: 'validation',    label: 'Validation & problems',   icon: ShieldCheck },
      { id: 'local-history', label: 'Local history',           icon: History     },
      { id: 'tool-config',   label: 'Tool configuration files', icon: SlidersHorizontal },
      { id: 'file-types',    label: 'Other file types',        icon: FileText    },
      { id: 'xml-schemas',   label: 'XML schemas',             icon: FileCode2   },
    ] },
    { id: 'imber', label: 'Imber, the assistant', icon: Sparkles, items: imberItems },
    { id: 'search', label: 'Structural search', icon: Replace, items: [
      { id: 'structural',       label: 'Structural search',     icon: Replace },
      { id: 'structural-pages', label: 'Searching pages',       icon: FileCode2 },
      { id: 'structural-run',   label: 'Searching & replacing', icon: Search  },
    ] },
    { id: 'jsp', label: 'JSP & Struts', icon: FileCode2, items: [
      { id: 'jsp',              label: 'JSP pages',         icon: FileCode2 },
      { id: 'taglibs',          label: 'Tag libraries',     icon: Braces    },
      { id: 'struts',           label: 'Struts navigation', icon: Compass   },
      { id: 'forms',            label: 'Form analysis',     icon: ScrollText },
      { id: 'message-bundles',  label: 'Message bundles',   icon: Languages },
    ] },
    { id: 'spring-docs', label: 'Spring', icon: Layers, items: [
      { id: 'spring',           label: 'Beans & injection',         icon: Layers            },
      { id: 'spring-config',    label: 'Configuration',             icon: SlidersHorizontal },
      { id: 'spring-web',       label: 'Web & endpoints',           icon: Network           },
      { id: 'spring-proxies',   label: 'Proxies & declarations',    icon: ShieldCheck       },
    ] },
    { id: 'mybatis-docs', label: 'MyBatis', icon: Database, items: [
      { id: 'mybatis',              label: 'Mappers & navigation',   icon: Database  },
      { id: 'mybatis-statements',   label: 'Statements & result maps', icon: FileCode2 },
      { id: 'mybatis-dynamic-sql',  label: 'Dynamic SQL',            icon: Braces    },
      { id: 'mybatis-checks',       label: 'Mapper checks',          icon: ShieldCheck },
    ] },
    { id: 'java', label: 'Java & frameworks', icon: Coffee, items: [
      { id: 'jpa',              label: 'JPA',               icon: Database  },
      { id: 'bean-validation',  label: 'Bean Validation',   icon: ShieldCheck },
      { id: 'scheduled',        label: 'Scheduled jobs',    icon: History   },
      { id: 'json-types',       label: 'JSON types',        icon: Braces    },
      { id: 'mapstruct',        label: 'MapStruct mappers', icon: ArrowLeftRight },
      { id: 'jakarta-ee',       label: 'Jakarta EE (CDI, EJB, servlets)', icon: Boxes },
      { id: 'jaxrs',            label: 'JAX-RS resources',  icon: Network   },
      { id: 'dto-lab',          label: 'DTO Lab',           icon: Beaker    },
      { id: 'i18n',            label: 'i18n labels',       icon: Languages },
      { id: 'tomcat',           label: 'Tomcat hot-swap',   icon: Flame     },
    ] },
    { id: 'rust', label: 'Rust', icon: Cog, items: [
      { id: 'cargo',   label: 'Rust & Cargo',   icon: Cog      },
      { id: 'bevy',    label: 'Bevy ECS',       icon: Gamepad2 },
      { id: 'bevy-remote', label: 'Bevy Remote',  icon: Gamepad2 },
      { id: 'shaders', label: 'Shaders (WGSL)', icon: Palette  },
    ] },
    { id: 'run', label: 'Build, run & test', icon: Play, items: [
      { id: 'running',      label: 'Building & running',     icon: Play         },
      { id: 'debugging',    label: 'Debugging',              icon: Bug          },
      { id: 'debug-values', label: 'Frames, values & watches', icon: ListTree   },
      { id: 'testing',      label: 'Testing',                icon: FlaskConical },
      { id: 'mockito',      label: 'Mockito',                icon: FlaskConical },
      { id: 'assertj',      label: 'AssertJ',                icon: ListChecks   },
      { id: 'profiler',     label: 'Profiler',               icon: Flame        },
      { id: 'live',         label: 'Live services',          icon: Activity     },
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
    'new-project':       NewProject,
    'project-tree':      ProjectTree,
    'jdk':               Jdk,
    'maven':             Maven,
    'dependencies':      Dependencies,
    'dependency-versions': DependencyVersions,
    'dependency-security': DependencySecurity,
    'pom':               PomEditing,
    'module-graph':      ModuleGraph,
    'encodings':         Encodings,
    'editing':           Editing,
    'navigation':        Navigation,
    'goto':              GoTo,
    'structure':         Structure,
    'completion':        Completion,
    'refactoring':       Refactoring,
    'voice':             Voice,
    'validation':        Validation,
    'local-history':     LocalHistory,
    'tool-config':       ToolConfig,
    'file-types':        FileTypes,
    'structural':        Structural,
    'structural-pages':  StructuralPages,
    'structural-run':    StructuralRun,
    'jsp':               JspEditing,
    'taglibs':           Taglibs,
    'struts':            StrutsNavigation,
    'mybatis':           MyBatis,
    'mybatis-statements': MyBatisStatements,
    'mybatis-dynamic-sql': MyBatisDynamicSql,
    'mybatis-checks':    MyBatisChecks,
    'spring':            Spring,
    'spring-config':     SpringConfiguration,
    'spring-web':        SpringWeb,
    'spring-proxies':    SpringProxies,
    'jpa':               Jpa,
    'forms':             FormAnalysis,
    'bean-validation':   BeanValidation,
    'scheduled':         Scheduled,
    'json-types':        JsonTypes,
    'mapstruct':         MapStruct,
    'jakarta-ee':        JakartaEE,
    'jaxrs':             JaxRs,
    'mockito':           Mockito,
    'assertj':           AssertJ,
    'dto-lab':           DtoLab,
    'templates':         Templates,
    'template-language': TemplateLanguage,
    'template-reference': TemplateReference,
    'message-bundles':   MessageBundles,
    'i18n':              I18n,
    'xml-schemas':       XmlSchemas,
    'tomcat':            Tomcat,
    'cargo':             Cargo,
    'bevy':              Bevy,
    'bevy-remote':       BevyRemote,
    'shaders':           Shaders,
    'running':           Running,
    'profiler':          Profiler,
    'live':              Live,
    'debugging':         Debugging,
    'debug-values':      DebugValues,
    'testing':           Testing,
    'lsp':               LanguageServers,
    'lsp-setup':         LspSetup,
    'index':             TheIndex,
    'appearance':        Appearance,
    'shortcuts':         Shortcuts,
    ...imberSections,
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
