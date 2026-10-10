//! The Tauri shell for BHNotepad.
//!
//! Deliberately thin: it owns the window, the config directory and the IPC
//! commands, and nothing else. All behaviour lives in `bhn-core`, which has no
//! idea Tauri exists — that is what makes the Windows and Linux builds the same
//! application rather than three lookalikes.
//!
//! **The quit path must stay empty.** No confirmation, no "unsaved changes"
//! check, nothing that can veto a close. Text is written through to disk as it
//! is typed, so quitting is never a decision point. See CLAUDE.md.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bhn_core::{Session, Workspace};
use serde::Serialize;
use tauri::{Emitter, Manager, State};

/// Emitted after files are opened from outside the window — a double-click in
/// Finder or Explorer, or a second launch handing its arguments over. The UI
/// reloads the session when it hears this.
const SESSION_CHANGED: &str = "bhnotepad://session-changed";

/// Emitted when a file handed to the app from outside could not be opened.
/// Without this, double-clicking a file that is refused does nothing at all
/// and says nothing — the window does not even come forward.
const OPEN_FAILED: &str = "bhnotepad://open-failed";

/// Files that arrived before the workspace existed. See `open_files`.
static PENDING_FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Where the session store lives, per platform. This is the only place in the
/// project that asks the OS about paths; the core takes it as an argument.
///
/// - macOS   `~/Library/Application Support/BHNotepad`
/// - Windows `%APPDATA%\BHNotepad`
/// - Linux   `${XDG_CONFIG_HOME:-~/.config}/BHNotepad`
fn config_root() -> PathBuf {
	dirs::config_dir()
		.unwrap_or_else(|| PathBuf::from("."))
		.join("BHNotepad")
}

struct AppState {
	workspace: Mutex<Workspace>,
	/// Why the previous session could not be restored in full, if it could
	/// not. Taken by the first `session_state` call so the window can say so
	/// once — this app's whole promise is that tabs come back, so a failure to
	/// restore has to be visible, not just logged to a console nobody reads.
	restore_note: Mutex<Option<String>>,
}

/// Commands report failures as plain strings; the UI only ever shows them in a
/// status line, and a typed error hierarchy across the IPC boundary would buy
/// nothing.
type CommandResult<T> = Result<T, String>;

fn to_message(error: impl std::fmt::Display) -> String {
	error.to_string()
}

#[derive(Serialize)]
struct Restored {
	session: Session,
	/// Set when the previous session could not be read in full, so the UI can
	/// say so once instead of failing silently.
	note: Option<String>,
}

#[derive(Serialize)]
struct Opened {
	session: Session,
	id: String,
	text: String,
}

/// The session as it stands, for the UI's initial render.
///
/// Carries the restore note on the first call only, so a warning is shown once
/// rather than on every refresh.
#[tauri::command]
fn session_state(state: State<'_, AppState>) -> CommandResult<Restored> {
	let workspace = state.workspace.lock().map_err(to_message)?;
	let note = state
		.restore_note
		.lock()
		.map_err(to_message)?
		.take();
	Ok(Restored { session: workspace.session().clone(), note })
}

#[tauri::command]
fn buffer_text(state: State<'_, AppState>, id: String) -> CommandResult<String> {
	let workspace = state.workspace.lock().map_err(to_message)?;
	workspace.text(&id).map_err(to_message)
}

#[tauri::command]
fn new_tab(state: State<'_, AppState>) -> CommandResult<Opened> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	let id = workspace.new_scratch().map_err(to_message)?;
	Ok(Opened { session: workspace.session().clone(), id, text: String::new() })
}

#[tauri::command]
fn open_path(state: State<'_, AppState>, path: String) -> CommandResult<Opened> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	let (id, text) = workspace.open_path(&path).map_err(to_message)?;
	Ok(Opened { session: workspace.session().clone(), id, text })
}

/// The hot path: called as the user types, debounced by the UI.
///
/// Returns nothing so the UI never waits on it, and takes the text rather than
/// a diff because the whole document is what has to be on disk.
#[tauri::command]
fn update_text(state: State<'_, AppState>, id: String, text: String) -> CommandResult<()> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.update_text(&id, &text).map_err(to_message)
}

#[tauri::command]
fn update_position(
	state: State<'_, AppState>,
	id: String,
	caret: u64,
	scroll_top: u64,
) -> CommandResult<()> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.update_position(&id, caret, scroll_top);
	Ok(())
}

