<script lang="ts">
  /**
   * Hirundo documentation — built on the shared `DocsShell`. Topics live in `./docs/`; this file only wires
   * the navigation.
   */
  import { ArrowLeftRight, BookOpen, Braces, Code2, FileText, FlaskConical, FolderTree, Globe, Keyboard, Radio, Rocket, Send, Variable } from 'lucide-svelte';
  import DocsShell, { type DocsNavItem, type DocsNavGroup } from '$lib/components/shared/DocsShell.svelte';
  import GettingStarted from './docs/GettingStarted.svelte';
  import Environments from './docs/Environments.svelte';
  import Tests from './docs/Tests.svelte';
  import Concepts from './docs/Concepts.svelte';
  import Variables from './docs/Variables.svelte';
  import RequestEditor from './docs/RequestEditor.svelte';
  import Response from './docs/Response.svelte';
  import Streams from './docs/Streams.svelte';
  import ImportExport from './docs/ImportExport.svelte';
  import FromCode from './docs/FromCode.svelte';
  import Format from './docs/Format.svelte';
  import Shortcuts from './docs/Shortcuts.svelte';

  let { onClose, initialSection = 'getting-started' }: { onClose: () => void; initialSection?: string } = $props();

  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started', icon: Rocket },
    { id: 'concepts', label: 'Concepts', icon: FolderTree },
  ];

  const navGroups: DocsNavGroup[] = [
    {
      id: 'requests', label: 'Requests', icon: Send,
      items: [
        { id: 'request-editor', label: 'The request editor', icon: Send },
        { id: 'variables', label: 'Variables', icon: Variable },
        { id: 'environments', label: 'Environments', icon: Globe },
        { id: 'response', label: 'The response', icon: FileText },
        { id: 'tests', label: 'Tests and runner', icon: FlaskConical },
      ],
    },
    {
      id: 'protocols', label: 'Beyond plain requests', icon: Radio,
      items: [{ id: 'streams', label: 'Streams, cookies, SOAP', icon: Radio }],
    },
    {
      id: 'sources', label: 'Import and Bennu', icon: ArrowLeftRight,
      items: [
        { id: 'import-export', label: 'Import and export', icon: ArrowLeftRight },
        { id: 'from-code', label: 'Detected from code', icon: Code2 },
      ],
    },
    {
      id: 'reference', label: 'Reference', icon: Keyboard,
      items: [
        { id: 'shortcuts', label: 'Keyboard shortcuts', icon: Keyboard },
        { id: 'format', label: 'On disk', icon: Braces },
      ],
    },
  ];

  const sections = {
    'getting-started': GettingStarted, concepts: Concepts, 'request-editor': RequestEditor, variables: Variables, environments: Environments,
    response: Response, tests: Tests, streams: Streams, 'import-export': ImportExport, 'from-code': FromCode, shortcuts: Shortcuts, format: Format,
  };
</script>

<DocsShell {topItems} {navGroups} {sections} {onClose} title="Hirundo Documentation" headerIcon={BookOpen} product="Hirundo" fileBase="hirundo-docs" {initialSection} />
