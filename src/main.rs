use eframe::egui;
mod win;
mod monitors;
mod brightness;

fn main() -> Result<(), eframe::Error> {
    win::init_com();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 250.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "Brightness Control",
        options,
        Box::new(|_cc| {
            let monitors = monitors::enumerate_monitors();
            let mut values = Vec::new();

            for m in &monitors {
                values.push(brightness::get_brightness(m).unwrap_or(50));
            }

            Box::new(App { monitors, values })
        }),
    )
}

struct App {
    monitors: Vec<monitors::Monitor>,
    values: Vec<u32>,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Monitor Brightness");
            });
            ui.add_space(10.0);

            if self.monitors.is_empty() {
                ui.label("No supported monitors found.");
            } else {
                for (i, monitor) in self.monitors.iter().enumerate() {
                    ui.group(|ui| {
                        ui.label(format!("Display {}: {}", i + 1, monitor.name));
                        ui.add_space(5.0);
                        
                        let slider = egui::Slider::new(&mut self.values[i], 0..=100)
                            .text("%")
                            .clamp_to_range(true);

                        if ui.add(slider).drag_stopped() {
                             brightness::set_brightness(monitor, self.values[i]);
                        }
                    });
                    ui.add_space(10.0);
                }
            }
        });
    }
}