#[tauri::command]
fn save(state: State<'_, AppState>, id: String, path: Option<String>) -> CommandResult<Session> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.save(&id, path.as_deref()).map_err(to_message)?;
	Ok(workspace.session().clone())
}

/// Closes a tab. There is no confirmation here and there must never be one.
#[tauri::command]
fn close_tab(state: State<'_, AppState>, id: String) -> CommandResult<Session> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.close(&id).map_err(to_message)?;
	Ok(workspace.session().clone())
}

/// Closes every tab. The shell asks the user once before calling this; the
/// core does not ask, and neither does anything else in the close path.
#[tauri::command]
fn close_all(state: State<'_, AppState>) -> CommandResult<Session> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.close_all().map_err(to_message)?;
	Ok(workspace.session().clone())
}

/// The tabs that have never been saved to a file, in tab order, so Save All
/// can ask where each should go.
#[tauri::command]
fn unsaved_tabs(state: State<'_, AppState>) -> CommandResult<Vec<bhn_core::Buffer>> {
	let workspace = state.workspace.lock().map_err(to_message)?;
	Ok(workspace.unsaved_tabs())
}

#[tauri::command]
fn set_active(state: State<'_, AppState>, index: usize) -> CommandResult<()> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.set_active(index);
	Ok(())
}

#[tauri::command]
fn reorder(state: State<'_, AppState>, from: usize, to: usize) -> CommandResult<Session> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.reorder(from, to);
	workspace.persist().map_err(to_message)?;
	Ok(workspace.session().clone())
}

#[tauri::command]
fn set_word_wrap(state: State<'_, AppState>, id: String, wrap: bool) -> CommandResult<()> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.set_word_wrap(&id, wrap);
	Ok(())
}

#[tauri::command]
fn set_language(state: State<'_, AppState>, id: String, language: String) -> CommandResult<()> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.set_language(&id, &language);
	Ok(())
}

/// Writes `session.json`.
///
/// Caret moves and wrap toggles are kept in memory and flushed by this, rather
/// than rewriting the file on every cursor movement. Text snapshots do not wait
/// for it — those are already on disk.
#[tauri::command]
fn flush(state: State<'_, AppState>) -> CommandResult<()> {
	let workspace = state.workspace.lock().map_err(to_message)?;
	workspace.persist().map_err(to_message)
}

/// Reopens the most recently closed tab. Returns its id, or null if there is
/// nothing to bring back.
#[tauri::command]
fn restore_closed(state: State<'_, AppState>) -> CommandResult<Option<Opened>> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	let Some(id) = workspace.restore_last_closed().map_err(to_message)? else {
		return Ok(None);
	};
	let text = workspace.text(&id).map_err(to_message)?;
	Ok(Some(Opened { session: workspace.session().clone(), id, text }))
}

/// Discards the closed-tab history, text included.
#[tauri::command]
fn forget_closed(state: State<'_, AppState>) -> CommandResult<Session> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.forget_closed_history().map_err(to_message)?;
	Ok(workspace.session().clone())
}

/// Throws away unsaved edits and re-reads the file from disk.
#[tauri::command]
fn reload(state: State<'_, AppState>, id: String) -> CommandResult<Opened> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	let text = workspace.reload(&id).map_err(to_message)?;
	Ok(Opened { session: workspace.session().clone(), id, text })
}

#[tauri::command]
fn rename(state: State<'_, AppState>, id: String, path: String) -> CommandResult<Session> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.rename(&id, &path).map_err(to_message)?;
	Ok(workspace.session().clone())
}

#[tauri::command]
fn save_copy(state: State<'_, AppState>, id: String, path: String) -> CommandResult<()> {
	let workspace = state.workspace.lock().map_err(to_message)?;
	workspace.save_copy(&id, &path).map_err(to_message)
}

#[tauri::command]
fn close_others(state: State<'_, AppState>, id: String) -> CommandResult<Session> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.close_others(&id).map_err(to_message)?;
	Ok(workspace.session().clone())
}

#[tauri::command]
fn close_to_left(state: State<'_, AppState>, id: String) -> CommandResult<Session> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.close_to_left(&id).map_err(to_message)?;
	Ok(workspace.session().clone())
}

