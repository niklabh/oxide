//! # Drawing Pad — Oxide Browser Example
//!
//! An interactive drawing application that runs as a WASM guest inside the Oxide
//! browser. Supports freehand drawing with automatic circle recognition, plus
//! line, rectangle, and circle tools, with a bottom dock for color, brush size,
//! and tool selection.
//!
//! ## Module layout
//!
//! - [`geometry`] — core types, the color palette, and dock layout constants.
//! - [`smoothing`] — multi-stage stroke-smoothing and circle-recognition pipeline.
//! - [`shapes`] — drawing tools (`DrawTool`) and committed geometry (`Geom`, `Shape`).
//! - [`session`] — the drawing-session state machine (idle → drawing → committed shape).
//! - [`render`] — low-level canvas helpers (rect outlines, stroke tubes, previews).
//! - [`app`] — global state, the WASM entry points (`start_app`/`on_frame`), dock UI.
//!
//! ## Architecture
//!
//! This is a single-threaded frame loop driven by the Oxide host. The host calls
//! `on_frame(delta_ms)` every frame (~60 Hz), and each frame runs three phases:
//! input, draw, and dock UI. All drawing and UI happens through the `oxide-sdk`
//! FFI boundary — canvas primitives, mouse input, UI widgets, and logging.

#![allow(static_mut_refs)]

extern crate alloc;

mod app;
mod geometry;
mod render;
mod session;
mod shapes;
mod smoothing;
