// The CodeMirror surface.
//
// One EditorView is reused for every tab. Each tab's EditorState is kept in a
// map and swapped in on a switch, which preserves its undo history and
// selection the way Notepad++ does — remounting per tab would silently throw
// both away.

import { useEffect, useRef } from "react";
import { Compartment, EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, drawSelection, rectangularSelection, crosshairCursor } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { search, highlightSelectionMatches } from "@codemirror/search";
import { bracketMatching, indentOnInput } from "@codemirror/language";

import type { Buffer } from "./ipc";
import { extensionFor } from "./languages";
import { editorTheme, highlighting } from "./theme";

/** How long after the last keystroke the text is written to disk. */
const AUTOSAVE_MS = 300;
/** Caret/scroll are cheap to lose, so they are reported less eagerly. */
const POSITION_MS = 700;

interface Props {
  buffer: Buffer;
  /** Text for `buffer.id`, already loaded by the parent. */
  text: string;
  /**
   * Ids of every open tab. Used to drop the parked state of tabs that have
   * been closed — without this, each closed tab's document and undo history
   * would be held for the life of the session.
   */
  openIds: string[];
  onTextChanged: (id: string, text: string) => void;
  onPositionChanged: (id: string, caret: number, scrollTop: number) => void;
  /** Called the first time a tab is edited, so the tab bar can show a dot. */
  onFirstEdit: (id: string) => void;
  /** Line/column for the status bar. Throttled, not debounced — it should
   *  track the caret closely without re-rendering on every keystroke. */
  onCaretInfo: (info: { line: number; col: number; selected: number }) => void;
  /** Hands the live EditorView up so the Find dialog can drive the search. */
  onViewReady: (view: EditorView | null) => void;
}

/** How often the status bar's line/column readout is refreshed. */
const CARET_INFO_MS = 90;

