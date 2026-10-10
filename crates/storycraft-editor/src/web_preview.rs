//! The course preview, shown in the system web view (WebView2 on Windows,
//! WKWebView on macOS) so it behaves exactly like a published course,
//! web objects included.
//!
//! The course is published to memory and served to the web view through a
//! custom protocol, so nothing is written to disk.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use storycraft_export::Package;
use storycraft_model::Id;
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::http::{Response, StatusCode, header};

const SCHEME: &str = "storycraft";

/// Where the custom protocol's pages live, per platform (see wry's docs).
#[cfg(windows)]
const BASE: &str = "http://storycraft.localhost/";
#[cfg(not(windows))]
const BASE: &str = "storycraft://localhost/";

pub struct WebPreview {
    webview: wry::WebView,
    url: String,
    bounds: egui::Rect,
    visible: bool,
}

impl WebPreview {
    /// Open the preview over `rect` (egui points) of `window`.
    pub fn new(
        window: &eframe::Frame,
        package: Package,
        start: Option<&Id>,
        rect: egui::Rect,
        pixels_per_point: f32,
    ) -> Result<Self, String> {
        let files: Arc<RwLock<HashMap<String, Vec<u8>>>> = Arc::new(RwLock::new(package.files.into_iter().collect()));
        let served = files.clone();
        let url = match start {
            Some(slide) => format!("{BASE}index.html?sc-start={}", percent_encode(slide.as_str())),
            None => format!("{BASE}index.html"),
        };
        let webview = wry::WebViewBuilder::new()
            .with_custom_protocol(SCHEME.into(), move |_, request| {
                let path = percent_decode(request.uri().path().trim_start_matches('/'));
                let path = if path.is_empty() { "index.html".to_owned() } else { path };
                let files = served.read().expect("preview files lock");
                match files.get(&path) {
                    Some(bytes) => Response::builder()
                        .header(header::CONTENT_TYPE, content_type(&path))
                        .header(header::CACHE_CONTROL, "no-store")
                        .body(Cow::Owned(bytes.clone()))
                        .unwrap(),
                    None => Response::builder().status(StatusCode::NOT_FOUND).body(Cow::Borrowed(&[][..])).unwrap(),
                }
            })
            .with_url(&url)
            .with_bounds(to_wry(rect, pixels_per_point))
            .with_focused(true)
            .build_as_child(window)
            .map_err(|e| format!("the web preview could not start: {e}"))?;
        Ok(WebPreview { webview, url, bounds: rect, visible: true })
    }

    /// Keep the web view over `rect`, hidden while egui shows something on top
    /// (menus, dialogs), since a native child window always draws above egui.
    pub fn place(&mut self, rect: egui::Rect, pixels_per_point: f32, covered: bool) {
        if rect != self.bounds {
            self.bounds = rect;
            let _ = self.webview.set_bounds(to_wry(rect, pixels_per_point));
        }
        let visible = !covered;
        if visible != self.visible {
            self.visible = visible;
            let _ = self.webview.set_visible(visible);
        }
    }

    pub fn restart(&self) {
        let _ = self.webview.load_url(&self.url);
    }

    /// Move keyboard focus back to the editor (e.g. before closing).
    pub fn release_focus(&self) {
        let _ = self.webview.focus_parent();
    }
}

fn to_wry(rect: egui::Rect, pixels_per_point: f32) -> wry::Rect {
    let min = rect.min * pixels_per_point;
    let size = rect.size() * pixels_per_point;
    wry::Rect {
        position: PhysicalPosition::new(min.x.round() as i32, min.y.round() as i32).into(),
        size: PhysicalSize::new(size.x.max(1.0).round() as u32, size.y.max(1.0).round() as u32).into(),
    }
}

fn content_type(path: &str) -> &'static str {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(b) = hex {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What the preview panel asks the app to do after a frame.
pub enum Outcome {
    Open,
    Closed,
    /// The web view could not start; the app falls back to the built-in preview.
    Failed(String),
}

/// A preview from request to close: the published course waits here until
/// the first frame tells us where on screen the web view goes.
pub struct Session {
    pending: Option<(Package, Option<Id>)>,
    view: Option<WebPreview>,
}

impl Session {
    pub fn new(package: Package, start: Option<Id>) -> Self {
        Session { pending: Some((package, start)), view: None }
    }

    /// Show the preview panel. `covered` means egui is drawing something over
    /// it (a menu or dialog), so the web view must step aside.
    pub fn ui(&mut self, ui: &mut egui::Ui, window: &eframe::Frame, covered: bool) -> Outcome {
        let mut outcome = Outcome::Open;
        ui.horizontal(|ui| {
            if ui.button("■ Close preview").clicked() {
                outcome = Outcome::Closed;
            }
            if ui.button("⟲ Restart").clicked() {
                if let Some(view) = &self.view {
                    view.restart();
                }
            }
            ui.separator();
            ui.label(egui::RichText::new("This is the published course, running in your system web view.").weak());
        });
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray(34));
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            if covered { "The preview is hidden while a menu or dialog is open" } else { "Loading the preview…" },
            egui::FontId::proportional(14.0),
            egui::Color32::from_gray(160),
        );

        let ppp = ui.ctx().pixels_per_point();
        match (&mut self.view, self.pending.take()) {
            (Some(view), _) => view.place(rect, ppp, covered),
            (None, Some((package, start))) => match WebPreview::new(window, package, start.as_ref(), rect, ppp) {
                Ok(view) => self.view = Some(view),
                Err(e) => return Outcome::Failed(e),
            },
            (None, None) => {}
        }
        if matches!(outcome, Outcome::Closed) {
            if let Some(view) = &self.view {
                view.release_focus();
            }
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_round_trip() {
        let s = "slide id/ü?&=";
        assert_eq!(percent_decode(&percent_encode(s)), s);
        assert_eq!(percent_decode("a%20b%2"), "a b%2");
        assert_eq!(percent_decode("%41"), "A");
    }
}
