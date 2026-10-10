use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

#[cfg(target_os = "macos")]
pub fn blur(effect: &mut Option<objc2::rc::Retained<objc2_app_kit::NSVisualEffectView>>, enabled: bool) {
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSApplication, NSAutoresizingMaskOptions, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView};
    if !enabled {
        if let Some(view) = effect.take() {
            if let (Some(window), Some(content)) = (view.window(), view.subviews().firstObject()) {
                window.setContentView(Some(&content));
                window.makeFirstResponder(Some(&content));
            }
        }
        return;
    }
    if effect.is_none() && enabled {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let Some(window) = NSApplication::sharedApplication(mtm).windows().firstObject() else { return };
        let Some(content) = window.contentView() else { return };
        let view = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content.bounds());
        view.setMaterial(NSVisualEffectMaterial::HUDWindow);
        view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        view.setState(NSVisualEffectState::Active);
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        content.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        window.setContentView(Some(&view));
        view.addSubview(&content);
        window.makeFirstResponder(Some(&content));
        *effect = Some(view);
    }
}

fn light(time: f64) -> Vec2 {
    let phase = (time * 0.18) as f32;
    egui::vec2(0.5 + 0.32 * phase.sin(), 0.5 + 0.24 * (phase * 2.0).sin())
}

pub fn paint(painter: &egui::Painter, rect: Rect, time: f64, bright: bool) {
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
                220,
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
