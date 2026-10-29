//! Package header: summary, name/import/export tables and other header sections.

use std::collections::HashMap;

use crate::errors::{ParseError, Result, invalid};
use crate::reader::{Guid, Reader};
use crate::versions::{ue4, ue5};

pub const PACKAGE_FILE_TAG: u32 = 0x9E2A83C1;
pub const PACKAGE_FILE_TAG_SWAPPED: u32 = 0xC1832A9E;

pub const PKG_FILTER_EDITOR_ONLY: u32 = 0x8000_0000;
pub const PKG_UNVERSIONED_PROPERTIES: u32 = 0x0000_2000;
pub const PKG_COOKED: u32 = 0x0000_0200;

#[derive(Debug, Clone, Default)]
pub struct EngineVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub changelist: u32,
    pub branch: String,
}

impl std::fmt::Display for EngineVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}-{}+{}", self.major, self.minor, self.patch, self.changelist & 0x7fff_ffff, self.branch)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub legacy_file_version: i32,
    pub file_version_ue4: i32,
    pub file_version_ue5: i32,
    pub file_version_licensee: i32,
    pub unversioned: bool,
    pub saved_hash: Vec<u8>,
    pub total_header_size: i32,
    pub custom_versions: Vec<(Guid, i32)>,
    pub package_name: String,
    pub package_flags: u32,
    pub name_count: i32,
    pub name_offset: i32,
    pub soft_object_paths_count: i32,
    pub soft_object_paths_offset: i32,
    pub localization_id: String,
    pub gatherable_text_data_count: i32,
    pub gatherable_text_data_offset: i32,
    pub export_count: i32,
    pub export_offset: i32,
    pub import_count: i32,
    pub import_offset: i32,
    pub cell_export_count: i32,
    pub cell_export_offset: i32,
    pub cell_import_count: i32,
    pub cell_import_offset: i32,
    pub metadata_offset: i32,
    pub depends_offset: i32,
    pub soft_package_references_count: i32,
    pub soft_package_references_offset: i32,
    pub searchable_names_offset: i32,
    pub thumbnail_table_offset: i32,
    pub persistent_guid: Option<Guid>,
    pub saved_by: EngineVersion,
    pub compatible_with: EngineVersion,
    pub compression_flags: u32,
    pub package_source: u32,
    pub asset_registry_data_offset: i32,
    pub bulk_data_start_offset: i64,
    pub world_tile_info_data_offset: i32,
    pub chunk_ids: Vec<i32>,
    pub preload_dependency_count: i32,
    pub preload_dependency_offset: i32,
    pub names_referenced_from_export_data_count: i32,
    pub payload_toc_offset: i64,
    pub data_resource_offset: i32,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub class_package: String,
    pub class_name: String,
    pub outer_index: i32,
    pub object_name: String,
    pub package_name: String,
    pub optional: bool,
}

#[derive(Debug, Clone)]
pub struct Export {
    pub class_index: i32,
    pub super_index: i32,
    pub template_index: i32,
    pub outer_index: i32,
    pub object_name: String,
    pub object_flags: u32,
    pub serial_size: i64,
    pub serial_offset: i64,
    pub forced_export: bool,
    pub not_for_client: bool,
    pub not_for_server: bool,
    pub is_inherited_instance: bool,
    pub package_flags: u32,
    pub not_always_loaded_for_editor_game: bool,
    pub is_asset: bool,
    pub generate_public_hash: bool,
    pub script_serialization_start_offset: i64,
    pub script_serialization_end_offset: i64,
}

pub const RF_PUBLIC: u32 = 0x1;
pub const RF_STANDALONE: u32 = 0x2;
pub const RF_TRANSACTIONAL: u32 = 0x8;
pub const RF_CLASS_DEFAULT_OBJECT: u32 = 0x10;
pub const RF_ARCHETYPE_OBJECT: u32 = 0x20;

