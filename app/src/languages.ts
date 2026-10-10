// Maps the core's language id to a CodeMirror extension.
//
// The ids come from `bhn-core`'s language table, so detection is identical on
// macOS, Windows and Linux and lives in one place. This file only decides how
// to *highlight* an id.
//
// Every loader is a dynamic import, so Vite splits each grammar into its own
// chunk and the app starts without parsing grammars for languages this session
// never opens. An id with no loader simply gets no highlighting — never a
// wrong one.

import { StreamLanguage } from "@codemirror/language";
import type { Extension } from "@codemirror/state";

type Loader = () => Promise<Extension>;

/** Wraps one of the CodeMirror 5 modes shipped in `legacy-modes`. */
const legacy = (
  load: () => Promise<Record<string, unknown>>,
  exportName: string,
): Loader => async () => {
  const mod = await load();
  // The legacy modes are plain CodeMirror 5 mode objects; StreamLanguage is
  // what adapts them to CodeMirror 6.
  return StreamLanguage.define(mod[exportName] as never);
};

const loaders: Record<string, Loader> = {
  // ── First-class CodeMirror 6 grammars ────────────────────────────────────
  javascript: async () => (await import("@codemirror/lang-javascript")).javascript(),
  jsx: async () => (await import("@codemirror/lang-javascript")).javascript({ jsx: true }),
  typescript: async () =>
    (await import("@codemirror/lang-javascript")).javascript({ typescript: true }),
  tsx: async () =>
    (await import("@codemirror/lang-javascript")).javascript({ jsx: true, typescript: true }),
  python: async () => (await import("@codemirror/lang-python")).python(),
  html: async () => (await import("@codemirror/lang-html")).html(),
  css: async () => (await import("@codemirror/lang-css")).css(),
  json: async () => (await import("@codemirror/lang-json")).json(),
  markdown: async () => (await import("@codemirror/lang-markdown")).markdown(),
  sql: async () => (await import("@codemirror/lang-sql")).sql(),
  xml: async () => (await import("@codemirror/lang-xml")).xml(),
  rust: async () => (await import("@codemirror/lang-rust")).rust(),
  cpp: async () => (await import("@codemirror/lang-cpp")).cpp(),
  java: async () => (await import("@codemirror/lang-java")).java(),
  php: async () => (await import("@codemirror/lang-php")).php(),
  yaml: async () => (await import("@codemirror/lang-yaml")).yaml(),
  go: async () => (await import("@codemirror/lang-go")).go(),

  // C has no grammar of its own; the C++ one handles it, which is also what
  // Notepad++ does with its cpp lexer.
  c: async () => (await import("@codemirror/lang-cpp")).cpp(),
  // SCSS and Less are supersets of CSS. Highlighting them as CSS is right for
  // the bulk of a file and costs nothing extra in the bundle.
  scss: async () => (await import("@codemirror/lang-css")).css(),
  less: async () => (await import("@codemirror/lang-css")).css(),

  // ── CodeMirror 5 modes via legacy-modes ──────────────────────────────────
  shell: legacy(() => import("@codemirror/legacy-modes/mode/shell"), "shell"),
  lua: legacy(() => import("@codemirror/legacy-modes/mode/lua"), "lua"),
  perl: legacy(() => import("@codemirror/legacy-modes/mode/perl"), "perl"),
  ruby: legacy(() => import("@codemirror/legacy-modes/mode/ruby"), "ruby"),
  powershell: legacy(() => import("@codemirror/legacy-modes/mode/powershell"), "powerShell"),
  properties: legacy(() => import("@codemirror/legacy-modes/mode/properties"), "properties"),
  toml: legacy(() => import("@codemirror/legacy-modes/mode/toml"), "toml"),
  dockerfile: legacy(() => import("@codemirror/legacy-modes/mode/dockerfile"), "dockerFile"),
  nginx: legacy(() => import("@codemirror/legacy-modes/mode/nginx"), "nginx"),
  diff: legacy(() => import("@codemirror/legacy-modes/mode/diff"), "diff"),
  vb: legacy(() => import("@codemirror/legacy-modes/mode/vb"), "vb"),
  swift: legacy(() => import("@codemirror/legacy-modes/mode/swift"), "swift"),
  haskell: legacy(() => import("@codemirror/legacy-modes/mode/haskell"), "haskell"),
  clojure: legacy(() => import("@codemirror/legacy-modes/mode/clojure"), "clojure"),
  erlang: legacy(() => import("@codemirror/legacy-modes/mode/erlang"), "erlang"),
  tcl: legacy(() => import("@codemirror/legacy-modes/mode/tcl"), "tcl"),
  r: legacy(() => import("@codemirror/legacy-modes/mode/r"), "r"),
  verilog: legacy(() => import("@codemirror/legacy-modes/mode/verilog"), "verilog"),
  vhdl: legacy(() => import("@codemirror/legacy-modes/mode/vhdl"), "vhdl"),
  stex: legacy(() => import("@codemirror/legacy-modes/mode/stex"), "stex"),
  cmake: legacy(() => import("@codemirror/legacy-modes/mode/cmake"), "cmake"),
  protobuf: legacy(() => import("@codemirror/legacy-modes/mode/protobuf"), "protobuf"),
  asm: legacy(() => import("@codemirror/legacy-modes/mode/gas"), "gas"),
  csharp: legacy(() => import("@codemirror/legacy-modes/mode/clike"), "csharp"),
};

