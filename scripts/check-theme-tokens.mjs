// Fails if the two copies of the dark palette in app/src/styles.css drift apart.
//
// There are two on purpose — see the KEEP-IN-SYNC comment in that file — because
// CSS cannot share a declaration block between `@media (prefers-color-scheme:
// dark)` (the automatic, flash-free default) and `:root[data-theme="dark"]`
// (the explicit choice from View → Theme, which has to work on a light OS).
//
// A comment alone would not survive someone tweaking one colour: the symptom
// is an editor whose forced dark mode looks subtly different from its automatic
// one, which nobody would think to check. This makes it a build failure.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const css = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "..", "app", "src", "styles.css"),
  "utf8",
);

/** The declarations inside the first `{ … }` that follows `marker`. */
function declarations(marker) {
  const at = css.indexOf(marker);
  if (at === -1) throw new Error(`check-theme-tokens: could not find ${marker}`);
  const open = css.indexOf("{", at + marker.length);
  const close = css.indexOf("}", open);
  if (open === -1 || close === -1) throw new Error(`check-theme-tokens: ${marker} is unclosed`);
  return css
    .slice(open + 1, close)
    .split(";")
    .map((line) => line.trim().replace(/\s+/g, " "))
    .filter((line) => line.startsWith("--"))
    .sort();
}

const automatic = declarations(':root:not([data-theme="light"])');
const forced = declarations(':root[data-theme="dark"]');

if (automatic.length === 0) {
  console.error("check-theme-tokens: the automatic dark palette is empty");
  process.exit(1);
}

const only = (a, b) => a.filter((line) => !b.includes(line));
const missing = only(automatic, forced);
const extra = only(forced, automatic);

if (missing.length || extra.length) {
  console.error("check-theme-tokens: the two dark palettes in app/src/styles.css disagree.");
  for (const line of missing) console.error(`  only in the @media copy:      ${line}`);
  for (const line of extra) console.error(`  only in the [data-theme] copy: ${line}`);
  console.error("Both must declare the same tokens with the same values.");
  process.exit(1);
}

console.log(`check-theme-tokens: ${automatic.length} dark tokens match in both copies.`);
