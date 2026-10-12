//! Fork: renders the window off screen, at 2x, with the macOS integrated title bar and a document
//! open, with `PDFCRAFT_CHROME_SHOT=<file.png>`; no window opens.

use egui_kittest::Harness;
use pdfcraft_ui_egui::PdfCraftApp;

#[test]
fn chrome_shot() {
    let Ok(out) = std::env::var("PDFCRAFT_CHROME_SHOT") else { return };
    let pdf = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >> endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \n0000000115 00000 n \ntrailer << /Size 4 /Root 1 0 R >>\nstartxref\n186\n%%EOF";
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 560.0)).with_pixels_per_point(2.0).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        let _ = app.set_option("language", "pt-BR");
        // PDFCRAFT_CHROME_WINDOWS=1: the strip under a system title bar (Windows, Linux).
        app.integrated_titlebar = std::env::var("PDFCRAFT_CHROME_WINDOWS").is_err();
        app.open_bytes("Procuração processo 0000001-00.2026.8.26.0000.pdf", None, pdf.to_vec()).unwrap();
        app.open_bytes("Manifestação - Exemplo.pdf", None, pdf.to_vec()).unwrap();
        // PDFCRAFT_CHROME_HOME=1: the document stays open in its tab and Home shows.
        if std::env::var("PDFCRAFT_CHROME_HOME").is_ok() {
            app.active = None;
        }
        app
    });
    h.run_steps(6);
    h.render().unwrap().save(out).unwrap();
}
