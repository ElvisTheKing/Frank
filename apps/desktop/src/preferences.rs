use serde::{Deserialize, Serialize};
use ui_egui::{AppTheme, RawModeChoice, UiState};
use viewer_model::{LayoutMode, SyncMode, TitleFields, Workspace};

pub(crate) const PREFERENCES_KEY: &str = "preferences-v1";
const PREFERENCES_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersistedPreferences {
    version: u32,
    theme: AppTheme,
    clean_view: bool,
    show_pixel_grid: bool,
    develop_raws_on_load: bool,
    raw_mode: RawModeChoice,
    sync_adjustments: bool,
    synchronized: bool,
    sync_mode: SyncMode,
    layout_mode: LayoutMode,
    title_fields: TitleFields,
}

impl PersistedPreferences {
    pub(crate) fn capture(workspace: &Workspace, ui_state: &UiState) -> Self {
        Self {
            version: PREFERENCES_VERSION,
            theme: ui_state.theme,
            clean_view: !ui_state.show_pane_controls,
            show_pixel_grid: ui_state.show_pixel_grid,
            develop_raws_on_load: ui_state.develop_raws_on_load,
            raw_mode: ui_state.raw_mode,
            sync_adjustments: ui_state.sync_adjustments,
            synchronized: workspace.synchronized,
            sync_mode: workspace.sync_mode,
            layout_mode: workspace.layout_mode,
            title_fields: workspace.title_fields,
        }
    }

    pub(crate) fn apply(self, workspace: &mut Workspace, ui_state: &mut UiState) {
        ui_state.theme = self.theme;
        ui_state.show_pane_controls = !self.clean_view;
        ui_state.show_pixel_grid = self.show_pixel_grid;
        ui_state.develop_raws_on_load = self.develop_raws_on_load;
        ui_state.raw_mode = self.raw_mode;
        ui_state.sync_adjustments = self.sync_adjustments;
        workspace.synchronized = self.synchronized;
        workspace.sync_mode = self.sync_mode;
        workspace.layout_mode = self.layout_mode;
        workspace.title_fields = self.title_fields;
    }

    pub(crate) fn load(storage: &dyn eframe::Storage) -> Option<Self> {
        let preferences: Self = eframe::get_value(storage, PREFERENCES_KEY)?;
        (preferences.version == PREFERENCES_VERSION).then_some(preferences)
    }
}

impl Default for PersistedPreferences {
    fn default() -> Self {
        Self::capture(&Workspace::demo(), &UiState::default())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use viewer_model::ImageId;

    #[derive(Default)]
    struct TestStorage(HashMap<String, String>);

    impl eframe::Storage for TestStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn preferences_round_trip_without_session_state() {
        let mut workspace = Workspace::demo();
        workspace.synchronized = false;
        workspace.sync_mode = SyncMode::SourcePixels;
        workspace.layout_mode = LayoutMode::Row;
        workspace.title_fields.lens = true;
        workspace.panes[0].image_id = Some(ImageId(99));

        let mut ui_state = UiState::default();
        ui_state.theme = AppTheme::Light;
        ui_state.show_pane_controls = false;
        ui_state.show_pixel_grid = true;
        ui_state.develop_raws_on_load = true;
        ui_state.raw_mode = RawModeChoice::AsShot;
        ui_state.sync_adjustments = true;

        let expected = PersistedPreferences::capture(&workspace, &ui_state);
        let mut storage = TestStorage::default();
        eframe::set_value(&mut storage, PREFERENCES_KEY, &expected);
        let loaded = PersistedPreferences::load(&storage).expect("version 1 preferences load");
        assert_eq!(loaded, expected);

        let mut restored_workspace = Workspace::demo();
        let mut restored_ui = UiState::default();
        loaded.apply(&mut restored_workspace, &mut restored_ui);
        assert_eq!(
            PersistedPreferences::capture(&restored_workspace, &restored_ui),
            expected
        );
        assert!(
            restored_workspace
                .panes
                .iter()
                .all(|pane| pane.image_id.is_none())
        );
    }

    #[test]
    fn unknown_preferences_version_falls_back_to_defaults() {
        let mut storage = TestStorage::default();
        let preferences = PersistedPreferences {
            version: PREFERENCES_VERSION + 1,
            ..PersistedPreferences::default()
        };
        eframe::set_value(&mut storage, PREFERENCES_KEY, &preferences);
        assert!(PersistedPreferences::load(&storage).is_none());
    }
}
