//! Parser for Unreal Engine `.uasset` / `.umap` packages (editor format, UE 4.x - 5.8) with
//! LLM friendly text output.

pub mod errors;
pub mod graph;
pub mod object;
pub mod package;
pub mod property;
pub mod reader;
pub mod render;
pub mod structs;
pub mod text;
pub mod versions;

use std::path::Path;

use errors::Result;

/// Fully parsed package.
pub struct Asset {
    pub package: package::Package,
    pub objects: Vec<object::Object>,
}

impl Asset {
    pub fn from_bytes(data: Vec<u8>) -> Result<Asset> {
        let package = package::Package::from_bytes(data)?;
        let objects = object::read_all(&package);
        Ok(Asset { package, objects })
    }

    pub fn open(path: &Path) -> Result<Asset> {
        let data = std::fs::read(path)?;
        let mut package = package::Package::from_bytes(data)?;
        if package.package_path().is_empty() {
            package.path_hint = guess_package_path(path);
        }
        let objects = object::read_all(&package);
        Ok(Asset { package, objects })
    }

    pub fn to_text(&self, opts: &render::Options) -> String {
        render::render(&self.package, &self.objects, opts)
    }

    pub fn to_json(&self, file: &str) -> String {
        render::json::render(&self.package, &self.objects, file)
    }
}

/// Guesses "/Game/Foo/Bar" from ".../Content/Foo/Bar.uasset".
pub fn guess_package_path(path: &Path) -> Option<String> {
    let s = path.to_string_lossy().replace('\\', "/");
    let stem = s.rsplit_once('.').map(|(a, _)| a).unwrap_or(&s);
    if let Some(pos) = stem.rfind("/Content/") {
        let rest = &stem[pos + "/Content/".len()..];
        let root = &stem[..pos];
        let mount = if root.ends_with("/Engine") {
            "Engine".to_string()
        } else if let Some(plugin_pos) = root.rfind("/Plugins/") {
            root[plugin_pos + 9..].rsplit('/').next().unwrap_or("Game").to_string()
        } else {
            "Game".to_string()
        };
        return Some(format!("/{mount}/{rest}"));
    }
    Path::new(stem).file_name().map(|f| format!("/Game/{}", f.to_string_lossy()))
}
