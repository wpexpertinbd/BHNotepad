// Editor actions for the Edit / Search / View menus.
//
// Most of these are CodeMirror commands that were already in the bundle and
// simply had nothing bound to them — moving a line, duplicating one, toggling
// a comment. The handful written out here are the ones CodeMirror has no
// opinion about: case conversion, trimming trailing space.

import { EditorView } from "@codemirror/view";
import {
  copyLineDown,
  deleteLine,
  indentLess,
  indentMore,
  moveLineDown,
  moveLineUp,
  redo,
  selectAll,
  toggleComment,
  undo,
} from "@codemirror/commands";
import { gotoLine } from "@codemirror/search";

export type Command = (view: EditorView) => void;

const run = (fn: (view: EditorView) => boolean): Command => (view) => {
  fn(view);
  view.focus();
};

export const editorUndo = run(undo);
export const editorRedo = run(redo);
export const editorSelectAll = run(selectAll);
export const editorDuplicateLine = run(copyLineDown);
export const editorDeleteLine = run(deleteLine);
export const editorMoveLineUp = run(moveLineUp);
export const editorMoveLineDown = run(moveLineDown);
export const editorIndent = run(indentMore);
export const editorOutdent = run(indentLess);
export const editorToggleComment = run(toggleComment);
export const editorGoToLine = run(gotoLine);

/**
 * Replaces the selection, or the whole document when nothing is selected.
 *
 * Operating on the whole document when there is no selection is what Notepad++
 * does for Trim Trailing Space, and it is almost always what is wanted.
 */
function transformSelection(view: EditorView, transform: (text: string) => string): void {
  const { state } = view;
  const ranges = state.selection.ranges.filter((range) => !range.empty);

  if (ranges.length === 0) {
    const whole = state.doc.toString();
    const replaced = transform(whole);
    if (replaced === whole) {
      view.focus();
      return;
    }
    view.dispatch({
      changes: { from: 0, to: state.doc.length, insert: replaced },
      // Keep the caret where it was rather than jumping to the end.
      selection: { anchor: Math.min(state.selection.main.head, replaced.length) },
    });
    view.focus();
    return;
  }

  view.dispatch({
    changes: ranges.map((range) => ({
      from: range.from,
      to: range.to,
      insert: transform(state.sliceDoc(range.from, range.to)),
    })),
  });
  view.focus();
}

export const editorUpperCase: Command = (view) =>
  transformSelection(view, (text) => text.toUpperCase());

export const editorLowerCase: Command = (view) =>
  transformSelection(view, (text) => text.toLowerCase());

/** Removes spaces and tabs at the end of every line. */
export const editorTrimTrailing: Command = (view) =>
  transformSelection(view, (text) => text.replace(/[ \t]+$/gm, ""));

// ── Clipboard ────────────────────────────────────────────────────────────
//
// The keyboard shortcuts are handled by the webview itself and always work.
// These exist so the Edit menu is not missing the three entries everyone
// looks for first; they go through the async clipboard API, which can be
// refused, so each reports rather than failing silently.

export async function editorCopy(view: EditorView): Promise<string | null> {
  const text = view.state.sliceDoc(
    view.state.selection.main.from,
    view.state.selection.main.to,
  );
  if (!text) return null;
  try {
    await navigator.clipboard.writeText(text);
    view.focus();
    return null;
  } catch {
    return "Could not reach the clipboard — the keyboard shortcut still works";
  }
}

export async function editorCut(view: EditorView): Promise<string | null> {
  const { from, to } = view.state.selection.main;
  if (from === to) return null;
  const failure = await editorCopy(view);
  if (failure) return failure;
  view.dispatch({ changes: { from, to, insert: "" } });
  view.focus();
  return null;
}

export async function editorPaste(view: EditorView): Promise<string | null> {
  try {
    const text = await navigator.clipboard.readText();
    if (!text) return null;
    const { from, to } = view.state.selection.main;
    view.dispatch({
      changes: { from, to, insert: text },
      selection: { anchor: from + text.length },
    });
    view.focus();
    return null;
  } catch {
    return "Could not read the clipboard — the keyboard shortcut still works";
  }
}

// ── Zoom ─────────────────────────────────────────────────────────────────

const MIN_FONT = 8;
const MAX_FONT = 36;
const DEFAULT_FONT = 13;
/** Remembered per machine; a per-viewer convenience, not session state. */
const ZOOM_KEY = "bhnotepad.fontSize";

export function currentFontSize(): number {
  try {
    const stored = Number(localStorage.getItem(ZOOM_KEY));
    if (Number.isFinite(stored) && stored >= MIN_FONT && stored <= MAX_FONT) return stored;
  } catch {
    // Private windows and blocked site data both throw; the default is fine.
  }
  return DEFAULT_FONT;
}

export function applyFontSize(size: number): number {
  const clamped = Math.min(MAX_FONT, Math.max(MIN_FONT, Math.round(size)));
  document.documentElement.style.setProperty("--editor-font-size", `${clamped}px`);
  try {
    localStorage.setItem(ZOOM_KEY, String(clamped));
  } catch {
    // Not worth telling anyone about: the size still applies for this run.
  }
  return clamped;
}

export const zoomIn = () => applyFontSize(currentFontSize() + 1);
export const zoomOut = () => applyFontSize(currentFontSize() - 1);
export const zoomReset = () => applyFontSize(DEFAULT_FONT);

