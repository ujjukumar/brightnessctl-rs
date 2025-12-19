use eframe::egui;
mod win;

fn main() -> Result<(), eframe::Error> {
    win::init_com();
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Brightness Control",
        options,
        Box::new(|_cc| Box::new(App)),
    )
}

struct App;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Brightness");
        });
    }
}
