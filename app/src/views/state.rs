use slint::ComponentHandle;

impl crate::OrcaWindow {
    pub(crate) fn shell(&self) -> crate::AppState<'_> {
        self.global()
    }
    pub(crate) fn library(&self) -> crate::LibraryState<'_> {
        self.global()
    }
    pub(crate) fn player(&self) -> crate::PlayerState<'_> {
        self.global()
    }
    pub(crate) fn lyrics(&self) -> crate::LyricsState<'_> {
        self.global()
    }
    pub(crate) fn preferences(&self) -> crate::PreferencesState<'_> {
        self.global()
    }
    pub(crate) fn dialogs(&self) -> crate::DialogState<'_> {
        self.global()
    }
}