export function Editor({
  buffer,
  text,
  openIds,
  onTextChanged,
  onPositionChanged,
  onFirstEdit,
  onCaretInfo,
  onViewReady,
}: Props) {
  const host = useRef<HTMLDivElement | null>(null);
  const view = useRef<EditorView | null>(null);
  const states = useRef(new Map<string, EditorState>());
  const currentId = useRef<string | null>(null);

  // Compartments let language and wrapping be reconfigured without rebuilding
  // the state, which would discard the undo history.
  const language = useRef(new Compartment());
  const wrapping = useRef(new Compartment());

  const textTimer = useRef<number | null>(null);
  const positionTimer = useRef<number | null>(null);
  // Held in refs so the update listener never closes over a stale prop.
  const caretTimer = useRef<number | null>(null);
  const callbacks = useRef({
    onTextChanged,
    onPositionChanged,
    onFirstEdit,
    onCaretInfo,
    onViewReady,
  });
  callbacks.current = {
    onTextChanged,
    onPositionChanged,
    onFirstEdit,
    onCaretInfo,
    onViewReady,
  };
  const dirtyReported = useRef(new Set<string>());

  const makeState = (contents: string, wrap: boolean) =>
    EditorState.create({
      doc: contents,
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        drawSelection(),
        rectangularSelection(),
        crosshairCursor(),
        history(),
        indentOnInput(),
        bracketMatching(),
        highlightSelectionMatches(),
        // The search engine, but NOT `searchKeymap` — that would bind Mod-f to
        // CodeMirror's own docked panel. Find/Replace is a floating dialog
        // here (FindDialog.tsx), driven through these same commands.
        search(),
        keymap.of([...historyKeymap, ...defaultKeymap, indentWithTab]),
        editorTheme,
        highlighting,
        language.current.of([]),
        wrapping.current.of(wrap ? EditorView.lineWrapping : []),
        EditorView.updateListener.of((update) => {
          const id = currentId.current;
          if (!id) return;

          if (update.docChanged) {
            if (!dirtyReported.current.has(id)) {
              dirtyReported.current.add(id);
              callbacks.current.onFirstEdit(id);
            }
            if (textTimer.current !== null) window.clearTimeout(textTimer.current);
            const body = update.state.doc.toString();
            textTimer.current = window.setTimeout(() => {
              callbacks.current.onTextChanged(id, body);
            }, AUTOSAVE_MS);
          }

          if (update.docChanged || update.selectionSet) {
            // Throttled: if a refresh is already pending, let it stand rather
            // than pushing it further out on every keystroke.
            if (caretTimer.current === null) {
              caretTimer.current = window.setTimeout(() => {
                caretTimer.current = null;
                const view = update.view;
                const range = view.state.selection.main;
                const line = view.state.doc.lineAt(range.head);
                callbacks.current.onCaretInfo({
                  line: line.number,
                  col: range.head - line.from + 1,
                  selected: range.to - range.from,
                });
              }, CARET_INFO_MS);
            }
          }

          if (update.docChanged || update.selectionSet || update.geometryChanged) {
            if (positionTimer.current !== null) window.clearTimeout(positionTimer.current);
            const caret = update.state.selection.main.head;
            positionTimer.current = window.setTimeout(() => {
              const scroller = view.current?.scrollDOM;
              const line = scroller
                ? view.current!.state.doc.lineAt(
                    view.current!.lineBlockAtHeight(scroller.scrollTop).from,
                  ).number - 1
                : 0;
              callbacks.current.onPositionChanged(id, caret, line);
            }, POSITION_MS);
          }
        }),
      ],
    });

  // Create the view once.
  useEffect(() => {
    if (!host.current) return;
    const instance = new EditorView({
      state: makeState(text, buffer.word_wrap),
      parent: host.current,
    });
    view.current = instance;
    currentId.current = buffer.id;
    states.current.set(buffer.id, instance.state);
    instance.focus();
    callbacks.current.onViewReady(instance);

    return () => {
      callbacks.current.onViewReady(null);
      // Anything still pending must reach disk; a React remount in development
      // would otherwise drop the last few hundred milliseconds of typing.
      if (textTimer.current !== null) {
        window.clearTimeout(textTimer.current);
        const id = currentId.current;
        if (id) callbacks.current.onTextChanged(id, instance.state.doc.toString());
      }
      if (positionTimer.current !== null) window.clearTimeout(positionTimer.current);
      if (caretTimer.current !== null) window.clearTimeout(caretTimer.current);
      instance.destroy();
      view.current = null;
    };
    // Intentionally once: later tab changes are handled by the effect below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Swap documents when the active tab changes.
  useEffect(() => {
    const instance = view.current;
    if (!instance) return;
    if (currentId.current === buffer.id) return;

    // Flush the outgoing tab before its state is parked, so nothing typed in
    // the last instant is lost by switching away.
    const leaving = currentId.current;
    if (leaving && textTimer.current !== null) {
      window.clearTimeout(textTimer.current);
      textTimer.current = null;
      callbacks.current.onTextChanged(leaving, instance.state.doc.toString());
    }
    if (leaving) states.current.set(leaving, instance.state);

    const saved = states.current.get(buffer.id);
    currentId.current = buffer.id;
    instance.setState(saved ?? makeState(text, buffer.word_wrap));
    if (!saved) states.current.set(buffer.id, instance.state);

    // A tab restored from a previous run opens where it was left.
    if (!saved && buffer.caret > 0) {
      const max = instance.state.doc.length;
      const head = Math.min(buffer.caret, max);
      instance.dispatch({
        selection: { anchor: head },
        effects: EditorView.scrollIntoView(head, { y: "center" }),
      });
    }
    instance.focus();
  }, [buffer.id, buffer.caret, buffer.word_wrap, text]);

  // Highlighting follows the tab's language.
  useEffect(() => {
    let cancelled = false;
    void extensionFor(buffer.language).then((extension) => {
      if (cancelled || !view.current) return;
      view.current.dispatch({
        effects: language.current.reconfigure(extension ?? []),
      });
    });
    return () => {
      cancelled = true;
    };
  }, [buffer.language, buffer.id]);

  useEffect(() => {
    view.current?.dispatch({
      effects: wrapping.current.reconfigure(
        buffer.word_wrap ? EditorView.lineWrapping : [],
      ),
    });
  }, [buffer.word_wrap, buffer.id]);

  // Forget tabs that are no longer open, along with their undo history.
  useEffect(() => {
    const open = new Set(openIds);
    for (const id of [...states.current.keys()]) {
      if (!open.has(id)) {
        states.current.delete(id);
        dirtyReported.current.delete(id);
      }
    }
  }, [openIds]);

  return <div className="editor" ref={host} />;
}
