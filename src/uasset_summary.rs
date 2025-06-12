use std::io::{Read, Seek};

use crate::uasset_parser::{Parsable, UassetParser};

#[derive(Debug, Default)]
pub struct UassetSummary {
    pub tag: u32,
    pub legacy_file_version: i32,
    pub legacy_ue3_version: i32,
    pub file_version_ue4: i32,
    pub file_version_ue5: i32,
    pub file_version_licensee_ue4: u32,
    pub saved_hash: Option<[u8; 20]>,
    pub total_header_size: i32,
    pub custom_versions: Vec<[u8; 20]>,
    pub package_name: String,
    pub package_flags: u32,
    pub name_count: i32,
    pub name_offset: i32,
    pub soft_object_paths_count: Option<i32>,
    pub soft_object_paths_offset: Option<i32>,
    pub localization_id: String,
    pub gatherable_text_data_count: i32,
    pub gatherable_text_data_offset: i32,
    pub export_count: i32,
    pub export_offset: i32,
    pub import_count: i32,
    pub import_offset: i32,
    pub cell_export_count: Option<i32>,
    pub cell_export_offset: Option<i32>,
    pub cell_import_count: Option<i32>,
    pub cell_import_offset: Option<i32>,
    pub metadata_offset: Option<i32>,
    pub depends_offset: i32,
    pub soft_package_references_count: i32,
    pub soft_package_references_offset: i32,
    pub searchable_names_offset: i32,
    pub thumbnail_table_offset: i32,
    pub guid: Option<[u8; 16]>,
    pub persistent_guid: [u8; 16],
    pub generations: Vec<[u8; 8]>,
    pub saved_by_engine_version_major: u16,
    pub saved_by_engine_version_minor: u16,
    pub saved_by_engine_version_patch: u16,
    pub saved_by_engine_version_changelist: u32,
    pub saved_by_engine_version_name: String,
    pub compatible_engine_version_major: u16,
    pub compatible_engine_version_minor: u16,
    pub compatible_engine_version_patch: u16,
    pub compatible_engine_version_changelist: u32,
    pub compatible_engine_version_name: String,
    pub compression_flags: u32,
    pub compressed_chunks: Vec<[u8; 16]>,
    pub package_source: u32,
    pub additional_packages_to_cook: Vec<String>,
    pub asset_registry_data_offset: i32,
    pub bulk_data_start_offset: i64,
}

impl<R: Read + Seek> Parsable<UassetSummary> for UassetParser<R> {
    fn parse(&mut self) -> std::io::Result<UassetSummary> {
        todo!()
    }
}
