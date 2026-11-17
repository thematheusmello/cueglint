// SPDX-License-Identifier: MIT OR Apache-2.0
//! Real window/AccessKit regression; no sessions, capture, models or inference.
use eframe::egui::{self, ViewportBuilder, ViewportClass, ViewportCommand, ViewportId};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct Check(Arc<AtomicBool>);

impl eframe::App for Check {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ui.label("Cueglint native child startup check");
        for (name, visible) in [
            ("default", None),
            ("shown", Some(true)),
            ("hidden", Some(false)),
        ] {
            let mut builder = ViewportBuilder::default()
                .with_title(format!("Cueglint child check: {name}"))
                .with_inner_size([320.0, 120.0]);
            builder.visible = visible;
            ctx.show_viewport_immediate(ViewportId::from_hash_of(name), builder, |ui, class| {
                assert!(
                    matches!(class, ViewportClass::Immediate),
                    "must create a real native child"
                );
                assert!(
                    ui.input(|input| input.viewport().native_pixels_per_point)
                        .is_some_and(|scale| scale.is_finite() && scale > 0.0)
                );
                ui.label("Original native window fixture; no capture or inference.");
            });
        }
        self.0.store(true, Ordering::SeqCst);
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

fn main() -> eframe::Result {
    let checked = Arc::new(AtomicBool::new(false));
    let result = Arc::clone(&checked);
    eframe::run_native(
        "Cueglint native child check",
        eframe::NativeOptions {
            persist_window: false,
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(false);
            Ok(Box::new(Check(result)))
        }),
    )?;
    assert!(
        checked.load(Ordering::SeqCst),
        "native child checks must execute"
    );
    println!("Native default/shown/hidden child initialization passed with AccessKit enabled.");
    Ok(())
}
