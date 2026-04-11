use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let host = oxide_browser::runtime::BrowserHost::new()?;
    let host_state = host.host_state.clone();
    let status = host.status.clone();

    oxide_browser::ui::run_browser(host_state, status)?;
    Ok(())
}
