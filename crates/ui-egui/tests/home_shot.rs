//! Fork: renders Home off screen, at 2x, with `PDFCRAFT_HOME_SHOT=<file.png>`; no window opens.

use egui_kittest::Harness;
use pdfcraft_ui_egui::PdfCraftApp;

#[test]
fn home_shot() {
    let Ok(out) = std::env::var("PDFCRAFT_HOME_SHOT") else { return };
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 600.0)).with_pixels_per_point(2.0).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "pt-BR").or_else(|_| app.set_option("language", "en")).unwrap();
        for (name, pages) in [("Manifestação - Exemplo.pdf", 1), ("Procuração - Exemplo.pdf", 2)] {
            app.recent.push(pdfcraft_ui_egui::RecentFile {
                name: name.into(),
                path: format!("/Users/exemplo/Downloads/{name}"),
                pages,
                size: 136_000,
            });
        }
        app
    });
    h.run_steps(6);
    h.render().unwrap().save(out).unwrap();
}
