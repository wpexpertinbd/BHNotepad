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
