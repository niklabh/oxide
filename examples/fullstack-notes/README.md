# Fullstack Notes

A full-stack example for the Oxide browser: a **Rust HTTP backend** exchanges data
with a **WebAssembly frontend** using Protocol Buffers over HTTP.

## Architecture

```
┌─────────────────────────┐         HTTP + Protobuf         ┌──────────────────────┐
│   WASM Frontend         │ ◄──────────────────────────────► │   Rust Backend       │
│   (oxide-sdk guest)     │   GET/POST/DELETE /api/notes     │   (axum server)      │
│   renders on canvas     │                                  │   in-memory store    │
└─────────────────────────┘                                  └──────────────────────┘
```

The frontend performs a full **CRUD cycle** on startup:

1. **GET** `/api/notes` — fetch initial notes
2. **POST** `/api/notes` — create a new note (protobuf body)
3. **POST** `/api/notes/2/toggle` — toggle a note's done status
4. **DELETE** `/api/notes/3` — delete a note
5. **GET** `/api/notes` — fetch final state

All request and response bodies use the Protocol Buffers binary wire format
(the same `ProtoEncoder`/`ProtoDecoder` from `oxide-sdk`).

## Quick Start

### 1. Start the backend

```sh
cargo run -p fullstack-notes-backend
# => notes-server listening on http://0.0.0.0:3333
```

### 2. Build the frontend WASM module

```sh
cargo build -p fullstack-notes-frontend --target wasm32-unknown-unknown --release
```

The compiled module is at:

```
target/wasm32-unknown-unknown/release/fullstack_notes_frontend.wasm
```

### 3. Load in the Oxide browser

```sh
cargo run -p oxide
```

Open the `.wasm` file via **File → Open** (or drag-and-drop).

## Proto Schema

Both sides agree on the same field numbers (no `.proto` files needed):

```
Note {
    1: uint32  id
    2: string  title
    3: bool    done
    4: uint64  created_at
}

NoteList {
    1: repeated Note  (sub-messages)
    2: uint32         total
    3: uint32         done_count
}

CreateNoteRequest {
    1: string  title
}
```
