// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKit's thread arenas made memory depend on timing (Tools D18).
    arcade_link::runtime::cap_malloc_arenas();
    arcade_link::childenv::on_startup();
    arcade_look_lib::run()
}
