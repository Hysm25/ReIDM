use std::path::{Path, PathBuf};

/// Categorization system matching Internet Download Manager's FoldersTree specification
#[derive(Debug, Clone)]
pub struct Category {
    pub name: &'static str,
    pub subfolder: &'static str,
    pub extensions: &'static [&'static str],
}

pub const DEFAULT_CATEGORIES: &[Category] = &[
    Category {
        name: "Compressed",
        subfolder: "Compressed",
        extensions: &[
            "zip", "rar", "r00", "r01", "arj", "gz", "sit", "sitx", "sea", "ace", "bz2", "7z",
            "tar", "z", "iso", "img", "tgz",
        ],
    },
    Category {
        name: "Documents",
        subfolder: "Documents",
        extensions: &[
            "doc", "docx", "pdf", "ppt", "pptx", "pps", "xls", "xlsx", "txt", "rtf", "odt",
            "ods", "odp", "epub", "csv",
        ],
    },
    Category {
        name: "Music",
        subfolder: "Music",
        extensions: &[
            "mp3", "wav", "wma", "mpa", "ram", "ra", "aac", "aif", "m4a", "tsa", "flac", "ogg",
            "opus", "alac", "mid", "midi",
        ],
    },
    Category {
        name: "Programs",
        subfolder: "Programs",
        extensions: &[
            "exe", "msi", "apk", "deb", "rpm", "appimage", "dmg", "pkg", "bin", "jar",
        ],
    },
    Category {
        name: "Video",
        subfolder: "Video",
        extensions: &[
            "avi", "mpg", "mpe", "mpeg", "asf", "wmv", "mov", "qt", "rm", "rmvb", "mp4", "flv",
            "m4v", "webm", "ogv", "mkv", "ts", "tsv", "3gp", "f4v",
        ],
    },
];

/// Resolves the category and target save directory based on filename and user base download directory
pub fn determine_category(filename: &str) -> Option<&'static Category> {
    let path = Path::new(filename);
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())?;

    for cat in DEFAULT_CATEGORIES {
        for &cat_ext in cat.extensions {
            if ext == cat_ext {
                return Some(cat);
            }
        }
    }
    None
}

/// Resolves the full save path, creating the category folder if needed
pub fn resolve_destination(base_dir: &Path, filename: &str) -> (PathBuf, String) {
    if let Some(cat) = determine_category(filename) {
        let cat_dir = base_dir.join(cat.subfolder);
        (cat_dir, cat.name.to_string())
    } else {
        (base_dir.to_path_buf(), "General".to_string())
    }
}

/// Resolves duplicate filename by appending (1), (2), etc., matching IDM's conflict resolution
pub fn resolve_unique_filename(dir: &Path, filename: &str) -> String {
    let target = dir.join(filename);
    if !target.exists() {
        return filename.to_string();
    }

    let p = Path::new(filename);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = p.extension().and_then(|s| s.to_str());

    let mut counter = 1;
    loop {
        let candidate = match ext {
            Some(e) => format!("{} ({}).{}", stem, counter, e),
            None => format!("{} ({})", stem, counter),
        };
        if !dir.join(&candidate).exists() {
            return candidate;
        }
        counter += 1;
    }
}
