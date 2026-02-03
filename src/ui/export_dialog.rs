pub fn render_export_dialog(ctx: &egui::Context, export_result: &mut Option<String>) {
    let Some(result) = export_result.as_ref() else {
        return;
    };

    let result_clone = result.clone();

    egui::Window::new("Export Complete")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.label("Session exported to:");
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&result_clone).monospace().strong());
            ui.add_space(12.0);

            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    *export_result = None;
                }
            });
            ui.add_space(4.0);
        });
}
