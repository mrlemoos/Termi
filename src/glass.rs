use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

#[cfg(target_os = "macos")]
pub fn blur(effect: &mut Option<objc2::rc::Retained<objc2_app_kit::NSVisualEffectView>>, frame: &eframe::Frame, enabled: bool) {
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if !enabled {
        if let Some(view) = effect.take() {
            view.removeFromSuperview();
        }
        return;
    }
    if effect.is_none() {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let Ok(handle) = frame.window_handle() else { return };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return };
        // Frame owns this live NSView; access stays on the main thread.
        let content = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
        let (Some(window), Some(parent)) = (content.window(), unsafe { content.superview() }) else { return };
        let view = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content.frame());
        view.setMaterial(NSVisualEffectMaterial::HUDWindow);
        view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        view.setState(NSVisualEffectState::Active);
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        // Winit casts window.contentView() to WinitView. Never replace it.
        parent.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, Some(content));
        debug_assert!(window.contentView().is_some_and(|v| std::ptr::eq::<NSView>(&*v, content)));
        *effect = Some(view);
    }
}

fn light(time: f64) -> Vec2 {
    let phase = (time * 0.18) as f32;
    egui::vec2(0.5 + 0.32 * phase.sin(), 0.5 + 0.24 * (phase * 2.0).sin())
}

pub fn paint(painter: &egui::Painter, rect: Rect, time: f64, bright: bool, opacity: f32) {
    let centre = light(time);
    let base = if bright { [226.0, 229.0, 239.0] } else { [20.0, 25.0, 39.0] };
    let mut mesh = egui::Mesh::default();
    const STEPS: u32 = 32;
    for y in 0..=STEPS {
        for x in 0..=STEPS {
            let uv = egui::vec2(x as f32 / STEPS as f32, y as f32 / STEPS as f32);
            let delta = uv - centre;
            let glow = (-8.0 * delta.length_sq()).exp();
            let tint = [55.0, 43.0, 105.0];
            let colour = Color32::from_rgba_unmultiplied(
                (base[0] + tint[0] * glow) as u8,
                (base[1] + tint[1] * glow) as u8,
                (base[2] + tint[2] * glow) as u8,
                (opacity * 255.0).round() as u8,
            );
            mesh.colored_vertex(Pos2::new(rect.left() + uv.x * rect.width(), rect.top() + uv.y * rect.height()), colour);
            if x < STEPS && y < STEPS {
                let i = y * (STEPS + 1) + x;
                mesh.add_triangle(i, i + 1, i + STEPS + 1);
                mesh.add_triangle(i + 1, i + STEPS + 2, i + STEPS + 1);
            }
        }
    }
    painter.add(mesh);
}

fn chip_fill(bright: bool, selected: bool) -> Color32 {
    if bright {
        Color32::from_rgba_unmultiplied(255, 255, 255, if selected { 198 } else { 124 })
    } else if selected {
        Color32::from_rgba_unmultiplied(232, 238, 255, 96)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 44)
    }
}

/// Frosted strip. Alpha falls off toward the bottom so the terminal shows through.
pub fn bar(painter: &egui::Painter, rect: Rect, bright: bool) {
    let mut mesh = egui::Mesh::default();
    const STEPS: u32 = 24;
    let frost = if bright { 170.0 } else { 64.0 };
    for y in 0..=STEPS {
        for x in 0..=STEPS {
            let v = y as f32 / STEPS as f32;
            let alpha = (frost * (1.0 - v).powf(1.35)).clamp(0.0, 220.0);
            let colour = Color32::from_rgba_unmultiplied(236, 240, 255, alpha as u8);
            mesh.colored_vertex(Pos2::new(rect.left() + x as f32 / STEPS as f32 * rect.width(), rect.top() + v * rect.height()), colour);
            if x < STEPS && y < STEPS {
                let i = y * (STEPS + 1) + x;
                mesh.add_triangle(i, i + 1, i + STEPS + 1);
                mesh.add_triangle(i + 1, i + STEPS + 2, i + STEPS + 1);
            }
        }
    }
    painter.add(mesh);
    painter.rect_filled(
        Rect::from_min_max(rect.left_top(), Pos2::new(rect.right(), rect.top() + 1.0)),
        0.0,
        Color32::from_white_alpha(if bright { 190 } else { 100 }),
    );
}

/// One tab capsule.
pub fn chip(painter: &egui::Painter, rect: Rect, bright: bool, selected: bool) {
    let radius = rect.height() * 0.5;
    painter.rect_filled(rect.translate(egui::vec2(0.0, 1.0)), radius, Color32::from_black_alpha(if selected { 70 } else { 34 }));
    painter.rect_filled(rect, radius, chip_fill(bright, selected));
    let lip = Rect::from_min_max(rect.min + egui::vec2(radius * 0.5, 1.0), Pos2::new(rect.max.x - radius * 0.5, rect.min.y + 2.0));
    painter.rect_filled(lip, 1.0, Color32::from_white_alpha(if selected { 180 } else { 84 }));
    let rim = if bright {
        Color32::from_white_alpha(if selected { 230 } else { 150 })
    } else {
        Color32::from_white_alpha(if selected { 128 } else { 58 })
    };
    painter.rect_stroke(rect, radius, egui::Stroke::new(1.0, rim), egui::StrokeKind::Inside);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_controls_only_background_alpha() {
        for (opacity, alpha) in [(0.0, 0), (0.5, 128), (1.0, 255)] {
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(Default::default(), |ui| {
                paint(&ui.ctx().layer_painter(egui::LayerId::background()), Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0)), 0.0, false, opacity);
            });
            let egui::Shape::Mesh(mesh) = &output.shapes[0].shape else { panic!("expected glass mesh") };
            assert!(mesh.vertices.iter().all(|v| v.color.a() == alpha));
            output.textures_delta.clear();
        }
    }

    #[test]
    fn selected_chip_is_frosted_not_solid() {
        for bright in [false, true] {
            let on = chip_fill(bright, true).a();
            let off = chip_fill(bright, false).a();
            assert!(on > off, "bright={bright}");
            assert!(on < 255, "bright={bright}");
        }
        assert!(chip_fill(true, true).a() > chip_fill(false, true).a());
    }

    #[test]
    fn bar_frost_is_even_and_fades_down() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(200.0, 48.0));
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            bar(&ui.painter(), rect, false);
        });
        let egui::Shape::Mesh(mesh) = &output.shapes[0].shape else { panic!("expected glass mesh") };
        let top: Vec<u8> = mesh.vertices.iter().filter(|v| v.pos.y == rect.top()).map(|v| v.color.a()).collect();
        let bottom_clear = mesh.vertices.iter().filter(|v| v.pos.y == rect.bottom()).all(|v| v.color.a() == 0);
        output.textures_delta.clear();
        assert!(top.len() > 2 && top.windows(2).all(|pair| pair[0] == pair[1]), "highlight should not sit in one place");
        assert!(top[0] > 40);
        assert!(bottom_clear);
    }

    #[test]
    fn light_moves_in_a_bounded_wave() {
        assert_eq!(light(0.0), egui::vec2(0.5, 0.5));
        assert_ne!(light(0.0), light(5.0));
        for t in 0..1000 {
            let p = light(t as f64);
            assert!((0.17..=0.83).contains(&p.x));
            assert!((0.25..=0.75).contains(&p.y));
        }
    }
}
