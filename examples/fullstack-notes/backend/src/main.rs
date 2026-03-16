mod proto;

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Router,
};
use proto::{ProtoDecoder, ProtoEncoder};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Domain ───────────────────────────────────────────────────────────────────

struct Note {
    id: u32,
    title: String,
    done: bool,
    created_at: u64,
}

/// Proto field layout (shared with WASM frontend):
///   Note        { 1: uint32 id, 2: string title, 3: bool done, 4: uint64 created_at }
///   NoteList    { 1: repeated Note (sub-msg), 2: uint32 total, 3: uint32 done_count }
///   CreateReq   { 1: string title }
impl Note {
    fn to_proto(&self) -> ProtoEncoder {
        ProtoEncoder::new()
            .uint32(1, self.id)
            .string(2, &self.title)
            .bool(3, self.done)
            .uint64(4, self.created_at)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn proto_response(status: StatusCode, body: Vec<u8>) -> impl IntoResponse {
    (
        status,
        [(header::CONTENT_TYPE, "application/protobuf")],
        body,
    )
}

// ── Shared State ─────────────────────────────────────────────────────────────

struct AppState {
    notes: Mutex<Vec<Note>>,
    next_id: Mutex<u32>,
}

type SharedState = Arc<AppState>;

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn list_notes(State(state): State<SharedState>) -> impl IntoResponse {
    let notes = state.notes.lock().unwrap();
    let total = notes.len() as u32;
    let done_count = notes.iter().filter(|n| n.done).count() as u32;

    let mut enc = ProtoEncoder::new();
    for note in notes.iter() {
        enc = enc.message(1, &note.to_proto());
    }
    enc = enc.uint32(2, total).uint32(3, done_count);

    proto_response(StatusCode::OK, enc.finish())
}

async fn create_note(
    State(state): State<SharedState>,
    body: Bytes,
) -> impl IntoResponse {
    let mut title = String::new();
    let mut decoder = ProtoDecoder::new(&body);
    while let Some(field) = decoder.next() {
        if field.number == 1 {
            title = field.as_str().to_string();
        }
    }

    if title.is_empty() {
        return proto_response(
            StatusCode::BAD_REQUEST,
            ProtoEncoder::new().string(1, "title is required").finish(),
        );
    }

    let mut next_id = state.next_id.lock().unwrap();
    let id = *next_id;
    *next_id += 1;
    drop(next_id);

    let note = Note {
        id,
        title,
        done: false,
        created_at: now_ms(),
    };
    let resp = note.to_proto().finish();
    state.notes.lock().unwrap().push(note);

    proto_response(StatusCode::CREATED, resp)
}

async fn toggle_note(
    State(state): State<SharedState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let mut notes = state.notes.lock().unwrap();
    if let Some(note) = notes.iter_mut().find(|n| n.id == id) {
        note.done = !note.done;
        let resp = note.to_proto().finish();
        proto_response(StatusCode::OK, resp)
    } else {
        proto_response(StatusCode::NOT_FOUND, Vec::new())
    }
}

async fn delete_note(
    State(state): State<SharedState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let mut notes = state.notes.lock().unwrap();
    if let Some(pos) = notes.iter().position(|n| n.id == id) {
        let removed = notes.remove(pos);
        let resp = removed.to_proto().finish();
        proto_response(StatusCode::OK, resp)
    } else {
        proto_response(StatusCode::NOT_FOUND, Vec::new())
    }
}

// ── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let state = Arc::new(AppState {
        notes: Mutex::new(vec![
            Note {
                id: 1,
                title: "Learn the Oxide browser".into(),
                done: true,
                created_at: 1710000000000,
            },
            Note {
                id: 2,
                title: "Build a WASM guest app".into(),
                done: false,
                created_at: 1710000060000,
            },
            Note {
                id: 3,
                title: "Deploy to production".into(),
                done: false,
                created_at: 1710000120000,
            },
        ]),
        next_id: Mutex::new(4),
    });

    let app = Router::new()
        .route("/api/notes", get(list_notes).post(create_note))
        .route("/api/notes/{id}/toggle", post(toggle_note))
        .route("/api/notes/{id}", delete(delete_note))
        .with_state(state);

    let addr = "0.0.0.0:3333";
    println!("notes-server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