#[derive(Debug, Clone)]
pub struct AssetRegistryEntry {
    pub object_path: String,
    pub object_class: String,
    pub tags: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct PackageMetaData {
    /// (object path, key/value pairs)
    pub objects: Vec<(String, Vec<(String, String)>)>,
    pub root: Vec<(String, String)>,
}

pub struct Package {
    pub data: Vec<u8>,
    pub summary: Summary,
    pub names: Vec<String>,
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
    pub soft_object_paths: Vec<String>,
    pub soft_package_references: Vec<String>,
    pub asset_registry: Vec<AssetRegistryEntry>,
    pub metadata: Option<PackageMetaData>,
    pub warnings: Vec<String>,
    /// Package path guessed from the file location (used when the package does not store it).
    pub path_hint: Option<String>,
    custom_version_map: HashMap<Guid, i32>,
}

impl Package {
    pub fn ue4(&self) -> i32 {
        self.summary.file_version_ue4
    }

    pub fn ue5(&self) -> i32 {
        self.summary.file_version_ue5
    }

    /// Returns the version of a custom version GUID; unknown versions are treated as
    /// "latest" for unversioned packages and as 0 (oldest) otherwise.
    pub fn custom_ver(&self, guid: Guid) -> i32 {
        match self.custom_version_map.get(&guid) {
            Some(v) => *v,
            None if self.summary.unversioned => i32::MAX,
            None => -1,
        }
    }

    pub fn is_filter_editor_only(&self) -> bool {
        self.summary.package_flags & PKG_FILTER_EDITOR_ONLY != 0
    }

    pub fn uses_unversioned_properties(&self) -> bool {
        self.summary.package_flags & PKG_UNVERSIONED_PROPERTIES != 0
    }

    /// Large world coordinates: FVector & friends are doubles.
    pub fn lwc(&self) -> bool {
        self.ue5() >= ue5::LARGE_WORLD_COORDINATES
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Package> {
        let mut pkg = Package {
            data,
            summary: Summary::default(),
            names: vec![],
            imports: vec![],
            exports: vec![],
            soft_object_paths: vec![],
            soft_package_references: vec![],
            asset_registry: vec![],
            metadata: None,
            warnings: vec![],
            path_hint: None,
            custom_version_map: HashMap::new(),
        };
        pkg.summary = pkg.read_summary()?;
        pkg.custom_version_map = pkg.summary.custom_versions.iter().cloned().collect();
        pkg.names = pkg.read_names()?;
        pkg.imports = pkg.read_imports()?;
        pkg.exports = pkg.read_exports()?;
        match pkg.read_soft_object_paths() {
            Ok(v) => pkg.soft_object_paths = v,
            Err(e) => pkg.warnings.push(format!("soft object path list: {e}")),
        }
        match pkg.read_soft_package_references() {
            Ok(v) => pkg.soft_package_references = v,
            Err(e) => pkg.warnings.push(format!("soft package references: {e}")),
        }
        match pkg.read_asset_registry() {
            Ok(v) => pkg.asset_registry = v,
            Err(e) => pkg.warnings.push(format!("asset registry data: {e}")),
        }
        match pkg.read_metadata() {
            Ok(v) => pkg.metadata = v,
            Err(e) => pkg.warnings.push(format!("package metadata: {e}")),
        }
        Ok(pkg)
    }

    pub fn reader(&self) -> Reader<'_> {
        Reader::new(&self.data)
    }

    fn read_engine_version(r: &mut Reader) -> Result<EngineVersion> {
        Ok(EngineVersion {
            major: r.u16()?,
            minor: r.u16()?,
            patch: r.u16()?,
            changelist: r.u32()?,
            branch: r.fstring()?,
        })
    }

