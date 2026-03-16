use oxide_sdk::proto::{ProtoDecoder, ProtoEncoder};
use oxide_sdk::*;

const API_BASE: &str = "http://localhost:3333";

// ── Colors ───────────────────────────────────────────────────────────────────

const BG: (u8, u8, u8) = (24, 24, 37);
const TITLE_BG: (u8, u8, u8) = (60, 50, 90);
const SECTION_BG: (u8, u8, u8) = (35, 35, 52);
const HEADING: (u8, u8, u8) = (180, 140, 255);
const TEXT: (u8, u8, u8) = (200, 200, 210);
const DIM: (u8, u8, u8) = (120, 120, 140);
const GREEN: (u8, u8, u8) = (120, 220, 140);
const AMBER: (u8, u8, u8) = (255, 200, 100);
const RED: (u8, u8, u8) = (255, 120, 120);
const CYAN: (u8, u8, u8) = (80, 220, 220);

// ── Domain ───────────────────────────────────────────────────────────────────

struct Note {
    id: u32,
    title: String,
    done: bool,
}

struct NoteList {
    notes: Vec<Note>,
    total: u32,
    done_count: u32,
}

fn decode_note_list(data: &[u8]) -> NoteList {
    let mut notes = Vec::new();
    let mut total = 0u32;
    let mut done_count = 0u32;

    let mut dec = ProtoDecoder::new(data);
    while let Some(field) = dec.next() {
        match field.number {
            1 => {
                let mut id = 0u32;
                let mut title = String::new();
                let mut done = false;
                let mut sub = field.as_message();
                while let Some(f) = sub.next() {
                    match f.number {
                        1 => id = f.as_u32(),
                        2 => title = f.as_str().to_string(),
                        3 => done = f.as_bool(),
                        _ => {}
                    }
                }
                notes.push(Note { id, title, done });
            }
            2 => total = field.as_u32(),
            3 => done_count = field.as_u32(),
            _ => {}
        }
    }

    NoteList {
        notes,
        total,
        done_count,
    }
}

fn decode_single_note(data: &[u8]) -> Note {
    let mut id = 0u32;
    let mut title = String::new();
    let mut done = false;

    let mut dec = ProtoDecoder::new(data);
    while let Some(f) = dec.next() {
        match f.number {
            1 => id = f.as_u32(),
            2 => title = f.as_str().to_string(),
            3 => done = f.as_bool(),
            _ => {}
        }
    }
    Note { id, title, done }
}

// ── Operation log ────────────────────────────────────────────────────────────

struct Op {
    method: &'static str,
    path: String,
    status: u32,
    detail: String,
}

// ── Drawing helpers ──────────────────────────────────────────────────────────

fn draw_section_bg(y: f32, h: f32, w: f32) {
    canvas_rect(0.0, y, w, h, SECTION_BG.0, SECTION_BG.1, SECTION_BG.2, 255);
}

fn draw_heading(x: f32, y: f32, label: &str) {
    canvas_text(x, y, 16.0, HEADING.0, HEADING.1, HEADING.2, label);
}

fn draw_text(x: f32, y: f32, c: (u8, u8, u8), msg: &str) {
    canvas_text(x, y, 14.0, c.0, c.1, c.2, msg);
}

fn draw_note_row(x: f32, y: f32, note: &Note, tag: &str) {
    let (marker, color) = if note.done {
        ("[done]", GREEN)
    } else {
        ("[    ]", AMBER)
    };
    let label = if tag.is_empty() {
        format!("{}  #{}  {}", marker, note.id, note.title)
    } else {
        format!("{}  #{}  {}  {}", marker, note.id, note.title, tag)
    };
    draw_text(x, y, color, &label);
}

