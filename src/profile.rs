use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

use crate::mapping::Profile;

fn config_dir() -> PathBuf {
    ProjectDirs::from("", "", "space-controller")
        .map(|p| p.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".space-controller"))
}

pub fn profiles_dir() -> PathBuf {
    config_dir().join("profiles")
}

pub fn list_profiles() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(profiles_dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
                .filter_map(|e| e.path().file_stem()?.to_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

pub fn load(name: &str) -> Result<Profile> {
    let path = profiles_dir().join(format!("{name}.toml"));
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn save(profile: &Profile) -> Result<PathBuf> {
    let dir = profiles_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.toml", profile.name));
    let text = toml::to_string_pretty(profile)?;
    fs::write(&path, &text)?;
    Ok(path)
}