    fn read_summary(&self) -> Result<Summary> {
        let mut r = self.reader();
        let mut s = Summary::default();
        let tag = r.u32().map_err(|_| ParseError::InvalidTag)?;
        if tag == PACKAGE_FILE_TAG_SWAPPED {
            return Err(ParseError::Unsupported("big-endian packages are not supported".into()));
        }
        if tag != PACKAGE_FILE_TAG {
            return Err(ParseError::InvalidTag);
        }
        s.legacy_file_version = r.i32()?;
        if s.legacy_file_version >= 0 || s.legacy_file_version < -9 {
            return Err(ParseError::UnsupportedLegacyVersion(s.legacy_file_version));
        }
        if s.legacy_file_version != -4 {
            r.i32()?; // LegacyUE3Version
        }
        s.file_version_ue4 = r.i32()?;
        if s.legacy_file_version <= -8 {
            s.file_version_ue5 = r.i32()?;
        }
        s.file_version_licensee = r.i32()?;
        s.unversioned = s.file_version_ue4 == 0 && s.file_version_ue5 == 0 && s.file_version_licensee == 0;
        if s.unversioned {
            s.file_version_ue4 = ue4::CORRECT_LICENSEE_FLAG;
            s.file_version_ue5 = ue5::LATEST;
        }
        if s.file_version_ue4 < ue4::OLDEST_LOADABLE_PACKAGE {
            return Err(ParseError::Unsupported(format!(
                "package file version {} is too old",
                s.file_version_ue4
            )));
        }
        if s.file_version_ue5 > ue5::LATEST {
            // keep going: format changes after this point are usually additive
        }
        let ue5v = s.file_version_ue5;
        let ue4v = s.file_version_ue4;

        if ue5v >= ue5::PACKAGE_SAVED_HASH {
            s.saved_hash = r.bytes(20)?.to_vec();
            s.total_header_size = r.i32()?;
        }
        if s.legacy_file_version <= -2 {
            let n = r.count(20)?;
            for _ in 0..n {
                if s.legacy_file_version == -2 {
                    // enum-based custom versions
                    let tag = r.u32()?;
                    let ver = r.i32()?;
                    s.custom_versions.push((Guid::from_u32s(0, 0, 0, tag), ver));
                } else if s.legacy_file_version >= -5 {
                    let g = r.guid()?;
                    let ver = r.i32()?;
                    r.fstring()?;
                    s.custom_versions.push((g, ver));
                } else {
                    let g = r.guid()?;
                    let ver = r.i32()?;
                    s.custom_versions.push((g, ver));
                }
            }
        }
        if ue5v < ue5::PACKAGE_SAVED_HASH {
            s.total_header_size = r.i32()?;
        }
        s.package_name = r.fstring()?;
        s.package_flags = r.u32()?;
        let filter_editor_only = s.package_flags & PKG_FILTER_EDITOR_ONLY != 0;
        s.name_count = r.i32()?;
        s.name_offset = r.i32()?;
        if ue5v >= ue5::ADD_SOFTOBJECTPATH_LIST {
            s.soft_object_paths_count = r.i32()?;
            s.soft_object_paths_offset = r.i32()?;
        }
        if !filter_editor_only && ue4v >= ue4::ADDED_PACKAGE_SUMMARY_LOCALIZATION_ID {
            s.localization_id = r.fstring()?;
        }
        if ue4v >= ue4::SERIALIZE_TEXT_IN_PACKAGES {
            s.gatherable_text_data_count = r.i32()?;
            s.gatherable_text_data_offset = r.i32()?;
        }
        s.export_count = r.i32()?;
        s.export_offset = r.i32()?;
        s.import_count = r.i32()?;
        s.import_offset = r.i32()?;
        if ue5v >= ue5::VERSE_CELLS {
            s.cell_export_count = r.i32()?;
            s.cell_export_offset = r.i32()?;
            s.cell_import_count = r.i32()?;
            s.cell_import_offset = r.i32()?;
        }
        if ue5v >= ue5::METADATA_SERIALIZATION_OFFSET {
            s.metadata_offset = r.i32()?;
        }
        s.depends_offset = r.i32()?;
        if ue4v >= ue4::ADD_STRING_ASSET_REFERENCES_MAP {
            s.soft_package_references_count = r.i32()?;
            s.soft_package_references_offset = r.i32()?;
        }
        if ue4v >= ue4::ADDED_SEARCHABLE_NAMES {
            s.searchable_names_offset = r.i32()?;
        }
        s.thumbnail_table_offset = r.i32()?;
        if ue5v >= ue5::IMPORT_TYPE_HIERARCHIES {
            r.i32()?; // ImportTypeHierarchiesCount
            r.i32()?; // ImportTypeHierarchiesOffset
        }
        if ue5v < ue5::PACKAGE_SAVED_HASH {
            s.saved_hash = r.bytes(16)?.to_vec(); // legacy package guid
        }
        if !filter_editor_only {
            if ue4v >= ue4::ADDED_PACKAGE_OWNER {
                s.persistent_guid = Some(r.guid()?);
            }
            if ue4v >= ue4::ADDED_PACKAGE_OWNER && ue4v < ue4::NON_OUTER_PACKAGE_IMPORT {
                r.guid()?; // OwnerPersistentGuid
            }
        }
        let generations = r.count(8)?;
        r.skip(generations * 8)?;
        if ue4v >= ue4::ENGINE_VERSION_OBJECT {
            s.saved_by = Self::read_engine_version(&mut r)?;
        } else {
            let cl = r.u32()?;
            s.saved_by = EngineVersion { major: 4, changelist: cl, ..Default::default() };
        }
        if ue4v >= ue4::PACKAGE_SUMMARY_HAS_COMPATIBLE_ENGINE_VERSION {
            s.compatible_with = Self::read_engine_version(&mut r)?;
        } else {
            s.compatible_with = s.saved_by.clone();
        }
        s.compression_flags = r.u32()?;
        let chunks = r.count(16)?;
        if chunks > 0 {
            return Err(ParseError::Unsupported("package-level compression is not supported".into()));
        }
        s.package_source = r.u32()?;
        let additional = r.count(4)?;
        for _ in 0..additional {
            r.fstring()?;
        }
        if s.legacy_file_version > -7 {
            r.i32()?; // NumTextureAllocations
        }
        s.asset_registry_data_offset = r.i32()?;
        s.bulk_data_start_offset = r.i64()?;
        if ue4v >= ue4::WORLD_LEVEL_INFO {
            s.world_tile_info_data_offset = r.i32()?;
        }
        if ue4v >= ue4::CHANGED_CHUNKID_TO_BE_AN_ARRAY_OF_CHUNKIDS {
            let n = r.count(4)?;
            for _ in 0..n {
                s.chunk_ids.push(r.i32()?);
            }
        } else if ue4v >= ue4::ADDED_CHUNKID_TO_ASSETDATA_AND_UPACKAGE {
            let id = r.i32()?;
            if id >= 0 {
                s.chunk_ids.push(id);
            }
        }
        if ue4v >= ue4::PRELOAD_DEPENDENCIES_IN_COOKED_EXPORTS {
            s.preload_dependency_count = r.i32()?;
            s.preload_dependency_offset = r.i32()?;
        } else {
            s.preload_dependency_count = -1;
        }
        s.names_referenced_from_export_data_count = s.name_count;
        if ue5v >= ue5::NAMES_REFERENCED_FROM_EXPORT_DATA {
            s.names_referenced_from_export_data_count = r.i32()?;
        }
        s.payload_toc_offset = -1;
        if ue5v >= ue5::PAYLOAD_TOC {
            s.payload_toc_offset = r.i64()?;
        }
        s.data_resource_offset = -1;
        if ue5v >= ue5::DATA_RESOURCES {
            s.data_resource_offset = r.i32()?;
        }
        Ok(s)
    }

