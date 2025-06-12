use crate::errors::{ParseError, Result};
use crate::uasset_parser::{Parsable, UassetParser};
use crate::versions::EUnrealEngineObjectUE5Version;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Read, Seek, SeekFrom};

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
    fn parse(&mut self) -> Result<UassetSummary> {
        self.reader.seek(SeekFrom::Start(0))?;

        let mut s = UassetSummary::default();

        s.tag = self.reader.read_u32::<LittleEndian>()?;

        if s.tag != 0x9e2a83c1 {
            return Err(ParseError::InvalidTag);
        }

        s.legacy_file_version = self.reader.read_i32::<LittleEndian>()?;

        if ![-7, -8, -9].contains(&s.legacy_file_version) {
            return Err(ParseError::UnsupportedLegacyVersion(s.legacy_file_version));
        }

        s.legacy_ue3_version = self.reader.read_i32::<LittleEndian>()?;
        s.file_version_ue4 = self.reader.read_i32::<LittleEndian>()?;

        if s.legacy_file_version <= -8 {
            s.file_version_ue5 = self.reader.read_i32::<LittleEndian>()?;
        } else {
            s.file_version_ue5 = 0;
        }

        s.file_version_licensee_ue4 = self.reader.read_u32::<LittleEndian>()?;

        const KNOWN_SUPPORTED_UE5VER: i32 = 1017;
        if s.file_version_ue5 > KNOWN_SUPPORTED_UE5VER {
            eprintln!(
                "Warning: ObjectUE5Version {} too new; newest known supported version {}",
                s.file_version_ue5, KNOWN_SUPPORTED_UE5VER
            );
            eprintln!("Parsing will attempt to continue, but there may be errors reading the file");
        }

        if s.file_version_ue5 >= EUnrealEngineObjectUE5Version::PackageSavedHash as i32 {
            let mut hash = [0u8; 20];
            self.reader.read_exact(&mut hash)?;
            s.saved_hash = Some(hash);
            s.total_header_size = self.reader.read_i32::<LittleEndian>()?;
        }

        s.custom_versions = self.read_tarray(
            |parser| {
                let mut buf = [0u8; 20];
                parser.reader.read_exact(&mut buf)?;
                Ok(buf)
            },
            100000,
        )?;

        if s.file_version_ue5 < EUnrealEngineObjectUE5Version::PackageSavedHash as i32 {
            s.total_header_size = self.reader.read_i32::<LittleEndian>()?;
        }

        s.package_name = self.read_fstring()?;
        s.package_flags = self.reader.read_u32::<LittleEndian>()?;
        s.name_count = self.reader.read_i32::<LittleEndian>()?;
        s.name_offset = self.reader.read_i32::<LittleEndian>()?;

        if s.file_version_ue5 >= EUnrealEngineObjectUE5Version::AddSoftObjectPathList as i32 {
            s.soft_object_paths_count = Some(self.reader.read_i32::<LittleEndian>()?);
            s.soft_object_paths_offset = Some(self.reader.read_i32::<LittleEndian>()?);
        }

        s.localization_id = self.read_fstring()?;

        s.gatherable_text_data_count = self.reader.read_i32::<LittleEndian>()?;
        s.gatherable_text_data_offset = self.reader.read_i32::<LittleEndian>()?;
        s.export_count = self.reader.read_i32::<LittleEndian>()?;
        s.export_offset = self.reader.read_i32::<LittleEndian>()?;
        s.import_count = self.reader.read_i32::<LittleEndian>()?;
        s.import_offset = self.reader.read_i32::<LittleEndian>()?;

        if s.file_version_ue5 >= EUnrealEngineObjectUE5Version::VerseCells as i32 {
            s.cell_export_count = Some(self.reader.read_i32::<LittleEndian>()?);
            s.cell_export_offset = Some(self.reader.read_i32::<LittleEndian>()?);
            s.cell_import_count = Some(self.reader.read_i32::<LittleEndian>()?);
            s.cell_import_offset = Some(self.reader.read_i32::<LittleEndian>()?);
        }

        if s.file_version_ue5 >= EUnrealEngineObjectUE5Version::MetadataSerializationOffset as i32 {
            s.metadata_offset = Some(self.reader.read_i32::<LittleEndian>()?);
        }

        s.depends_offset = self.reader.read_i32::<LittleEndian>()?;
        s.soft_package_references_count = self.reader.read_i32::<LittleEndian>()?;
        s.soft_package_references_offset = self.reader.read_i32::<LittleEndian>()?;
        s.searchable_names_offset = self.reader.read_i32::<LittleEndian>()?;
        s.thumbnail_table_offset = self.reader.read_i32::<LittleEndian>()?;

        if s.file_version_ue5 < EUnrealEngineObjectUE5Version::PackageSavedHash as i32 {
            let mut guid = [0u8; 16];
            self.reader.read_exact(&mut guid)?;
            s.guid = Some(guid);
        }

        let mut persistent_guid = [0u8; 16];
        self.reader.read_exact(&mut persistent_guid)?;
        s.persistent_guid = persistent_guid;

        self.check_file_offset(s.gatherable_text_data_offset as i64)?;
        self.check_file_offset(s.export_offset as i64)?;
        self.check_file_offset(s.import_offset as i64)?;
        self.check_file_offset(s.depends_offset as i64)?;
        self.check_file_offset(s.soft_package_references_offset as i64)?;
        self.check_file_offset(s.searchable_names_offset as i64)?;
        self.check_file_offset(s.thumbnail_table_offset as i64)?;

        let current_pos = self.reader.stream_position()?;
        let remaining_bytes = (s.total_header_size as u64).saturating_sub(current_pos + 1);
        let max_generations = (remaining_bytes / 20) as usize;

        s.generations = self.read_tarray(
            |parser| {
                let mut buf = [0u8; 8];
                parser.reader.read_exact(&mut buf)?;
                Ok(buf)
            },
            max_generations,
        )?;

        s.saved_by_engine_version_major = self.reader.read_u16::<LittleEndian>()?;
        s.saved_by_engine_version_minor = self.reader.read_u16::<LittleEndian>()?;
        s.saved_by_engine_version_patch = self.reader.read_u16::<LittleEndian>()?;
        s.saved_by_engine_version_changelist = self.reader.read_u32::<LittleEndian>()?;
        s.saved_by_engine_version_name = self.read_fstring()?;

        s.compatible_engine_version_major = self.reader.read_u16::<LittleEndian>()?;
        s.compatible_engine_version_minor = self.reader.read_u16::<LittleEndian>()?;
        s.compatible_engine_version_patch = self.reader.read_u16::<LittleEndian>()?;
        s.compatible_engine_version_changelist = self.reader.read_u32::<LittleEndian>()?;
        s.compatible_engine_version_name = self.read_fstring()?;

        self.check_asset_version(
            s.saved_by_engine_version_major,
            s.saved_by_engine_version_minor,
            s.saved_by_engine_version_patch,
        )?;

        s.compression_flags = self.reader.read_u32::<LittleEndian>()?;
        self.check_compression_flags(s.compression_flags)?;

        let current_pos = self.reader.stream_position()?;
        let remaining_bytes = (s.total_header_size as u64).saturating_sub(current_pos + 1);
        let max_chunks = (remaining_bytes / 16) as usize;

        s.compressed_chunks = self.read_tarray(
            |parser| {
                let mut buf = [0u8; 16];
                parser.reader.read_exact(&mut buf)?;
                Ok(buf)
            },
            max_chunks,
        )?;

        if !s.compressed_chunks.is_empty() {
            return Err(ParseError::CompressedChunksNotSupported);
        }

        s.package_source = self.reader.read_u32::<LittleEndian>()?;

        let current_pos = self.reader.stream_position()?;
        let remaining_bytes = (s.total_header_size as u64).saturating_sub(current_pos + 1);

        s.additional_packages_to_cook =
            self.read_tarray(|parser| parser.read_fstring(), remaining_bytes as usize)?;

        s.asset_registry_data_offset = self.reader.read_i32::<LittleEndian>()?;
        s.bulk_data_start_offset = self.reader.read_i64::<LittleEndian>()?;

        self.check_file_offset(s.asset_registry_data_offset as i64)?;
        self.check_file_offset(s.bulk_data_start_offset)?;

        Ok(s)
    }
}
