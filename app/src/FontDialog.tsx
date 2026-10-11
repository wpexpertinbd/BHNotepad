// Choosing the editor font, with a preview — github.com/wpexpertinbd/BHNotepad/issues/1.
//
// The preview is the point of the dialog, not decoration: the request was to
// be able to SEE whether a font renders ANSI box-drawing characters before
// committing to it, and no list of names can answer that.

import { useEffect, useMemo, useRef, useState } from "react";

import * as cmd from "./editorCommands";

interface Props {
  onClose: () => void;
  onChange: (family: string) => void;
}

/** Deliberately a line of CP437 art, an accented word and the characters that
 *  are easy to confuse — the three things a font choice is usually about. */
const PREVIEW = `╔══════════════╗
║ ▓▒░ BHNotepad ║
╚══════════════╝
Il1| O0o  café "quotes"
const sum = (a, b) => a + b;`;

export function FontDialog({ onClose, onChange }: Props) {
  const [family, setFamily] = useState(() => cmd.currentFont());
  const [typed, setTyped] = useState("");
  const [size, setSize] = useState(() => cmd.currentFontSize());
  const found = useMemo(() => cmd.availableFonts(), []);
  const host = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const choose = (next: string) => {
    setFamily(next);
    onChange(cmd.applyFont(next));
  };

  // The preview must show the font being considered, which is the one that is
  // already applied — applying on selection makes the whole editor the preview
  // as well, and Close is then just "I'm done looking".
  const previewFont = family === cmd.SYSTEM_FONT ? "var(--mono)" : `"${family}", monospace`;

  return (
    <div className="finddialog fontdialog" ref={host} role="dialog" aria-label="Font">
      <div className="finddialog-bar">
        <div className="finddialog-tabs">
          <button type="button" className="is-on">
            Font
          </button>
        </div>
        <button type="button" className="finddialog-close" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </div>

      <div className="finddialog-body">
        <div className="row">
          <span>Family</span>
          <select value={family} onChange={(event) => choose(event.target.value)}>
            <option value={cmd.SYSTEM_FONT}>System default</option>
            {found.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
            {family !== cmd.SYSTEM_FONT && !found.includes(family) && (
              <option value={family}>{family} (not installed)</option>
            )}
          </select>
        </div>

        <div className="row">
          <span>Other</span>
          <input
            type="text"
            value={typed}
            placeholder="Any font name on this machine"
            onChange={(event) => setTyped(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && typed.trim()) choose(typed.trim());
            }}
          />
          <button type="button" disabled={!typed.trim()} onClick={() => choose(typed.trim())}>
            Use
          </button>
        </div>

        <div className="row">
          <span>Size</span>
          <button type="button" onClick={() => setSize(cmd.zoomOut())}>
            −
          </button>
          <span className="fontdialog-size">{size} px</span>
          <button type="button" onClick={() => setSize(cmd.zoomIn())}>
            +
          </button>
          <button type="button" onClick={() => setSize(cmd.zoomReset())}>
            Reset
          </button>
        </div>

        <pre
          className="fontdialog-preview"
          style={{ fontFamily: previewFont, fontSize: `${size}px` }}
        >
          {PREVIEW}
        </pre>

        <p className="fontdialog-note">
          Box drawing above looking like boxes means this font can show ANSI art. If a file still
          shows <code>&#xFFFD;</code>, that is its encoding, not the font — use{" "}
          <strong>File → Reopen with Encoding</strong>.
        </p>
      </div>
    </div>
  );
}