    fn check_offset(&self, what: &str, offset: i64) -> Result<usize> {
        if offset < 0 || offset as usize > self.data.len() {
            return invalid(format!("{what} offset {offset} is outside of file ({} bytes)", self.data.len()));
        }
        Ok(offset as usize)
    }

    fn read_names(&self) -> Result<Vec<String>> {
        let s = &self.summary;
        let mut r = self.reader();
        r.seek(self.check_offset("name table", s.name_offset as i64)?)?;
        if s.name_count < 0 {
            return invalid("negative name count");
        }
        let mut names = Vec::with_capacity(s.name_count as usize);
        for _ in 0..s.name_count {
            names.push(r.fstring()?);
            if s.file_version_ue4 >= ue4::NAME_HASHES_SERIALIZED {
                r.skip(4)?;
            }
        }
        Ok(names)
    }

    pub fn name_at(&self, index: i32, number: i32) -> Result<String> {
        let Some(base) = (index >= 0).then(|| self.names.get(index as usize)).flatten() else {
            return invalid(format!("name index {index} out of range ({} names)", self.names.len()));
        };
        if number > 0 {
            Ok(format!("{}_{}", base, number - 1))
        } else {
            Ok(base.clone())
        }
    }

    pub fn read_fname(&self, r: &mut Reader) -> Result<String> {
        let index = r.i32()?;
        let number = r.i32()?;
        self.name_at(index, number)
    }