/** Language ids the user can pick from the status bar, in menu order. */
export const SELECTABLE: ReadonlyArray<{ id: string; label: string }> = [
  { id: "plain", label: "Plain text" },
  { id: "asm", label: "Assembly" },
  { id: "shell", label: "Bash / Shell" },
  { id: "c", label: "C" },
  { id: "csharp", label: "C#" },
  { id: "cpp", label: "C++" },
  { id: "clojure", label: "Clojure" },
  { id: "cmake", label: "CMake" },
  { id: "css", label: "CSS" },
  { id: "diff", label: "Diff / Patch" },
  { id: "dockerfile", label: "Dockerfile" },
  { id: "erlang", label: "Erlang" },
  { id: "go", label: "Go" },
  { id: "haskell", label: "Haskell" },
  { id: "html", label: "HTML" },
  { id: "java", label: "Java / Kotlin" },
  { id: "javascript", label: "JavaScript" },
  { id: "jsx", label: "JavaScript (JSX)" },
  { id: "json", label: "JSON" },
  { id: "stex", label: "LaTeX" },
  { id: "less", label: "Less" },
  { id: "lua", label: "Lua" },
  { id: "markdown", label: "Markdown" },
  { id: "nginx", label: "nginx" },
  { id: "perl", label: "Perl" },
  { id: "php", label: "PHP" },
  { id: "powershell", label: "PowerShell" },
  { id: "properties", label: "Properties / INI" },
  { id: "protobuf", label: "Protobuf" },
  { id: "python", label: "Python" },
  { id: "r", label: "R" },
  { id: "ruby", label: "Ruby" },
  { id: "rust", label: "Rust" },
  { id: "scss", label: "SCSS / Sass" },
  { id: "sql", label: "SQL" },
  { id: "swift", label: "Swift" },
  { id: "toml", label: "TOML" },
  { id: "typescript", label: "TypeScript" },
  { id: "tsx", label: "TypeScript (TSX)" },
  { id: "verilog", label: "Verilog" },
  { id: "vhdl", label: "VHDL" },
  { id: "vb", label: "Visual Basic" },
  { id: "xml", label: "XML" },
  { id: "yaml", label: "YAML" },
];

const labels = new Map(SELECTABLE.map((entry) => [entry.id, entry.label]));

/** Human-readable name for a language id, for the status bar. */
export function labelFor(id: string): string {
  return labels.get(id) ?? id;
}

// Grammars are small but parsing one is not free, and tabs are switched
// constantly. Caching by id means each is built once per run.
const cache = new Map<string, Extension>();

/**
 * Resolves a language id to a highlighting extension.
 *
 * Returns `null` for plain text and for any id with no grammar, which the
 * editor treats as "no highlighting" rather than an error. A grammar that
 * fails to load must not take the editor down with it — the text still has to
 * be readable and editable.
 */
export async function extensionFor(id: string): Promise<Extension | null> {
  if (!id || id === "plain") return null;
  const cached = cache.get(id);
  if (cached) return cached;

  const loader = loaders[id];
  if (!loader) return null;

  try {
    const extension = await loader();
    cache.set(id, extension);
    return extension;
  } catch (error) {
    console.warn(`BHNotepad: no highlighting for "${id}":`, error);
    return null;
  }
}