// ── Theme ────────────────────────────────────────────────────────────────

/**
 * Light, dark, or whatever the operating system is set to.
 *
 * "system" is the default and needs no JavaScript at all — `styles.css`
 * answers it with `prefers-color-scheme`, which is why dark mode has worked
 * since the first build without anything here. The other two exist because
 * that is not enough on its own: Notepad++'s dark mode is independent of the
 * Windows theme, and someone running a light desktop with a dark editor (or
 * the reverse) has no way to say so through the OS.
 */
export type ThemeChoice = "system" | "light" | "dark";

/** Remembered per machine, like the zoom level. */
const THEME_KEY = "bhnotepad.theme";

export const THEME_LABELS: Record<ThemeChoice, string> = {
  system: "Follow System",
  light: "Light",
  dark: "Dark",
};

export function currentTheme(): ThemeChoice {
  try {
    const stored = localStorage.getItem(THEME_KEY);
    if (stored === "light" || stored === "dark" || stored === "system") return stored;
  } catch {
    // Private windows and blocked site data both throw; the default is fine.
  }
  return "system";
}

export function applyTheme(choice: ThemeChoice): ThemeChoice {
  const root = document.documentElement;
  if (choice === "system") {
    // No attribute at all, so the `prefers-color-scheme` rules decide. Setting
    // data-theme="system" would instead pin it to the light tokens.
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", choice);
  }
  try {
    localStorage.setItem(THEME_KEY, choice);
  } catch {
    // Not worth telling anyone about: the choice still applies for this run.
  }
  return choice;
}

// ── Font ─────────────────────────────────────────────────────────────────

/**
 * The editor font, asked for in issue #1.
 *
 * `--mono` is the one place the family is decided, so changing it moves the
 * editor, the gutter and the Find dialog together. The stored value is a
 * family NAME; the generic fallbacks are appended here so a font that is
 * uninstalled later degrades to a monospace rather than to the UI font.
 */
const FONT_KEY = "bhnotepad.fontFamily";

/** What `--mono` is without a choice: whatever the platform calls monospace. */
export const SYSTEM_FONT = "";

/**
 * Fonts worth offering, if the machine has them.
 *
 * ⚠️ A webview cannot list installed fonts — `queryLocalFonts()` is Chromium
 * only and WebKit has nothing equivalent — so this is a curated list that gets
 * PROBED, and the box below it accepts any name for anything not here. The
 * list leans towards fonts with box-drawing glyphs, because the request that
 * started this was about reading ANSI art.
 */
export const FONT_CANDIDATES = [
  // macOS
  "Menlo", "Monaco", "SF Mono", "Andale Mono", "PT Mono", "Courier New",
  // Windows
  "Consolas", "Cascadia Mono", "Cascadia Code", "Lucida Console", "Lucida Sans Typewriter",
  // Linux
  "DejaVu Sans Mono", "Liberation Mono", "Ubuntu Mono", "Noto Sans Mono", "FreeMono",
  // Installed by developers on any platform
  "JetBrains Mono", "Fira Code", "Fira Mono", "Source Code Pro", "IBM Plex Mono",
  "Hack", "Inconsolata", "Roboto Mono", "Iosevka", "Terminus", "Perfect DOS VGA 437",
];

/**
 * Whether a font is actually installed.
 *
 * Measures a string in the candidate against three different generic
 * fallbacks. ⚠️ Comparing against ONE generic is the usual version of this
 * trick and it is wrong: if the candidate IS that generic's resolved font —
 * Menlo on a Mac, say — the widths match and the font is reported missing.
 * A font only counts as absent when it measures identically to all three.
 */
function installed(family: string, canvas: CanvasRenderingContext2D): boolean {
  const sample = "mmmmmmmmmmlli╔═╗█▒ WiI1l0O";
  const size = "72px";
  return ["monospace", "serif", "sans-serif"].some((generic) => {
    canvas.font = `${size} ${generic}`;
    const base = canvas.measureText(sample).width;
    canvas.font = `${size} "${family}", ${generic}`;
    return canvas.measureText(sample).width !== base;
  });
}

/** The candidates this machine actually has, in the order listed. */
export function availableFonts(): string[] {
  try {
    const context = document.createElement("canvas").getContext("2d");
    if (!context) return [];
    return FONT_CANDIDATES.filter((family) => installed(family, context));
  } catch {
    // Measuring is a nicety; the free-text box still works without it.
    return [];
  }
}

export function currentFont(): string {
  try {
    return localStorage.getItem(FONT_KEY) ?? SYSTEM_FONT;
  } catch {
    return SYSTEM_FONT;
  }
}

export function applyFont(family: string): string {
  const chosen = family.trim();
  const root = document.documentElement;
  if (chosen === SYSTEM_FONT) {
    // Back to the stack declared in styles.css.
    root.style.removeProperty("--mono");
  } else {
    // The generics stay on the end: an uninstalled font must fall back to a
    // monospace, not to whatever the UI font is — proportional text in an
    // editor turns ANSI art into noise.
    root.style.setProperty(
      "--mono",
      `"${chosen.replace(/"/g, "")}", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace`,
    );
  }
  try {
    localStorage.setItem(FONT_KEY, chosen);
  } catch {
    // Not worth telling anyone about: it still applies for this run.
  }
  return chosen;
}
