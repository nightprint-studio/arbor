/**
 * Single source of truth for plugin-supplied Lucide icon names.
 *
 * Plugins reference icons by string (e.g. `"Hammer"`, `"Play"`) — they don't
 * import Svelte components. This map turns those strings into renderable
 * components for sidebar items, form fields, activity-bar combos, dropdown
 * options, and pipeline editors.
 *
 * Adding the whole `lucide-svelte` library would tank the bundle, so the map
 * is intentionally curated. Missing names fall back to a generic glyph at the
 * call site (typically `Zap` or `Play` depending on context).
 */
import {
  // Core / placeholders
  Zap, Circle,
  // Run / playback
  Play, Pause, Square, SkipForward, RotateCw, RefreshCw, Shuffle,
  // Build / packaging
  Hammer, Wrench, Package, PackageSearch, PackageCheck, PackagePlus,
  Rocket, Upload, Download, Save, Trash, Trash2, Archive,
  // Editing primitives
  Plus, Minus, Check, X, Pencil, PenSquare, PencilLine, Edit, Edit2,
  Settings, Settings2, Sliders, Cog, Eraser, Undo2,
  CheckSquare,
  // Visibility / search
  Eye, EyeOff, Search, SearchX, ScanSearch, Filter, ListFilter, ListChecks,
  List, LayoutGrid, Table2,
  // Git
  GitBranch, GitBranchPlus, GitMerge, GitPullRequest, GitCommit,
  GitCommitHorizontal, GitCompare, GitFork, Combine, FastForward,
  // Files / folders
  FolderTree, Folder, FolderOpen, FolderPlus, FolderGit2,
  File, FilePlus, FilePlus2, FileText, FileCode, FileJson, FileCog, FileCheck2,
  FileType, FileDown, FileUp, FileSymlink, Files,
  // Code / terminal
  Terminal, TerminalSquare, Code, Code2, Bug, Braces, Keyboard,
  // Time
  Clock, Calendar, Timer, History,
  // Charts
  BarChart2, LineChart, PieChart, Activity, TrendingUp, TrendingDown, Gauge,
  // Status
  CheckCircle, CheckCircle2, XCircle, AlertTriangle, AlertCircle, AlertOctagon,
  Info, HelpCircle, Loader, Loader2,
  // Status glyphs
  Ban, MinusCircle, CirclePause, CircleCheck, CircleX, CircleAlert, CircleDashed,
  // Infra
  Database, Server, Cloud, CloudDownload, CloudUpload, HardDrive, Network,
  Container, Ship,
  // Links
  Link, Link2, ExternalLink, Copy, Clipboard,
  // Communication
  Mail, MessageSquare, MessageSquareOff, Bell, BellRing, Megaphone,
  // Sharing
  Share2, Send, Inbox, Bookmark, BookmarkPlus, Star, Heart,
  // People / auth
  User, Users, Lock, Unlock,
  Shield, ShieldCheck, ShieldAlert, ShieldX, ShieldOff, ShieldQuestion,
  Key, KeyRound, KeySquare,
  Fingerprint,
  // Arrows
  ChevronRight, ChevronDown, ChevronUp, ChevronLeft, ChevronsUpDown,
  ChevronsDownUp, ChevronsRight,
  ArrowRight, ArrowLeft, ArrowUp, ArrowDown,
  ArrowUpToLine, ArrowDownToLine, ArrowLeftRight,
  // Layout
  Workflow, Boxes, Box, Layers, Puzzle, Grid3x3,
  PanelLeft, PanelRight, PanelBottom, PanelTop, SidebarOpen, SidebarClose,
  LayoutPanelLeft, AppWindow,
  // Tags
  Flag, Tag, Hash, AtSign, Percent,
  // World
  Sun, Moon, Globe, Map, MapPin, Compass, Route,
  // Toolchain
  Leaf, Triangle, Coffee,
  // Text / content
  AlignJustify, AlignEndHorizontal, Replace, Scissors,
  Variable, ScrollText,
  // Misc
  CircleSlash, MoreVertical, MoreHorizontal,
  Plug, Unplug, Cpu, Crosshair, Sparkles, Palette, Rewind, StopCircle, Store,
  Bot,
  Gift,
  // Game / inspection
  Gamepad, Gamepad2, ScanLine, Joystick, Pin, PinOff,
  // Product palettes — Bennu, Garrulus, Picus and Merula resolve their commands here rather
  // than in registries of their own.
  Command, ListTree, FileCode2, Wand2, Lightbulb, SlidersHorizontal, Library, Target, ListTodo, IndentIncrease, TextCursorInput, BookOpen, FlaskConical, Beaker, ListRestart, Languages, LayoutDashboard, ArrowUpFromLine, CalendarDays, RotateCcw, FolderCog, FormInput, PackageMinus, Tags, SkipBack, Music4, Minimize2, Maximize2, Piano, FileInput, FileAudio, SearchCode, AlignLeft, PenLine, FolderPen, Repeat, PlayCircle, Hourglass, ZoomIn, ZoomOut, StretchVertical, FileMusic, PackageOpen, Crop, Snowflake, FilePen,
} from 'lucide-svelte';
import type { IconComponent } from '$lib/types/icon';

