// SPDX-License-Identifier: MIT OR Apache-2.0
//! Original Cueglint artwork; geometry source and exports live in `ui/brand`.

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Ui, vec2};

pub fn mark(ui: &mut Ui, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    paint(ui.painter(), rect.center(), size, color);
}

pub fn paint(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    let point = |x, y| center + vec2((x - 32.0) * size / 64.0, (y - 32.0) * size / 64.0);
    let mut curve = Vec::with_capacity(35);
    curve.push(point(39.0, 12.0));
    for index in 0..=32 {
        let angle = index as f32 * std::f32::consts::PI / 32.0;
        curve.push(point(28.0 - 20.0 * angle.sin(), 32.0 - 20.0 * angle.cos()));
    }
    curve.push(point(39.0, 52.0));
    painter.add(egui::Shape::line(curve, Stroke::new(size / 8.0, color)));
    painter.circle_filled(point(39.0, 12.0), size / 16.0, color);
    painter.circle_filled(point(39.0, 52.0), size / 16.0, color);
    // Overlapping convex diamonds give the glint a correct concave silhouette.
    for diamond in [
        [(47.0, 10.0), (52.0, 24.0), (47.0, 38.0), (42.0, 24.0)],
        [(34.0, 24.0), (47.0, 19.0), (60.0, 24.0), (47.0, 29.0)],
    ] {
        painter.add(egui::Shape::convex_polygon(
            diamond.into_iter().map(|(x, y)| point(x, y)).collect(),
            color,
            Stroke::NONE,
        ));
    }
}

pub fn app_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!(
        "../../../ui/brand/cueglint-app-icon-256.png"
    ))
    .expect("embedded Cueglint icon is a verified PNG")
    .into_rgba8();
    egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn embedded_icon_has_transparent_corners_and_teal_artwork() {
        let icon = app_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        for (x, y) in [(0, 0), (255, 0), (0, 255), (255, 255)] {
            assert_eq!(icon.rgba[(y * 256 + x) * 4 + 3], 0);
        }
        assert!(
            icon.rgba
                .chunks_exact(4)
                .any(|rgba| rgba == [149, 220, 204, 255])
        );
    }

    #[test]
    fn native_mark_stays_inside_its_allocation() {
        let context = egui::Context::default();
        let bounds = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(64.0, 64.0));
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            paint(ui.painter(), pos2(32.0, 32.0), 64.0, Color32::WHITE);
        });
        assert!(!output.shapes.is_empty());
        for shape in output.shapes {
            assert!(bounds.contains_rect(shape.shape.visual_bounding_rect()));
        }
    }
}
