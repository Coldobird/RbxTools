//! Comparing paths the way Windows does.

use std::path::Path;

/// A path compared the way Windows does: case-insensitively, with either separator.
#[derive(Clone, Copy)]
pub struct WindowsPath<'a>(&'a Path);

impl<'a> WindowsPath<'a> {
    pub fn new(path: &'a Path) -> Self {
        Self(path)
    }

    pub fn same_as(self, other: &Path) -> bool {
        self.normalized() == WindowsPath(other).normalized()
    }

    /// Whether the file name is `name`.
    pub fn is_named(self, name: &str) -> bool {
        self.0
            .file_name()
            .is_some_and(|file| file.to_string_lossy().eq_ignore_ascii_case(name))
    }

    /// Whether the path lies somewhere below `folder`.
    pub fn is_inside(self, folder: &Path) -> bool {
        let folder = format!(
            "{}\\",
            WindowsPath(folder).normalized().trim_end_matches('\\')
        );
        self.normalized().starts_with(&folder)
    }

    fn normalized(self) -> String {
        self.0
            .to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_case_and_separators_like_windows() {
        let steam = WindowsPath::new(Path::new(r"C:\Steam\Steam.exe"));
        assert!(steam.same_as(Path::new("c:/steam/steam.exe")));
        assert!(!steam.same_as(Path::new(r"C:\Other\steam.exe")));
        assert!(steam.is_named("STEAM.EXE"));
        assert!(steam.is_inside(Path::new(r"c:\steam\")));
        assert!(!steam.is_inside(Path::new(r"C:\Ste")));
    }
}
