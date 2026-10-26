// Prevents an extra console window on Windows in release; the desktop binary
// only exists for `tauri dev` on a laptop.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    hushflow_mobile_lib::run()
}