export const PLUGIN_ICONS: Record<string, any> = {
  Zap, Circle,
  Play, Pause, Square, SkipForward, RotateCw, RefreshCw, Shuffle,
  Hammer, Wrench, Package, PackageSearch, PackageCheck, PackagePlus,
  Rocket, Upload, Download, Save, Trash, Trash2, Archive,
  Plus, Minus, Check, X, Pencil, PenSquare, PencilLine, Edit, Edit2,
  Settings, Settings2, Sliders, Cog, Eraser, Undo2, CheckSquare,
  Eye, EyeOff, Search, SearchX, ScanSearch, Filter, ListFilter, ListChecks,
  List, LayoutGrid, Table2,
  GitBranch, GitBranchPlus, GitMerge, GitPullRequest, GitCommit,
  GitCommitHorizontal, GitCompare, GitFork, Combine, FastForward,
  FolderTree, Folder, FolderOpen, FolderPlus, FolderGit2,
  File, FilePlus, FilePlus2, FileText, FileCode, FileJson, FileCog, FileCheck2,
  FileType, FileDown, FileUp, FileSymlink, Files,
  // Aliases retained for plugins that picked these names early on.
  FileCopy2: Files, FileOutput: FileDown,
  Terminal, TerminalSquare, Code, Code2, Bug, Braces, Keyboard,
  Clock, Calendar, Timer, History,
  BarChart2, LineChart, PieChart, Activity, TrendingUp, TrendingDown, Gauge,
  CheckCircle, CheckCircle2, XCircle, AlertTriangle, AlertCircle, AlertOctagon,
  Info, HelpCircle, Loader, Loader2,
  Ban, MinusCircle, CirclePause, CircleCheck, CircleX, CircleAlert, CircleDashed,
  Database, Server, Cloud, CloudDownload, CloudUpload, HardDrive, Network,
  Container, Ship,
  Link, Link2, ExternalLink, Copy, Clipboard,
  Mail, MessageSquare, MessageSquareOff, Bell, BellRing, Megaphone,
  Share2, Send, Inbox, Bookmark, BookmarkPlus, Star, Heart,
  User, Users, Lock, Unlock,
  Shield, ShieldCheck, ShieldAlert, ShieldX, ShieldOff, ShieldQuestion,
  Key, KeyRound, KeySquare,
  Fingerprint,
  ChevronRight, ChevronDown, ChevronUp, ChevronLeft, ChevronsUpDown, ChevronsDownUp, ChevronsRight,
  ArrowRight, ArrowLeft, ArrowUp, ArrowDown,
  ArrowUpToLine, ArrowDownToLine, ArrowLeftRight,
  Workflow, Boxes, Box, Layers, Puzzle, Grid3x3,
  PanelLeft, PanelRight, PanelBottom, PanelTop, SidebarOpen, SidebarClose,
  LayoutPanelLeft, AppWindow,
  Flag, Tag, Hash, AtSign, Percent,
  Sun, Moon, Globe, Map, MapPin, Compass, Route,
  Leaf, Triangle, Coffee,
  AlignJustify, AlignEndHorizontal, Replace, Scissors,
  Variable, ScrollText,
  CircleSlash, MoreVertical, MoreHorizontal,
  Plug, Unplug, Cpu, Crosshair, Sparkles, Palette, Rewind, StopCircle, Store,
  Bot,
  Gift,
  Gamepad, Gamepad2, ScanLine, Joystick, Pin, PinOff,
  Command, ListTree, FileCode2, Wand2, Lightbulb, SlidersHorizontal, Library, Target, ListTodo, IndentIncrease, TextCursorInput, BookOpen, FlaskConical, Beaker, ListRestart, Languages, LayoutDashboard, ArrowUpFromLine, CalendarDays, RotateCcw, FolderCog, FormInput, PackageMinus, Tags, SkipBack, Music4, Minimize2, Maximize2, Piano, FileInput, FileAudio, SearchCode, AlignLeft, PenLine, FolderPen, Repeat, PlayCircle, Hourglass, ZoomIn, ZoomOut, StretchVertical, FileMusic, PackageOpen, Crop, Snowflake, FilePen,
};

/**
 * Resolve a command-palette icon name — one lucide name, one glyph, for every product.
 *
 * This replaced four private registries (Bennu's, Garrulus's, Picus's, Merula's) that each mapped
 * their own words to glyphs. They drifted the way copies do: a key used in a palette but missing
 * from that product's map fell back to the generic glyph with no error anywhere, so `git-branch`,
 * `download` and `trash` had been rendering as ⌘ for as long as anyone could tell. With one
 * vocabulary, a name either exists or it does not, in one place.
 *
 * Falls back to `Command` — the palette's own glyph — rather than throwing: a missing glyph must
 * never be the reason a verb cannot be reached.
 */
export function paletteIcon(name: string): IconComponent {
  return (PLUGIN_ICONS[name] as IconComponent | undefined) ?? (Command as unknown as IconComponent);
}
