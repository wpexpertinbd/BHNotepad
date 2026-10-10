# Disclaimer

BHNotepad is provided free of charge, **as is**, with no warranty of any kind. See
[LICENSE](LICENSE) for the full legal text.

## Your notes are your responsibility

BHNotepad is designed so that unsaved tabs survive quitting, crashing and power loss, and that
behaviour is covered by automated tests — including one that kills the process outright and
checks everything comes back. It is still not a backup system.

- **Keep anything you cannot afford to lose in a real file**, saved somewhere you back up.
- Scratch notes live in one folder in your user profile. If that folder is deleted, if the disk
  fails, or if a disk-cleaning tool removes it, those notes are gone.
- Writes are atomic and flushed to disk, but no application can defend against failing hardware
  or a filesystem that lies about having written.

## Not Notepad++

BHNotepad is an independent program. It is **not** Notepad++, not a fork of Notepad++, not
endorsed by or affiliated with Notepad++ or its author Don Ho, and contains none of its source
code. It does not load Notepad++ plugins and cannot be made to.

"Notepad++" and its logo belong to their owner and are referred to here only to describe what
this program is like and where its ideas came from.

## Platform coverage

The macOS builds are used daily by the author. The **Windows and Linux builds are produced by CI
and have not been run on real hardware** by the author. They are offered in good faith; please
report anything broken.

The macOS builds are ad-hoc signed and **not notarised by Apple**; the Windows builds are **not
code-signed**. Both operating systems will warn you on first run. Only download releases from
this repository.
