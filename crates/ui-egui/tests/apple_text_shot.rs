//! Fork: renders two interface lines off screen, at 2x, to compare with AppKit's rendering of the
//! same text. Runs only with `PDFCRAFT_TEXT_SHOT=<file.png>`; no window opens.

use egui::{Color32, RichText};
use egui_kittest::Harness;
use pdfcraft_ui_egui::theme;

#[test]
fn apple_text_shot() {
    let Ok(out) = std::env::var("PDFCRAFT_TEXT_SHOT") else { return };
    let dark = std::env::var("PDFCRAFT_TEXT_SHOT_DARK").is_ok();
    let (bg, body, head) = if dark {
        (Color32::from_rgb(0x26, 0x26, 0x2A), Color32::from_rgb(0xAE, 0xAE, 0xB6), Color32::from_rgb(0xEC, 0xEC, 0xEF))
    } else {
        (Color32::WHITE, Color32::from_rgb(0x5E, 0x5E, 0x66), Color32::from_rgb(0x22, 0x22, 0x26))
    };
    let mut first = true;
    let mut h = Harness::builder().with_size(egui::vec2(620.0, 60.0)).with_pixels_per_point(2.0).build_ui(move |ui| {
        if first {
            theme::install_fonts(ui.ctx());
            theme::apply(ui.ctx(), if dark { theme::ThemeKind::Dark } else { theme::ThemeKind::Light });
            first = false;
            // Fonts take effect next frame.
            return;
        }
        egui::Frame::NONE.fill(bg).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Get help, share feedback and follow development on Discord.").color(body));
            ui.label(RichText::new("Recommended tools").font(theme::semibold(15.0)).color(head));
        });
    });
    h.run_steps(3);
    h.render().unwrap().save(out).unwrap();
}
