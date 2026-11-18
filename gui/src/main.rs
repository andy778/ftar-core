fn main() -> eframe::Result {
    ftar_gui::run(std::env::args().nth(1).map(Into::into))
}
