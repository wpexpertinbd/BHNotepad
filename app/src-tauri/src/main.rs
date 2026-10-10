// Prevents a console window appearing behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
	bhnotepad_lib::run()
}
