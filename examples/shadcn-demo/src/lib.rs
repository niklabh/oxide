//! Tour of Oxide's shadcn/ui-inspired widget primitives.
//!
//! Each section demonstrates one component family. Run with:
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release -p shadcn-demo
//! ```
//!
//! Then open the resulting `.wasm` from `target/wasm32-unknown-unknown/release/shadcn_demo.wasm`
//! inside the Oxide browser (File → Open) or pass its path on the command line.

use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    log("shadcn-demo loaded");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    // Match the host's dark surface so the page feels native.
    canvas_clear(0x0a, 0x0a, 0x0b, 0xff);
    set_content_size(960, 940);

    // ── Header ──────────────────────────────────────────────────────
    ui_label(24.0, 24.0, "Oxide UI Kit", 28.0);
    ui_label_muted(
        24.0,
        58.0,
        "shadcn/ui-inspired primitives rendered by the host.",
        14.0,
    );
    ui_separator(24.0, 92.0, 912.0);

    // ── Buttons row ─────────────────────────────────────────────────
    ui_label(24.0, 112.0, "Buttons", 16.0);
    ui_button_variant(
        100,
        24.0,
        140.0,
        110.0,
        36.0,
        "Default",
        UiVariant::Default,
        || {},
    );
    ui_button_variant(
        101,
        144.0,
        140.0,
        110.0,
        36.0,
        "Secondary",
        UiVariant::Secondary,
        || {},
    );
    ui_button_variant(
        102,
        264.0,
        140.0,
        110.0,
        36.0,
        "Outline",
        UiVariant::Outline,
        || {},
    );
    ui_button_variant(
        103,
        384.0,
        140.0,
        110.0,
        36.0,
        "Ghost",
        UiVariant::Ghost,
        || {},
    );
    ui_button_variant(
        104,
        504.0,
        140.0,
        110.0,
        36.0,
        "Destructive",
        UiVariant::Destructive,
        || {},
    );

    // ── Badges ──────────────────────────────────────────────────────
    ui_label(24.0, 204.0, "Badges", 16.0);
    ui_badge(24.0, 232.0, "Default", UiVariant::Default);
    ui_badge(108.0, 232.0, "Secondary", UiVariant::Secondary);
    ui_badge(204.0, 232.0, "Outline", UiVariant::Outline);
    ui_badge(288.0, 232.0, "Destructive", UiVariant::Destructive);
    ui_badge(396.0, 232.0, "v0.7.0", UiVariant::Ghost);

    ui_separator(24.0, 276.0, 912.0);

    // ── Form section: inputs + switches ─────────────────────────────
    ui_label(24.0, 296.0, "Form", 16.0);

    ui_label_muted(24.0, 326.0, "Email", 13.0);
    let email = ui_text_input(200, 24.0, 348.0, 360.0, "name@example.com");

    ui_label_muted(24.0, 396.0, "Password", 13.0);
    let _password = ui_text_input(201, 24.0, 418.0, 360.0, "Enter a password");

    ui_label_muted(24.0, 466.0, "Message", 13.0);
    let message = ui_textarea(
        202,
        24.0,
        488.0,
        360.0,
        110.0,
        "Tell us what's on your mind…",
    );

    let remember = ui_checkbox(210, 24.0, 614.0, "Remember me", true);
    let marketing = ui_switch(211, 200.0, 614.0, "Marketing emails", false);

    ui_button_variant(
        220,
        24.0,
        654.0,
        120.0,
        36.0,
        "Sign in",
        UiVariant::Default,
        || {},
    );
    ui_button_variant(
        221,
        156.0,
        654.0,
        100.0,
        36.0,
        "Cancel",
        UiVariant::Ghost,
        || {},
    );

    // ── Live preview card ───────────────────────────────────────────
    ui_card(
        420.0,
        296.0,
        516.0,
        296.0,
        "Live preview",
        "Every field updates this card in real time.",
    );

    let mut row_y = 376.0;
    let preview = if email.is_empty() {
        "(no email yet)".to_string()
    } else {
        format!("📧  {email}")
    };
    ui_label(440.0, row_y, &preview, 14.0);
    row_y += 28.0;

    let lines = message.lines().count();
    let chars = message.chars().count();
    ui_label_muted(
        440.0,
        row_y,
        &format!("Message: {chars} chars · {lines} lines"),
        13.0,
    );
    row_y += 28.0;

    let prefs = format!(
        "Remember: {}   ·   Marketing: {}",
        if remember { "yes" } else { "no" },
        if marketing { "on" } else { "off" },
    );
    ui_label_muted(440.0, row_y, &prefs, 13.0);

    // ── Sliders + progress ─────────────────────────────────────────
    ui_label(24.0, 720.0, "Slider & progress", 16.0);

    let volume = ui_slider(300, 24.0, 752.0, 360.0, 0.0, 100.0, 32.0);
    ui_label_muted(400.0, 756.0, &format!("Volume {volume:.0}%"), 13.0);

    let goal = ui_slider(301, 24.0, 794.0, 360.0, 0.0, 1.0, 0.6);
    ui_label_muted(400.0, 798.0, "Goal progress", 13.0);

    ui_progress(24.0, 832.0, 912.0, goal);

    ui_separator(24.0, 868.0, 912.0);
    ui_label_muted(
        24.0,
        884.0,
        "Tip: arrow keys, shift-arrows, Cmd/Ctrl+A/C/X/V all work inside text fields.",
        12.0,
    );
}