#[tauri::command]
fn close_to_right(state: State<'_, AppState>, id: String) -> CommandResult<Session> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace.close_to_right(&id).map_err(to_message)?;
	Ok(workspace.session().clone())
}

/// Shows a file in Finder, Explorer or the desktop file manager.
#[tauri::command]
fn reveal(path: String) -> CommandResult<()> {
	tauri_plugin_opener::reveal_item_in_dir(PathBuf::from(path)).map_err(to_message)
}

/// Moves the file a tab is backed by to the Trash, and closes the tab.
///
/// The Trash rather than an unlink: a text editor should not be able to
/// destroy a file beyond recovery with one menu click.
#[tauri::command]
fn move_to_trash(state: State<'_, AppState>, id: String) -> CommandResult<Session> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	let path = workspace
		.session()
		.buffer(&id)
		.and_then(|b| b.path.clone())
		.ok_or_else(|| "That tab has never been saved, so there is no file to move".to_string())?;
	trash::delete(&path).map_err(|error| format!("could not move {path} to the Trash: {error}"))?;
	workspace.close(&id).map_err(to_message)?;
	Ok(workspace.session().clone())
}

// ── Update checking ──────────────────────────────────────────────────────

/// Looks for a newer release.
///
/// `manual` is the difference between the user asking and the app asking on
/// launch. A manual check always runs and always reports what happened; an
/// automatic one respects the user's preference and the 24-hour throttle, and
/// stays quiet unless there is genuinely something new.
///
/// ⚠️ A failed check must never be reported as "up to date" — see the rules in
/// `bhn_core::update`.
#[tauri::command]
fn check_update(
	state: State<'_, AppState>,
	manual: bool,
) -> CommandResult<Option<bhn_core::update::CheckResult>> {
	{
		let workspace = state.workspace.lock().map_err(to_message)?;
		let settings = workspace.settings();
		if !manual {
			if !settings.auto_update_check {
				return Ok(None);
			}
			if !bhn_core::update::check_due(settings.last_update_check) {
				return Ok(None);
			}
		}
	}

	// Stamp the time BEFORE the request, so a failure backs off too rather
	// than retrying on every launch.
	if !manual {
		let mut workspace = state.workspace.lock().map_err(to_message)?;
		let now = bhn_core::update::now_secs();
		workspace
			.update_settings(|s| s.last_update_check = now)
			.map_err(to_message)?;
	}

	// The lock is deliberately NOT held across the network call: a 15-second
	// timeout with the workspace locked would freeze every keystroke.
	Ok(Some(bhn_core::update::check()))
}

#[tauri::command]
fn set_auto_update(state: State<'_, AppState>, enabled: bool) -> CommandResult<()> {
	let mut workspace = state.workspace.lock().map_err(to_message)?;
	workspace
		.update_settings(|s| s.auto_update_check = enabled)
		.map_err(to_message)
}

/// Downloads a release's installer and returns where it landed.
///
/// Into a private scratch folder rather than Downloads: this is a file the
/// user will be asked to approve, so nothing must be able to prepare the path
/// in advance. `install_update` then runs exactly this file and no other.
#[tauri::command]
fn download_update(release: bhn_core::update::Release) -> CommandResult<String> {
	let saved = bhn_core::update::download(&release)?;
	Ok(saved.to_string_lossy().into_owned())
}

/// Runs a downloaded installer and quits so it can replace this app.
///
/// Mirrors BHUninstaller's `install_update`, including both of its guards: the
/// path must be *exactly* the file this process downloaded — not merely
/// something sitting in the folder it downloaded into — and it must look like
/// an installer rather than an arbitrary program.
#[tauri::command]
fn install_update(app: tauri::AppHandle, path: String) -> CommandResult<()> {
	let path = PathBuf::from(&path);
	if bhn_core::update::last_download().as_deref() != Some(path.as_path()) {
		return Err("that file was not downloaded by this app — refusing to run it".into());
	}
	let ext = path
		.extension()
		.and_then(|e| e.to_str())
		.map(|e| e.to_lowercase())
		.unwrap_or_default();
	if !matches!(ext.as_str(), "dmg" | "pkg" | "msi" | "exe" | "deb" | "rpm") {
		return Err(format!("{ext} is not an installer — refusing to run it"));
	}
	if !path.is_file() {
		return Err("the download is no longer there".into());
	}

	installer_command(&path, &ext)
		.spawn()
		.map_err(|error| format!("could not start the installer: {error}"))?;

	// Give it a moment to appear before this window disappears, so the user is
	// not left looking at nothing.
	let handle = app.clone();
	std::thread::spawn(move || {
		std::thread::sleep(std::time::Duration::from_millis(1500));
		handle.exit(0);
	});
	Ok(())
}

