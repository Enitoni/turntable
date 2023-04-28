use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source;

#[derive(Debug, Serialize, Deserialize)]
pub struct Cache {
    entries: BTreeMap<String, Source>,
    size: usize,
}

impl Cache {
    const CACHE_FOLDER: &'static str = "./source-cache";
    const CACHE_DATA_FILE: &'static str = "cache.ron";

    const MAX_SIZE: usize = 1024_usize.pow(3);

    fn new() -> Self {
        Self {
            entries: Default::default(),
            size: Default::default(),
        }
    }

    pub fn restore() -> Self {
        let mut path = PathBuf::from(Self::CACHE_FOLDER);

        fs::create_dir_all(&path).expect("create cache folder");
        path.push(Self::CACHE_DATA_FILE);

        fs::read_to_string(&path)
            .map(|data| ron::from_str(&data).expect("cache file is valid"))
            .unwrap_or_else(|_| Self::new())
    }

    fn save(&self) {
        let data = ron::to_string(&self).expect("serialize cache");

        let mut path = PathBuf::from(Self::CACHE_FOLDER);
        path.push(Self::CACHE_DATA_FILE);

        fs::write(path, data).expect("save serialized cache to file");
    }

    pub fn get_by_fingerprint(&self, fingerprint: &str) -> Option<Source> {
        self.entries.get(fingerprint).cloned()
    }
}
