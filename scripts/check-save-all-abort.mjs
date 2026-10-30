// Guards the two rules that stop Save All trapping the user in a queue of
// save sheets. Both were real: on 2026-10-30 a session of 21 tabs, 16 of them
// untitled scratch notes, queued 16 unescapable dialogs, and because a save
// panel is a window-modal sheet the quit path was blocked too — the app had to
// be killed from a shell.
//
// 1. Cancelling a Save All sheet must ABORT the run (`break`), never skip the
//    one tab and open the next (`continue`).
// 2. On macOS, Cmd+Shift+S must be Save As, not Save All. That key means Save
//    As in every Mac application, and binding Save All to it is what made a
//    one-file save turn into sixteen prompts. Windows and Linux keep the
//    Notepad++ layout, so this is asserted as a platform SWAP, not a global
//    change.
//
// Kept as source checks because this project has no JS test runner; the
// patterns below tolerate whitespace and reformatting rather than pinning an
// exact spelling.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const file = join(root, "app", "src", "App.tsx");
const src = readFileSync(file, "utf8");

const problems = [];

/** Returns the body of a `const <name> = useCallback(` through its closing brace. */
function body(name) {
  const start = src.indexOf(`const ${name} = useCallback(`);
  if (start === -1) return null;
  let depth = 0;
  for (let i = src.indexOf("{", start); i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  return null;
}

// --- 1. Cancel aborts the whole run ----------------------------------------
const saveAll = body("handleSaveAll");
if (!saveAll) {
  problems.push("handleSaveAll not found in App.tsx — this guard needs updating.");
} else {
  // The branch taken when the save dialog returns nothing (the user cancelled).
  const cancel = saveAll.match(/if\s*\(\s*!\s*picked\s*\)\s*\{([\s\S]*?)\}/);
  if (!cancel) {
    problems.push(
      "handleSaveAll: could not find the `if (!picked) { … }` cancel branch. " +
        "If the cancel path was rewritten, update this guard to match it.",
    );
  } else if (/\bcontinue\b/.test(cancel[1])) {
    problems.push(
      "handleSaveAll: cancelling a save sheet uses `continue`, so it skips one tab " +
        "and immediately opens the next sheet. With a session of untitled scratch " +
        "tabs — the normal state of this app — that is one unescapable dialog per " +
        "tab. Cancel must `break` and end the run.",
    );
  } else if (!/\bbreak\b/.test(cancel[1])) {
    problems.push(
      "handleSaveAll: the cancel branch neither breaks nor continues. Cancel must " +
        "`break` so the run ends on the first cancellation.",
    );
  }
}

// --- 2. macOS swaps Save As and Save All ------------------------------------
const key = src.match(/case "s":([\s\S]*?)return;/);
if (!key) {
  problems.push('Could not find the `case "s":` keyboard branch in App.tsx.');
} else {
  const branch = key[1];
  if (!/ON_MAC/.test(branch)) {
    problems.push(
      'The `case "s":` branch no longer mentions ON_MAC. On macOS, Cmd+Shift+S must ' +
        "be Save As (the Mac-wide convention) while Windows and Linux keep the " +
        "Notepad++ layout where Shift is Save All.",
    );
  } else {
    // Inside `if (event.shiftKey)`, the macOS arm must save ONE file, not all.
    const shift = branch.match(/if\s*\(\s*event\.shiftKey\s*\)\s*\{([\s\S]*?)\}\s*else/);
    if (!shift) {
      problems.push('Could not find the shiftKey arm of the `case "s":` branch.');
    } else if (!/ON_MAC\s*\)?\s*(?:void\s+)?current\.handleSave\(/.test(shift[1])) {
      problems.push(
        "Cmd+Shift+S on macOS no longer calls handleSave. That key is Save As in " +
          "every Mac application; binding Save All to it is what turned one save " +
          "into a queue of sixteen prompts.",
      );
    }
  }

  // The menu has to advertise the same keys the handler listens for, or the
  // app tells the user one thing and does another.
  const menu = src.match(/label:\s*"Save As…",\s*shortcut:\s*([^,]+),/);
  if (!menu || !/ON_MAC/.test(menu[1])) {
    problems.push(
      'The File menu\'s "Save As…" shortcut is not platform-dependent, so the menu ' +
        "advertises a different key from the one the handler listens for on macOS.",
    );
  }
}

if (problems.length > 0) {
  console.error("check-save-all-abort: FAILED\n");
  for (const p of problems) console.error("  • " + p + "\n");
  process.exit(1);
}

console.error("check-save-all-abort: Cancel ends a Save All run, and macOS binds ⌘⇧S to Save As.");
