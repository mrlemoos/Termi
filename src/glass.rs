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