    fn read_imports(&self) -> Result<Vec<Import>> {
        let s = &self.summary;
        let mut r = self.reader();
        r.seek(self.check_offset("import table", s.import_offset as i64)?)?;
        let mut v = Vec::with_capacity(s.import_count.max(0) as usize);
        for _ in 0..s.import_count {
            let class_package = self.read_fname(&mut r)?;
            let class_name = self.read_fname(&mut r)?;
            let outer_index = r.i32()?;
            let object_name = self.read_fname(&mut r)?;
            let mut package_name = String::from("None");
            if s.file_version_ue4 >= ue4::NON_OUTER_PACKAGE_IMPORT && !self.is_filter_editor_only() {
                package_name = self.read_fname(&mut r)?;
            } else if s.file_version_ue4 >= ue4::NON_OUTER_PACKAGE_IMPORT {
                package_name = self.read_fname(&mut r)?;
            }
            let mut optional = false;
            if s.file_version_ue5 >= ue5::OPTIONAL_RESOURCES {
                optional = r.bool32()?;
            }
            v.push(Import { class_package, class_name, outer_index, object_name, package_name, optional });
        }
        Ok(v)
    }

    fn read_exports(&self) -> Result<Vec<Export>> {
        let s = &self.summary;
        let ue4v = s.file_version_ue4;
        let ue5v = s.file_version_ue5;
        let mut r = self.reader();
        r.seek(self.check_offset("export table", s.export_offset as i64)?)?;
        let mut v = Vec::with_capacity(s.export_count.max(0) as usize);
        for _ in 0..s.export_count {
            let class_index = r.i32()?;
            let super_index = r.i32()?;
            let template_index = if ue4v >= ue4::TEMPLATE_INDEX_IN_COOKED_EXPORTS { r.i32()? } else { 0 };
            let outer_index = r.i32()?;
            let object_name = self.read_fname(&mut r)?;
            let object_flags = r.u32()?;
            let (serial_size, serial_offset) = if ue4v < ue4::EXPORTMAP_64BIT_SERIALSIZES {
                (r.i32()? as i64, r.i32()? as i64)
            } else {
                (r.i64()?, r.i64()?)
            };
            let forced_export = r.bool32()?;
            let not_for_client = r.bool32()?;
            let not_for_server = r.bool32()?;
            if ue5v < ue5::REMOVE_OBJECT_EXPORT_PACKAGE_GUID {
                r.guid()?;
            }
            let is_inherited_instance = if ue5v >= ue5::TRACK_OBJECT_EXPORT_IS_INHERITED { r.bool32()? } else { false };
            let package_flags = r.u32()?;
            let not_always_loaded_for_editor_game = if ue4v >= ue4::LOAD_FOR_EDITOR_GAME { r.bool32()? } else { false };
            let is_asset = if ue4v >= ue4::COOKED_ASSETS_IN_EDITOR_SUPPORT { r.bool32()? } else { false };
            let generate_public_hash = if ue5v >= ue5::OPTIONAL_RESOURCES { r.bool32()? } else { false };
            if ue4v >= ue4::PRELOAD_DEPENDENCIES_IN_COOKED_EXPORTS {
                r.skip(4 * 5)?;
            }
            let (mut ss, mut se) = (0, 0);
            if !self.uses_unversioned_properties() && ue5v >= ue5::SCRIPT_SERIALIZATION_OFFSET {
                ss = r.i64()?;
                se = r.i64()?;
            }
            if serial_offset < 0 || serial_size < 0 || (serial_offset + serial_size) as usize > self.data.len() {
                return invalid(format!("export '{object_name}' has invalid serial range {serial_offset}+{serial_size}"));
            }
            v.push(Export {
                class_index,
                super_index,
                template_index,
                outer_index,
                object_name,
                object_flags,
                serial_size,
                serial_offset,
                forced_export,
                not_for_client,
                not_for_server,
                is_inherited_instance,
                package_flags,
                not_always_loaded_for_editor_game,
                is_asset,
                generate_public_hash,
                script_serialization_start_offset: ss,
                script_serialization_end_offset: se,
            });
        }
        Ok(v)
    }

    /// Reads an FSoftObjectPath in its raw (non-indexed) form.
    pub fn read_soft_object_path_raw(&self, r: &mut Reader) -> Result<String> {
        if self.ue4() < ue4::ADDED_SOFT_OBJECT_PATH {
            return r.fstring();
        }
        let asset = if self.ue5() < ue5::FSOFTOBJECTPATH_REMOVE_ASSET_PATH_FNAMES {
            self.read_fname(r)?
        } else {
            let pkg = self.read_fname(r)?;
            let asset = self.read_fname(r)?;
            if pkg == "None" {
                "None".to_string()
            } else if asset == "None" {
                pkg
            } else {
                format!("{pkg}.{asset}")
            }
        };
        let sub = r.fstring()?;
        Ok(if sub.is_empty() { asset } else { format!("{asset}:{sub}") })
    }

