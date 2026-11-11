// stops a console window from opening next to the app in release builds on windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    letters_shell_lib::run()
}
