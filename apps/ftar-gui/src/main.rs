// Release builds use the GUI subsystem so Windows does not open a console
// window behind the app; debug builds keep it for log output.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    ftar_gui::run(std::env::args().nth(1).map(Into::into))
}
