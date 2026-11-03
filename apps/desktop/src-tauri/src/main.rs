// Kein Konsolenfenster im Release-Build – die Anwendung ist ein Fenster.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    wartungscockpit_desktop::run()
}
