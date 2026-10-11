// Fails if a React hook is called after App.tsx's early return.
//
// App returns `<div className="loading">` while the session is still loading,
// so any hook written below that line is only reached on the renders that get
// past it. React then tears the whole component tree down the moment the hook
// count changes, and the symptom is an ENTIRELY BLANK WINDOW — no error on
// screen, nothing on stderr, and `tsc` perfectly happy, because this is not a
// type error. It cost a full rebuild cycle twice in one session.
//
// The real tool for this is eslint-plugin-react-hooks; this is the same check
// for the one file and the one return that matter, without adding a linter and
// its dependency tree to a project that deliberately ships a 2 MB download.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const file = join(dirname(fileURLToPath(import.meta.url)), "..", "app", "src", "App.tsx");
const lines = readFileSync(file, "utf8").split("\n");

const guard = lines.findIndex((line) => line.includes("if (!session || !active) {"));
if (guard === -1) {
  console.error(
    "check-hook-order: could not find App.tsx's early return. If it was renamed or " +
      "removed, update this script — do not delete it.",
  );
  process.exit(1);
}

// A hook CALL at the start of a statement: `useEffect(`, `const x =
// useCallback(`, `const [a, b] = useState(`.
//
// ⚠️ An earlier version of this skipped any line containing `=>`, meaning to
// ignore JSX props like `onDrop={(id) => …}`. That also skipped every real
// `useCallback(() => …)`, so the guard passed against deliberately broken
// code. Anchoring to the start of the statement is what distinguishes them.
const HOOK = /^\s*(?:(?:const|let|var)\s[^=]*=\s*)?use[A-Z][A-Za-z0-9_]*\s*\(/;
const offenders = [];
for (let index = guard + 1; index < lines.length; index += 1) {
  const line = lines[index];
  if (HOOK.test(line) && !line.trimStart().startsWith("//")) {
    offenders.push(`  app/src/App.tsx:${index + 1}: ${line.trim()}`);
  }
}

if (offenders.length) {
  console.error(
    `check-hook-order: hook call(s) below App.tsx's early return on line ${guard + 1}.\n` +
      offenders.join("\n") +
      "\nEvery hook must be above that return, or React will blank the window.",
  );
  process.exit(1);
}

console.log(`check-hook-order: no hooks below App.tsx:${guard + 1}.`);