    /// Reads an FSoftObjectPath as stored in export data (index into the package list when present).
    pub fn read_soft_object_path(&self, r: &mut Reader) -> Result<String> {
        if !self.soft_object_paths.is_empty() {
            let idx = r.i32()?;
            return match self.soft_object_paths.get(idx as usize) {
                Some(p) if idx >= 0 => Ok(p.clone()),
                _ => invalid(format!("soft object path index {idx} out of range")),
            };
        }
        self.read_soft_object_path_raw(r)
    }

    fn read_soft_object_paths(&self) -> Result<Vec<String>> {
        let s = &self.summary;
        if s.soft_object_paths_count <= 0 || s.soft_object_paths_offset <= 0 {
            return Ok(vec![]);
        }
        let mut r = self.reader();
        r.seek(self.check_offset("soft object paths", s.soft_object_paths_offset as i64)?)?;
        (0..s.soft_object_paths_count).map(|_| self.read_soft_object_path_raw(&mut r)).collect()
    }

    fn read_soft_package_references(&self) -> Result<Vec<String>> {
        let s = &self.summary;
        if s.soft_package_references_count <= 0 || s.soft_package_references_offset <= 0 {
            return Ok(vec![]);
        }
        let mut r = self.reader();
        r.seek(self.check_offset("soft package references", s.soft_package_references_offset as i64)?)?;
        (0..s.soft_package_references_count)
            .map(|_| if s.file_version_ue4 < ue4::ADDED_SOFT_OBJECT_PATH { r.fstring() } else { self.read_fname(&mut r) })
            .collect()
    }

    fn read_asset_registry(&self) -> Result<Vec<AssetRegistryEntry>> {
        let s = &self.summary;
        if s.asset_registry_data_offset <= 0 {
            return Ok(vec![]);
        }
        let mut r = self.reader();
        r.seek(self.check_offset("asset registry", s.asset_registry_data_offset as i64)?)?;
        let pre_dependency_format =
            s.file_version_ue4 < ue4::ASSETREGISTRY_DEPENDENCYFLAGS || self.is_filter_editor_only();
        if !pre_dependency_format {
            r.i64()?; // DependencyDataOffset
        }
        let n = r.count(1)?;
        let mut out = vec![];
        for _ in 0..n {
            let object_path = r.fstring()?;
            let object_class = r.fstring()?;
            let tag_count = r.count(8)?;
            let mut tags = vec![];
            for _ in 0..tag_count {
                let k = r.fstring()?;
                let v = r.fstring()?;
                tags.push((k, v));
            }
            out.push(AssetRegistryEntry { object_path, object_class, tags });
        }
        Ok(out)
    }

    fn read_string_map(&self, r: &mut Reader) -> Result<Vec<(String, String)>> {
        let n = r.count(12)?;
        let mut m = Vec::with_capacity(n);
        for _ in 0..n {
            let k = self.read_fname(r)?;
            let v = r.fstring()?;
            m.push((k, v));
        }
        Ok(m)
    }

    fn read_metadata(&self) -> Result<Option<PackageMetaData>> {
        let s = &self.summary;
        if s.metadata_offset <= 0 {
            return Ok(None);
        }
        let mut r = self.reader();
        r.seek(self.check_offset("metadata", s.metadata_offset as i64)?)?;
        let n_obj = r.i32()?;
        let n_root = r.i32()?;
        if n_obj < 0 || n_root < 0 {
            return invalid("negative metadata counts");
        }
        let mut md = PackageMetaData::default();
        for _ in 0..n_obj {
            let path = self.read_soft_object_path(&mut r)?;
            let map = self.read_string_map(&mut r)?;
            md.objects.push((path, map));
        }
        for _ in 0..n_root {
            let k = self.read_fname(&mut r)?;
            let v = r.fstring()?;
            md.root.push((k, v));
        }
        Ok(Some(md))
    }

    // ---------------------------------------------------------------------------------------------
    // Object reference helpers

    pub fn import(&self, index: i32) -> Option<&Import> {
        if index < 0 { self.imports.get((-index - 1) as usize) } else { None }
    }

    pub fn export(&self, index: i32) -> Option<&Export> {
        if index > 0 { self.exports.get((index - 1) as usize) } else { None }
    }

