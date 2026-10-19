//! Loading picture files into textures for the plan and 3D views.

use std::cell::RefCell;
use std::collections::HashMap;

use egui::{Color32, ColorImage, Context, Painter, Pos2, TextureHandle, TextureOptions};
use housecraft_api::model::Color;

type Key = (String, Option<[u8; 4]>);

thread_local! {
    /// Loaded textures by file and transparent colour; `None` if the file
    /// could not be read.
    static CACHE: RefCell<HashMap<Key, Option<TextureHandle>>> = RefCell::new(HashMap::new());
}

/// Pixel size of an image file, without loading it as a texture.
pub fn pixel_size(path: &str) -> Option<[u32; 2]> {
    image::image_dimensions(path).ok().map(|(w, h)| [w, h])
}

/// The texture for `path` with pixels of `transparent` see-through.
pub fn texture(ctx: &Context, path: &str, transparent: Option<Color>) -> Option<TextureHandle> {
    let key = (path.to_string(), transparent.map(|c| c.0));
    CACHE.with(|c| {
        c.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let img = image::open(path).ok()?.to_rgba8();
                let (w, h) = img.dimensions();
                let mut pixels = img.into_raw();
                if let Some(Color([r, g, b, _])) = transparent {
                    // Key out the colour, with a little tolerance for JPEG noise.
                    for px in pixels.as_chunks_mut::<4>().0 {
                        let d = (px[0] as i32 - r as i32).abs()
                            + (px[1] as i32 - g as i32).abs()
                            + (px[2] as i32 - b as i32).abs();
                        if d < 24 {
                            px[3] = 0;
                        }
                    }
                }
                let image = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels);
                Some(ctx.load_texture(path, image, TextureOptions::LINEAR))
            })
            .clone()
    })
}

/// The texture for `path`, repeating (for panoramas wrapping round).
pub fn texture_repeat(ctx: &Context, path: &str) -> Option<TextureHandle> {
    REPEAT.with(|c| {
        c.borrow_mut()
            .entry(path.to_string())
            .or_insert_with(|| {
                let img = image::open(path).ok()?.to_rgba8();
                let (w, h) = img.dimensions();
                let image = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img.into_raw());
                let options = TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..TextureOptions::LINEAR };
                Some(ctx.load_texture(format!("{path}#repeat"), image, options))
            })
            .clone()
    })
}

thread_local! {
    static REPEAT: std::cell::RefCell<std::collections::HashMap<String, Option<TextureHandle>>> = Default::default();
}

/// Paint an image stretched over screen corners bottom-left, bottom-right,
/// top-right, top-left; a grey frame stands in when the file is missing.
pub fn paint(
    ctx: &Context,
    painter: &Painter,
    corners: [Pos2; 4],
    path: &str,
    opacity: f64,
    transparent: Option<Color>,
) {
    let Some(tex) = texture(ctx, path, transparent) else {
        let mut pts = corners.to_vec();
        pts.push(corners[0]);
        painter.add(egui::Shape::dashed_line(
            &pts,
            egui::Stroke::new(1.0, Color32::GRAY),
            4.0,
            3.0,
        ));
        painter.line_segment([corners[0], corners[2]], egui::Stroke::new(1.0, Color32::GRAY));
        return;
    };
    let tint = Color32::from_white_alpha((opacity.clamp(0.0, 1.0) * 255.0) as u8);
    let mut mesh = egui::Mesh::with_texture(tex.id());
    let uvs = [
        Pos2::new(0.0, 1.0),
        Pos2::new(1.0, 1.0),
        Pos2::new(1.0, 0.0),
        Pos2::new(0.0, 0.0),
    ];
    for (p, uv) in corners.iter().zip(uvs) {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *p,
            uv,
            color: tint,
        });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

/// The average colour of an image file.
pub fn average_color(path: &str) -> Option<Color> {
    let img = image::open(path).ok()?.to_rgb8();
    let n = (img.width() as u64 * img.height() as u64).max(1);
    let mut sum = [0u64; 3];
    for p in img.pixels() {
        for (s, v) in sum.iter_mut().zip(p.0) {
            *s += v as u64;
        }
    }
    let c = |k: usize| (sum[k] / n) as u8;
    Some(Color::rgb(c(0), c(1), c(2)))
}