fn status_color(status: u32) -> (u8, u8, u8) {
    match status {
        200..=299 => GREEN,
        400..=499 => AMBER,
        _ => RED,
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn start_app() {
    log("fullstack-notes: starting");

    let (width, _height) = canvas_dimensions();
    let w = width as f32;

    canvas_clear(BG.0, BG.1, BG.2, 255);

    // ── Title bar ────────────────────────────────────────────────────
    canvas_rect(0.0, 0.0, w, 50.0, TITLE_BG.0, TITLE_BG.1, TITLE_BG.2, 255);
    canvas_text(20.0, 14.0, 22.0, 220, 200, 255, "Oxide Notes");
    canvas_text(
        20.0,
        38.0,
        12.0,
        DIM.0,
        DIM.1,
        DIM.2,
        "Full-stack demo  \u{2022}  WASM frontend  \u{2022}  Rust backend  \u{2022}  Protobuf wire format",
    );

    let mut ops: Vec<Op> = Vec::new();
    let mut total_up: usize = 0;
    let mut total_down: usize = 0;
    let mut final_list: Option<NoteList> = None;
    let mut created_note: Option<Note> = None;

    // ── Step 1: GET initial notes ────────────────────────────────────
    let url = format!("{API_BASE}/api/notes");
    let initial_list = match fetch_get(&url) {
        Ok(resp) => {
            let list = decode_note_list(&resp.body);
            ops.push(Op {
                method: "GET",
                path: "/api/notes".into(),
                status: resp.status,
                detail: format!("{} notes", list.total),
            });
            total_down += resp.body.len();
            list
        }
        Err(code) => {
            render_offline(w, code);
            return;
        }
    };

    // ── Step 2: POST create a note ───────────────────────────────────
    let body = ProtoEncoder::new()
        .string(1, "Explore the Oxide fetch API")
        .finish();
    let sent = body.len();
    if let Ok(resp) = fetch_post(&format!("{API_BASE}/api/notes"), "application/protobuf", &body)
    {
        let note = decode_single_note(&resp.body);
        ops.push(Op {
            method: "POST",
            path: "/api/notes".into(),
            status: resp.status,
            detail: format!("created #{}", note.id),
        });
        total_up += sent;
        total_down += resp.body.len();
        created_note = Some(note);
    }

    // ── Step 3: POST toggle note #2 ─────────────────────────────────
    if let Ok(resp) = fetch_post(
        &format!("{API_BASE}/api/notes/2/toggle"),
        "",
        &[],
    ) {
        let note = decode_single_note(&resp.body);
        ops.push(Op {
            method: "POST",
            path: "/api/notes/2/toggle".into(),
            status: resp.status,
            detail: format!("#{} done={}", note.id, note.done),
        });
        total_down += resp.body.len();
    }

    // ── Step 4: DELETE note #3 ───────────────────────────────────────
    if let Ok(resp) = fetch_delete(&format!("{API_BASE}/api/notes/3")) {
        let note = decode_single_note(&resp.body);
        ops.push(Op {
            method: "DELETE",
            path: "/api/notes/3".into(),
            status: resp.status,
            detail: format!("removed #{}", note.id),
        });
        total_down += resp.body.len();
    }

    // ── Step 5: GET final state ──────────────────────────────────────
    if let Ok(resp) = fetch_get(&format!("{API_BASE}/api/notes")) {
        let list = decode_note_list(&resp.body);
        ops.push(Op {
            method: "GET",
            path: "/api/notes".into(),
            status: resp.status,
            detail: format!("{} notes", list.total),
        });
        total_down += resp.body.len();
        final_list = Some(list);
    }

    // ── Render ───────────────────────────────────────────────────────
    let mut y = 55.0;

    // Connection status
    draw_text(
        20.0,
        y,
        GREEN,
        &format!("\u{25CF}  Connected to {API_BASE}"),
    );
    y += 25.0;

    // ── Initial State ────────────────────────────────────────────────
    draw_section_bg(y - 2.0, 22.0, w);
    draw_heading(20.0, y, "Initial State");
    y += 24.0;

    draw_text(
        30.0,
        y,
        DIM,
        &format!(
            "{} total  \u{2022}  {} done  \u{2022}  {} pending",
            initial_list.total,
            initial_list.done_count,
            initial_list.total - initial_list.done_count
        ),
    );
    y += 20.0;
    for note in &initial_list.notes {
        draw_note_row(30.0, y, note, "");
        y += 20.0;
    }
    y += 8.0;

    // ── API Operations ───────────────────────────────────────────────
    draw_section_bg(y - 2.0, 22.0, w);
    draw_heading(20.0, y, "API Operations (CRUD cycle)");
    y += 24.0;

    for (i, op) in ops.iter().enumerate() {
        let arrow = if i == 0 || i == ops.len() - 1 {
            "\u{2190}"
        } else {
            "\u{2192}"
        };
        let line = format!(
            "{}  {:6} {:<26} {} {}",
            arrow, op.method, op.path, op.status, op.detail
        );
        draw_text(30.0, y, status_color(op.status), &line);
        y += 20.0;
    }
    y += 8.0;

    // ── Final State ──────────────────────────────────────────────────
    draw_section_bg(y - 2.0, 22.0, w);
    draw_heading(20.0, y, "Final State");
    y += 24.0;

    if let Some(ref list) = final_list {
        draw_text(
            30.0,
            y,
            DIM,
            &format!(
                "{} total  \u{2022}  {} done  \u{2022}  {} pending",
                list.total,
                list.done_count,
                list.total - list.done_count
            ),
        );
        y += 20.0;
        let created_id = created_note.as_ref().map(|n| n.id).unwrap_or(0);
        for note in &list.notes {
            let tag = if note.id == created_id {
                "(new)"
            } else if note.id == 2 {
                "(toggled)"
            } else {
                ""
            };
            draw_note_row(30.0, y, note, tag);
            y += 20.0;
        }
    }
    y += 8.0;

    // ── Data exchange stats ──────────────────────────────────────────
    draw_section_bg(y - 2.0, 22.0, w);
    draw_heading(20.0, y, "Data Exchange");
    y += 24.0;

    draw_text(
        30.0,
        y,
        CYAN,
        &format!(
            "\u{2191} Sent      {} bytes protobuf  ({} requests)",
            total_up,
            ops.len()
        ),
    );
    y += 20.0;
    draw_text(
        30.0,
        y,
        CYAN,
        &format!(
            "\u{2193} Received  {} bytes protobuf  ({} responses)",
            total_down,
            ops.len()
        ),
    );
    y += 20.0;
    draw_text(
        30.0,
        y,
        DIM,
        "Format: Protocol Buffers (binary wire format, no .proto files)",
    );
    y += 20.0;
    draw_text(
        30.0,
        y,
        DIM,
        &format!("Round trips: {} (full CRUD cycle)", ops.len()),
    );
    y += 30.0;

    // ── Decorative accents ───────────────────────────────────────────
    canvas_circle(w - 80.0, 120.0, 45.0, 60, 50, 90, 100);
    canvas_circle(w - 50.0, 180.0, 25.0, 100, 180, 255, 80);
    canvas_circle(w - 110.0, 170.0, 18.0, 180, 140, 255, 70);

    // ── Separator at bottom ──────────────────────────────────────────
    canvas_line(20.0, y, w - 20.0, y, DIM.0, DIM.1, DIM.2, 1.0);
    y += 12.0;
    draw_text(
        20.0,
        y,
        DIM,
        "fullstack-notes  \u{2022}  backend: cargo run -p fullstack-notes-backend  \u{2022}  frontend: this WASM module",
    );

    notify("Oxide Notes", "Full-stack demo completed successfully!");
    log("fullstack-notes: done");
}

/// Renders an error screen when the backend is unreachable.
fn render_offline(w: f32, err_code: i64) {
    canvas_clear(BG.0, BG.1, BG.2, 255);
    canvas_rect(0.0, 0.0, w, 50.0, TITLE_BG.0, TITLE_BG.1, TITLE_BG.2, 255);
    canvas_text(20.0, 14.0, 22.0, 220, 200, 255, "Oxide Notes");
    canvas_text(20.0, 38.0, 12.0, DIM.0, DIM.1, DIM.2, "Full-stack demo");

    draw_text(
        20.0,
        75.0,
        RED,
        &format!(
            "\u{25CF}  Cannot reach backend at {API_BASE}  (error {})",
            err_code
        ),
    );
    draw_text(20.0, 110.0, TEXT, "Start the backend server first:");
    draw_text(
        40.0,
        135.0,
        AMBER,
        "cargo run -p fullstack-notes-backend",
    );
    draw_text(
        20.0,
        170.0,
        TEXT,
        "Then reload this WASM module in the Oxide browser.",
    );

    error(&format!(
        "Backend unreachable at {API_BASE} (error {err_code}). Start it with: cargo run -p fullstack-notes-backend"
    ));
}
