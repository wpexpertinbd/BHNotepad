// The editor's look.
//
// Colours are CSS custom properties defined in `styles.css`, so light and dark
// are chosen by `prefers-color-scheme` in one place and the editor follows the
// rest of the window without any JavaScript watching the OS theme.

import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";

export const editorTheme = EditorView.theme({
  "&": {
    color: "var(--text)",
    backgroundColor: "var(--editor-bg)",
    height: "100%",
    fontSize: "var(--editor-font-size)",
  },
  ".cm-content": {
    fontFamily: "var(--mono)",
    caretColor: "var(--caret)",
    padding: "6px 0",
  },
  ".cm-scroller": { fontFamily: "var(--mono)", lineHeight: "1.5" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--caret)", borderLeftWidth: "2px" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "var(--selection)",
  },
  ".cm-activeLine": { backgroundColor: "var(--active-line)" },
  ".cm-gutters": {
    backgroundColor: "var(--gutter-bg)",
    color: "var(--gutter-text)",
    border: "none",
    borderRight: "1px solid var(--border)",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "var(--active-line)",
    color: "var(--gutter-text-active)",
  },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 8px 0 12px" },
  ".cm-selectionMatch": { backgroundColor: "var(--match)" },
  ".cm-searchMatch": {
    backgroundColor: "var(--match)",
    outline: "1px solid var(--match-outline)",
  },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "var(--match-selected)" },
  // The find panel ships as bare browser controls; these make it look like
  // part of the app rather than a form from 1998.
  ".cm-panels": {
    backgroundColor: "var(--panel-bg)",
    color: "var(--text)",
    borderTop: "1px solid var(--border)",
  },
  ".cm-panel.cm-search": { padding: "8px 10px", fontFamily: "var(--ui)" },
  ".cm-panel.cm-search input, .cm-panel.cm-search button, .cm-panel.cm-search label": {
    fontFamily: "var(--ui)",
    fontSize: "12px",
  },
  ".cm-panel.cm-search input[type=text]": {
    background: "var(--input-bg)",
    color: "var(--text)",
    border: "1px solid var(--border)",
    borderRadius: "5px",
    padding: "3px 6px",
    marginRight: "4px",
  },
  ".cm-panel.cm-search button": {
    background: "var(--input-bg)",
    color: "var(--text)",
    border: "1px solid var(--border)",
    borderRadius: "5px",
    padding: "3px 8px",
    marginRight: "4px",
    cursor: "pointer",
  },
  ".cm-panel.cm-search button[name=close]": {
    border: "none",
    background: "transparent",
    fontSize: "16px",
  },
  ".cm-tooltip": {
    background: "var(--panel-bg)",
    border: "1px solid var(--border)",
    color: "var(--text)",
  },
});

export const highlighting = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.keyword, color: "var(--syn-keyword)" },
    { tag: [tags.controlKeyword, tags.moduleKeyword], color: "var(--syn-keyword)" },
    { tag: [tags.name, tags.deleted, tags.character, tags.propertyName], color: "var(--text)" },
    { tag: [tags.variableName], color: "var(--syn-variable)" },
    { tag: [tags.function(tags.variableName), tags.labelName], color: "var(--syn-function)" },
    { tag: [tags.definition(tags.name)], color: "var(--syn-function)" },
    { tag: [tags.typeName, tags.className, tags.namespace], color: "var(--syn-type)" },
    { tag: [tags.constant(tags.name), tags.standard(tags.name)], color: "var(--syn-constant)" },
    { tag: [tags.number, tags.bool, tags.null], color: "var(--syn-number)" },
    { tag: [tags.string, tags.special(tags.string)], color: "var(--syn-string)" },
    { tag: [tags.regexp, tags.escape], color: "var(--syn-regexp)" },
    { tag: [tags.comment, tags.blockComment, tags.lineComment], color: "var(--syn-comment)", fontStyle: "italic" },
    { tag: [tags.meta, tags.processingInstruction], color: "var(--syn-meta)" },
    { tag: tags.operator, color: "var(--syn-operator)" },
    { tag: [tags.tagName], color: "var(--syn-keyword)" },
    { tag: [tags.attributeName], color: "var(--syn-function)" },
    { tag: [tags.attributeValue], color: "var(--syn-string)" },
    { tag: tags.heading, color: "var(--syn-type)", fontWeight: "bold" },
    { tag: tags.link, color: "var(--syn-function)", textDecoration: "underline" },
    { tag: tags.emphasis, fontStyle: "italic" },
    { tag: tags.strong, fontWeight: "bold" },
    { tag: tags.strikethrough, textDecoration: "line-through" },
    { tag: tags.invalid, color: "var(--syn-invalid)" },
  ]),
);
