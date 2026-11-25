//! The native file dialog, as the app uses it.
//!
//! With the `test-no-launch` feature (on only for this crate's own tests, through its
//! dev-dependency on itself) every dialog answers "cancelled" without opening, and
//! [`crate::ui::open_with_default_app`] starts nothing. Tests, and mutation testing above all,
//! can then reach any menu item or shortcut without a dialog blocking the run or a browser
//! tab opening on the desktop.

#[cfg(not(feature = "test-no-launch"))]
pub(crate) use rfd::FileDialog;

#[cfg(feature = "test-no-launch")]
pub(crate) use stub::FileDialog;

/// True when the app may start other programs or open native dialogs.
pub(crate) const LAUNCHES: bool = !cfg!(feature = "test-no-launch");

#[cfg(feature = "test-no-launch")]
mod stub {
    use std::path::PathBuf;

    /// Stands in for `rfd::FileDialog`: the same builder calls, and the user always cancels.
    pub(crate) struct FileDialog;

    impl FileDialog {
        pub(crate) fn new() -> Self {
            FileDialog
        }

        pub(crate) fn add_filter(self, _name: impl Into<String>, _extensions: &[&str]) -> Self {
            self
        }

        pub(crate) fn set_file_name(self, _name: impl Into<String>) -> Self {
            self
        }

        pub(crate) fn set_title(self, _title: impl Into<String>) -> Self {
            self
        }

        pub(crate) fn pick_file(self) -> Option<PathBuf> {
            None
        }

        pub(crate) fn pick_folder(self) -> Option<PathBuf> {
            None
        }

        pub(crate) fn save_file(self) -> Option<PathBuf> {
            None
        }
    }
}
