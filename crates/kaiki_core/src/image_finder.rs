use std::collections::BTreeSet;
use std::path::Path;

use compact_str::CompactString;

/// Image file extensions to include in comparison.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tiff", "bmp", "gif"];

/// Find all image files in a directory, returning their relative paths.
pub fn find_images(dir: &Path) -> BTreeSet<CompactString> {
    let mut images = BTreeSet::new();

    if !dir.exists() {
        return images;
    }

    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !e.file_type().is_dir())
    {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase());

        if let Some(ext) = ext
            && IMAGE_EXTENSIONS.contains(&ext.as_str())
            && let Ok(relative) = path.strip_prefix(dir)
        {
            images.insert(CompactString::from(relative.to_string_lossy()));
        }
    }

    images
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn test_find_images_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let images = find_images(dir.path());
        assert!(images.is_empty());
    }

    #[test]
    fn test_find_images_nonexistent_dir() {
        let images = find_images(Path::new("/nonexistent/path/xyz"));
        assert!(images.is_empty());
    }

    #[test]
    fn test_find_images_png_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.png"), b"fake").unwrap();
        fs::write(dir.path().join("b.png"), b"fake").unwrap();
        let images = find_images(dir.path());
        assert_eq!(images.len(), 2);
        assert!(images.contains("a.png"));
        assert!(images.contains("b.png"));
    }

    #[test]
    fn test_find_images_mixed_extensions() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("img.png"), b"fake").unwrap();
        fs::write(dir.path().join("photo.jpg"), b"fake").unwrap();
        fs::write(dir.path().join("data.json"), b"fake").unwrap();
        fs::write(dir.path().join("readme.txt"), b"fake").unwrap();
        let images = find_images(dir.path());
        assert_eq!(images.len(), 2);
        assert!(images.contains("img.png"));
        assert!(images.contains("photo.jpg"));
    }

    #[test]
    fn test_find_images_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("top.png"), b"fake").unwrap();
        fs::write(dir.path().join("sub/nested.png"), b"fake").unwrap();
        let images = find_images(dir.path());
        assert_eq!(images.len(), 2);
        assert!(images.contains("top.png"));
        assert!(images.contains("sub/nested.png"));
    }
}