fn installer_command(path: &std::path::Path, ext: &str) -> std::process::Command {
	#[cfg(target_os = "windows")]
	{
		// An .msi is data, not a program: it needs msiexec to run it.
		if ext == "msi" {
			let mut command = std::process::Command::new("msiexec");
			command.arg("/i").arg(path);
			return command;
		}
		std::process::Command::new(path)
	}
	#[cfg(target_os = "macos")]
	{
		let _ = ext;
		let mut command = std::process::Command::new("/usr/bin/open");
		command.arg(path);
		command
	}
	#[cfg(target_os = "linux")]
	{
		let _ = ext;
		let mut command = std::process::Command::new("xdg-open");
		command.arg(path);
		command
	}
}

/// Opens a release page in the user's browser.
#[tauri::command]
fn open_release_page(url: String) -> CommandResult<()> {
	// Only github.com, so a tampered API answer cannot send the user
	// somewhere else entirely.
	if !url.starts_with("https://github.com/") {
		return Err("that link is not on github.com".into());
	}
	tauri_plugin_opener::open_url(url, None::<&str>).map_err(to_message)
}

/// The size limit and where to change it, for the status bar's message.
#[derive(Serialize)]
struct Limits {
	max_open_bytes: u64,
	settings_path: String,
	auto_update_check: bool,
	version: String,
}

#[tauri::command]
fn limits(state: State<'_, AppState>) -> CommandResult<Limits> {
	let workspace = state.workspace.lock().map_err(to_message)?;
	Ok(Limits {
		max_open_bytes: workspace.settings().max_open_bytes,
		auto_update_check: workspace.settings().auto_update_check,
		version: env!("CARGO_PKG_VERSION").to_string(),
		settings_path: workspace
			.store()
			.root()
			.join("settings.json")
			.to_string_lossy()
			.into_owned(),
	})
}

/// The language id for a name or path, so Save As can relabel a tab without
/// the UI carrying a second copy of the extension table.
#[tauri::command]
fn detect_language(path: String) -> String {
	bhn_core::detect_language(&path).to_string()
}

/// Opens files handed to the app from outside the window and tells the UI.
///
/// Used by three paths that all mean the same thing: command-line arguments at
/// launch, a second launch whose arguments are forwarded here by the
/// single-instance plugin, and macOS's "Opened" event when a document is
/// dropped on the icon or double-clicked in Finder.
///
/// A file that cannot be read is reported and skipped rather than aborting the
/// rest — opening four files and failing on one should still open three.
fn open_files(app: &tauri::AppHandle, paths: Vec<PathBuf>) {
	let Some(state) = app.try_state::<AppState>() else {
		// macOS can deliver a launch-time document before `setup` has managed
		// the state. Dropping it here is how a cold-start double-click
		// silently opened nothing, so hold it until the workspace exists and
		// let `setup` drain the queue.
		if let Ok(mut pending) = PENDING_FILES.lock() {
			pending.extend(paths);
		}
		return;
	};
	let Ok(mut workspace) = state.workspace.lock() else {
		return;
	};

	let mut opened = 0;
	let mut failures: Vec<String> = Vec::new();
	for path in paths {
		match workspace.open_path(&path) {
			Ok(_) => opened += 1,
			Err(error) => {
				eprintln!("BHNotepad: could not open {}: {error}", path.display());
				failures.push(error.to_string());
			}
		}
	}
	drop(workspace);

	if opened > 0 {
		let _ = app.emit(SESSION_CHANGED, ());
	}
	if let Some(message) = failures.first() {
		let _ = app.emit(OPEN_FAILED, message.clone());
	}

	// Come forward for a failure as much as for a success: the user asked for
	// a file, and silence looks exactly like the app being broken.
	if opened > 0 || !failures.is_empty() {
		if let Some(window) = app.get_webview_window("main") {
			let _ = window.show();
			let _ = window.set_focus();
		}
	}
}

