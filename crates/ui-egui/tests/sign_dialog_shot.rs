//! Fork: renders Sign with a Digital ID off screen, with the OS key stores and PKCS #11 tokens
//! listed, with `PDFCRAFT_SIGN_SHOT=<file.png>`; no window opens. It loads the real modules.

use egui_kittest::Harness;
use pdfcraft_ui_egui::PdfCraftApp;

#[test]
fn sign_dialog_shot() {
    let Ok(out) = std::env::var("PDFCRAFT_SIGN_SHOT") else { return };
    let pdf = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF";
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 760.0)).with_pixels_per_point(2.0).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        let _ = app.set_option("language", "pt-BR");
        app.os_key_store_ids = true;
        app.open_bytes("exemplo.pdf", None, pdf.to_vec()).unwrap();
        app.start_signing(0, Some([100.0, 100.0, 300.0, 160.0]), None, None);
        app
    });
    h.run_steps(6);
    h.render().unwrap().save(out).unwrap();
}
