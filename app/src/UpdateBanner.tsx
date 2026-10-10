// The strip that appears when a newer release exists.
//
// A banner rather than a modal: an editor that interrupts you to talk about
// itself is worse than one that waits. It can be dismissed, and dismissing it
// is remembered for that version so the same release never nags twice.

import { useState } from "react";

import * as ipc from "./ipc";

interface Props {
  release: ipc.UpdateRelease;
  onDismiss: () => void;
  onProblem: (message: string) => void;
}

function describeSize(bytes: number): string {
  if (bytes <= 0) return "";
  return ` (${(bytes / 1048576).toFixed(1)} MB)`;
}

export function UpdateBanner({ release, onDismiss, onProblem }: Props) {
  const [busy, setBusy] = useState(false);
  /** Where the installer was downloaded to, once it has been. */
  const [saved, setSaved] = useState<string | null>(null);

  return (
    <div className="updatebar" role="status">
      <span className="updatebar-text">
        {saved
          ? `BHNotepad ${release.version} is ready to install.`
          : `BHNotepad ${release.version} is available.`}
      </span>

      {!saved && release.asset_url && (
        <button
          type="button"
          className="updatebar-primary"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            void ipc
              .downloadUpdate(release)
              .then((path) => setSaved(path))
              .catch((error) =>
                onProblem(error instanceof Error ? error.message : String(error)),
              )
              .finally(() => setBusy(false));
          }}
        >
          {busy ? "Downloading…" : `Download${describeSize(release.asset_size)}`}
        </button>
      )}

      {saved && (
        <button
          type="button"
          className="updatebar-primary"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            // This quits BHNotepad a moment later so the installer can
            // replace it. Every tab is already on disk, so there is nothing
            // to save first and nothing to ask about.
            void ipc.installUpdate(saved).catch((error) => {
              setBusy(false);
              onProblem(error instanceof Error ? error.message : String(error));
            });
          }}
        >
          {busy ? "Starting the installer…" : "Install and Quit"}
        </button>
      )}

      <button
        type="button"
        onClick={() => {
          void ipc.openReleasePage(release.page_url).catch((error) =>
            onProblem(error instanceof Error ? error.message : String(error)),
          );
        }}
      >
        What's new
      </button>

      <button type="button" className="updatebar-close" aria-label="Dismiss" onClick={onDismiss}>
        ×
      </button>
    </div>
  );
}
