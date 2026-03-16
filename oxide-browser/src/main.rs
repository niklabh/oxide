mod capabilities;
mod engine;
mod navigation;
mod runtime;
mod ui;
mod url;

use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let host = runtime::BrowserHost::new()?;
    let host_state = host.host_state.clone();
    let status = host.status.clone();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Oxide Browser")
            .with_inner_size([1024.0, 720.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Oxide Browser",
        native_options,
        Box::new(move |_cc| Ok(Box::new(ui::OxideApp::new(host_state, status)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}