    pub fn is_valid_index(&self, index: i32) -> bool {
        index == 0 || self.import(index).is_some() || self.export(index).is_some()
    }

    pub fn obj_name(&self, index: i32) -> String {
        if let Some(i) = self.import(index) {
            i.object_name.clone()
        } else if let Some(e) = self.export(index) {
            e.object_name.clone()
        } else {
            "None".into()
        }
    }

    pub fn outer_of(&self, index: i32) -> i32 {
        if let Some(i) = self.import(index) {
            i.outer_index
        } else if let Some(e) = self.export(index) {
            e.outer_index
        } else {
            0
        }
    }

    /// Class name (short) of the referenced object.
    pub fn class_name(&self, index: i32) -> String {
        if let Some(i) = self.import(index) {
            i.class_name.clone()
        } else if let Some(e) = self.export(index) {
            if e.class_index == 0 { "Class".into() } else { self.obj_name(e.class_index) }
        } else {
            "None".into()
        }
    }

    /// Full path of an object: "/Script/Engine.Actor", "/Game/Foo.Foo:Sub.Sub2".
    pub fn obj_path(&self, index: i32) -> String {
        if index == 0 {
            return "None".into();
        }
        let mut chain = vec![];
        let mut cur = index;
        let mut guard = 0;
        while cur != 0 && guard < 64 {
            if !self.is_valid_index(cur) {
                chain.push(format!("<bad index {cur}>"));
                break;
            }
            chain.push(self.obj_name(cur));
            cur = self.outer_of(cur);
            guard += 1;
        }
        chain.reverse();
        let is_export = index > 0;
        let mut out = String::new();
        if is_export {
            out.push_str(&self.package_path());
            out.push('.');
        }
        for (i, part) in chain.iter().enumerate() {
            if i > 0 {
                let sep = if is_export {
                    if i == 1 { ':' } else { '.' }
                } else if i == 1 {
                    '.'
                } else if i == 2 {
                    ':'
                } else {
                    '.'
                };
                out.push(sep);
            }
            out.push_str(part);
        }
        out
    }

    /// Path of an export relative to the package: "BP_Foo:EventGraph.K2Node_Event_0".
    pub fn export_rel_path(&self, index: i32) -> String {
        let full = self.obj_path(index);
        let prefix = format!("{}.", self.package_path());
        full.strip_prefix(&prefix).map(|s| s.to_string()).unwrap_or(full)
    }

    /// Long package name, e.g. "/Game/Foo/BP_Bar". Derived from imports/registry when the
    /// summary does not store it.
    pub fn package_path(&self) -> String {
        if self.summary.package_name.starts_with('/') && self.summary.package_name != "None" {
            return self.summary.package_name.clone();
        }
        for e in &self.asset_registry {
            if e.object_path.starts_with('/') {
                if let Some((p, _)) = e.object_path.split_once('.') {
                    return p.to_string();
                }
            }
        }
        // Registry tags of blueprints reference their own package: "...'/Game/X/BP.BP_C'"
        let top: Vec<&str> = self.exports.iter().filter(|e| e.outer_index == 0).map(|e| e.object_name.as_str()).collect();
        for e in &self.asset_registry {
            for (_, v) in &e.tags {
                for part in v.split('\'') {
                    if let Some((pkg, obj)) = part.split_once('.') {
                        if pkg.starts_with('/') && top.contains(&obj) && pkg.rsplit('/').next().map(|n| obj.starts_with(n)).unwrap_or(false) {
                            return pkg.to_string();
                        }
                    }
                }
            }
        }
        // A soft reference to the package itself
        for r in &self.soft_package_references {
            if let Some(name) = r.rsplit('/').next() {
                if r.starts_with('/') && top.contains(&name) {
                    return r.clone();
                }
            }
        }
        self.path_hint.clone().unwrap_or_default()
    }

    /// Short reference string used in output: imports as full path, exports as relative path.
    pub fn ref_string(&self, index: i32) -> String {
        if index == 0 {
            "None".into()
        } else if index < 0 {
            self.obj_path(index)
        } else {
            self.export_rel_path(index)
        }
    }

    pub fn export_data(&self, export_index: usize) -> &[u8] {
        let e = &self.exports[export_index];
        &self.data[e.serial_offset as usize..(e.serial_offset + e.serial_size) as usize]
    }
}