/// Picks the file paths out of a process argument list.
///
/// Skips the program name and anything that looks like a flag, so a stray
/// `--flag` from a launcher is not treated as a file name.
///
/// `cwd` is the directory the arguments were written relative to. A second
/// launch forwards its arguments to the running app, whose working directory
/// is somewhere else entirely — so resolving a relative `notes.txt` against
/// our own cwd would open, and later overwrite, a different file of the same
/// name.
fn paths_from_args<I: IntoIterator<Item = String>>(args: I, cwd: &Path) -> Vec<PathBuf> {
	args.into_iter()
		.skip(1)
		.filter(|arg| !arg.starts_with('-'))
		.map(|arg| {
			let path = PathBuf::from(arg);
			if path.is_absolute() {
				path
			} else {
				cwd.join(path)
			}
		})
		.filter(|path| path.is_file())
		.collect()
}

/// Best-effort flush at teardown.
///
/// A belt-and-braces measure only: everything it writes is positional. If the
/// process is killed before this runs, the text is still on disk and the tabs
/// still come back — just possibly with the caret where it was a moment
/// earlier. Never make this a prerequisite for a clean exit.
fn flush_quietly(app: &tauri::AppHandle) {
	if let Some(state) = app.try_state::<AppState>() {
		if let Ok(workspace) = state.workspace.lock() {
			if let Err(error) = workspace.persist() {
				eprintln!("BHNotepad: could not write session.json: {error}");
			}
		}
	}
}

pub fn run() {
	let builder = tauri::Builder::default();

	// A second process sharing the same store can lose tabs: it persists a
	// session that knows nothing about the first one's, and those notes are
	// then unreferenced. Windows and Linux start a new process for every
	// double-clicked file; macOS normally keeps one instance, but `open -n`
	// starts a rival anyway. So a second launch always hands its files to the
	// running app and exits, on every desktop platform.
	#[cfg(desktop)]
	let builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
		open_files(app, paths_from_args(argv, Path::new(&cwd)));
	}));

	builder
		.plugin(tauri_plugin_dialog::init())
		.plugin(tauri_plugin_opener::init())
		.setup(|app| {
			let root = config_root();
			let (workspace, note) = Workspace::open(&root)
				.map_err(|error| format!("could not open {}: {error}", root.display()))?;
			if let Some(note) = &note {
				eprintln!("BHNotepad: {note}");
			}
			app.manage(AppState {
				workspace: Mutex::new(workspace),
				restore_note: Mutex::new(note),
			});

			// Files named on the command line (how Windows and Linux deliver a
			// double-clicked document to a cold start), plus anything macOS
			// handed over before the workspace existed.
			let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
			let mut paths = paths_from_args(std::env::args(), &cwd);
			if let Ok(mut pending) = PENDING_FILES.lock() {
				paths.append(&mut pending);
			}
			if !paths.is_empty() {
				open_files(app.handle(), paths);
			}
			Ok(())
		})
		.on_window_event(|window, event| {
			// Save positions on the way out and then get out of the way. Note
			// what is NOT here: no `api.prevent_close()`, no dialog, no check
			// for unsaved work. Closing the window is always allowed.
			if let tauri::WindowEvent::CloseRequested { .. } = event {
				flush_quietly(window.app_handle());
			}
		})
		.invoke_handler(tauri::generate_handler![
			session_state,
			buffer_text,
			new_tab,
			open_path,
			update_text,
			update_position,
			save,
			close_tab,
			close_all,
			unsaved_tabs,
			set_active,
			reorder,
			set_word_wrap,
			set_language,
			flush,
			detect_language,
			limits,
			restore_closed,
			forget_closed,
			reload,
			rename,
			save_copy,
			close_others,
			close_to_left,
			close_to_right,
			reveal,
			move_to_trash,
			check_update,
			set_auto_update,
			download_update,
			install_update,
			open_release_page,
		])
		.build(tauri::generate_context!())
		.expect("BHNotepad failed to start")
		.run(|app, event| {
			match event {
				tauri::RunEvent::ExitRequested { .. } => flush_quietly(app),
				// macOS delivers a double-clicked document here rather than on
				// the command line, both at launch and while already running.
				#[cfg(target_os = "macos")]
				tauri::RunEvent::Opened { urls } => {
					let paths: Vec<PathBuf> = urls
						.iter()
						.filter_map(|url| url.to_file_path().ok())
						.collect();
					if !paths.is_empty() {
						open_files(app, paths);
					}
				}
				_ => {}
			}
		});
}
