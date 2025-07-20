use std::path::PathBuf;

pub struct TreeItem {
    pub path: PathBuf,
    pub prefix: String,
    pub is_dir: bool,
}
