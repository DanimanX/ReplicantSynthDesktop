//! ReplicantSynth Desktop — the cross-platform 64-pad clip launcher.

mod app;
mod audio;
mod decode;
mod midi;
mod plugins;
mod ui;

fn main() -> eframe::Result<()> {
    println!(
        "ReplicantSynth Desktop {} — 64-pad clip launcher (303 · Wave · 909 · ReplicantFX)",
        env!("CARGO_PKG_VERSION")
    );
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([1120.0, 720.0])
            .with_title("ReplicantSynth Desktop"),
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "ReplicantSynth Desktop",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
