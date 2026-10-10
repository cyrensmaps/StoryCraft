//! GPU textures for image assets, decoded on first use.

use std::collections::{BTreeMap, HashMap};

use egui::{ColorImage, TextureHandle, TextureId, TextureOptions};
use storycraft_model::Id;

#[derive(Default)]
pub struct Textures {
    /// `None` records a failed decode so it is not retried every frame.
    loaded: HashMap<Id, Option<TextureHandle>>,
}

impl Textures {
    pub fn get(&mut self, ctx: &egui::Context, id: &Id, media: &BTreeMap<Id, Vec<u8>>) -> Option<TextureId> {
        self.loaded
            .entry(id.clone())
            .or_insert_with(|| {
                let bytes = media.get(id)?;
                let image = image::load_from_memory(bytes).ok()?.to_rgba8();
                let size = [image.width() as usize, image.height() as usize];
                let color = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                Some(ctx.load_texture(format!("asset-{id}"), color, TextureOptions::LINEAR))
            })
            .as_ref()
            .map(|t| t.id())
    }

    pub fn clear(&mut self) {
        self.loaded.clear();
    }
}

/// Pixel size of an encoded image, if it can be decoded.
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    reader.into_dimensions().ok()
}
