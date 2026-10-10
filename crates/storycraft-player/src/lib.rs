//! WebAssembly entry points for published courses.
//!
//! The ABI is deliberately tiny so the JavaScript side needs no generated
//! bindings: strings cross the boundary as UTF-8 bytes in linear memory, and
//! structured data travels as JSON.
//!
//! * Inputs: JS calls `sc_alloc(len)`, copies bytes in, and passes
//!   `(ptr, len)`. The callee takes ownership and frees the buffer.
//! * Outputs: functions that produce data leave it in an output buffer and
//!   return its length; JS reads it from `sc_out_ptr()`.

use std::cell::RefCell;

use storycraft_engine::{Engine, Snapshot};
use storycraft_model::{Id, Point, Project};

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn with_engine(f: impl FnOnce(&mut Engine)) {
    ENGINE.with_borrow_mut(|e| {
        if let Some(e) = e.as_mut() {
            f(e)
        }
    })
}

fn set_out(bytes: Vec<u8>) -> usize {
    let len = bytes.len();
    OUT.with_borrow_mut(|out| *out = bytes);
    len
}

/// Take ownership of a buffer previously handed out by [`sc_alloc`].
///
/// # Safety
/// `ptr`/`len` must come from `sc_alloc(len)` and not have been freed.
unsafe fn take_string(ptr: *mut u8, len: usize) -> String {
    let bytes = unsafe { Vec::from_raw_parts(ptr, len, len) };
    String::from_utf8(bytes).unwrap_or_default()
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_alloc(len: usize) -> *mut u8 {
    let mut buf = vec![0u8; len];
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_out_ptr() -> *const u8 {
    OUT.with_borrow(|out| out.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_out_len() -> usize {
    OUT.with_borrow(|out| out.len())
}

/// Load a project (JSON). Returns 0 on success; otherwise the error message
/// is in the output buffer.
///
/// # Safety
/// See [`take_string`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sc_init(ptr: *mut u8, len: usize) -> i32 {
    let json = unsafe { take_string(ptr, len) };
    let result = serde_json::from_str::<Project>(&json)
        .map_err(|e| format!("course data is invalid: {e}"))
        .and_then(|p| Engine::new(p).map_err(|e| e.to_string()));
    match result {
        Ok(engine) => {
            ENGINE.with_borrow_mut(|e| *e = Some(engine));
            0
        }
        Err(msg) => {
            set_out(msg.into_bytes());
            1
        }
    }
}

/// Restore saved progress (JSON [`Snapshot`]). Invalid data is ignored so a
/// corrupt bookmark never blocks the course.
///
/// # Safety
/// See [`take_string`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sc_restore(ptr: *mut u8, len: usize) {
    let json = unsafe { take_string(ptr, len) };
    if let Ok(snapshot) = serde_json::from_str::<Snapshot>(&json) {
        with_engine(|e| e.restore(&snapshot));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_start() {
    with_engine(|e| e.start());
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_tick(dt_ms: f64) {
    with_engine(|e| e.tick(dt_ms));
}

/// `inside == 0` means the pointer left the slide.
#[unsafe(no_mangle)]
pub extern "C" fn sc_pointer_move(x: f32, y: f32, inside: i32) {
    with_engine(|e| e.pointer_move((inside != 0).then_some(Point { x, y })));
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_pointer_down(x: f32, y: f32) {
    with_engine(|e| e.pointer_down(Point { x, y }));
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_pointer_up(x: f32, y: f32) {
    with_engine(|e| e.pointer_up(Point { x, y }));
}

/// Activate an object by id (keyboard users).
///
/// # Safety
/// See [`take_string`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sc_click(ptr: *mut u8, len: usize) {
    let id = unsafe { take_string(ptr, len) };
    with_engine(|e| e.click(&Id::from(id)));
}

/// # Safety
/// See [`take_string`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sc_key(ptr: *mut u8, len: usize) {
    let key = unsafe { take_string(ptr, len) };
    with_engine(|e| e.key_pressed(&key));
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_next() {
    with_engine(|e| e.next_slide());
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_prev() {
    with_engine(|e| e.previous_slide());
}

/// Current [`storycraft_engine::Frame`] as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn sc_frame() -> usize {
    let mut json = Vec::new();
    with_engine(|e| json = serde_json::to_vec(&e.frame()).unwrap_or_default());
    set_out(json)
}

/// Pending [`storycraft_engine::Effect`]s as a JSON array (drains them).
#[unsafe(no_mangle)]
pub extern "C" fn sc_effects() -> usize {
    let mut json = b"[]".to_vec();
    with_engine(|e| json = serde_json::to_vec(&e.take_effects()).unwrap_or_default());
    set_out(json)
}

/// Learner progress as JSON, for LMS suspend data.
#[unsafe(no_mangle)]
pub extern "C" fn sc_snapshot() -> usize {
    let mut json = Vec::new();
    with_engine(|e| json = serde_json::to_vec(&e.snapshot()).unwrap_or_default());
    set_out(json)
}
