pub mod properties;
pub mod slides;
pub mod timeline;
pub mod triggers;
pub mod variables;

use crate::app::{EditorApp, RightTab};

/// The right-hand panel: properties, triggers and variables.
pub fn inspector(app: &mut EditorApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let triggers = format!("Triggers ({})", app.layer().triggers.len());
        ui.selectable_value(&mut app.right_tab, RightTab::Properties, "Properties");
        ui.selectable_value(&mut app.right_tab, RightTab::Triggers, triggers);
        ui.selectable_value(&mut app.right_tab, RightTab::Variables, "Variables");
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match app.right_tab {
        RightTab::Properties => properties::show(app, ui),
        RightTab::Triggers => triggers::show(app, ui),
        RightTab::Variables => variables::show(app, ui),
    });
}
