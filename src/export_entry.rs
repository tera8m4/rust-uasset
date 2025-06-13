use crate::errors::Result;
use crate::fname::FName;
use crate::uasset_parser::{Parsable, UassetParser};
use crate::versions::EUnrealEngineObjectUE5Version;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Read, Seek};

#[derive(Debug)]
pub struct ExportEntry {
    pub class_index: i32,
    pub super_index: i32,
    pub template_index: i32,
    pub outer_index: i32,
    pub object_name: FName,
    pub object_flags: i32,
    pub serial_size: i64,
    pub serial_offset: i64,
    pub force_export: bool,
    pub not_for_client: bool,
    pub not_for_server: bool,
    pub is_inherited_instance: bool,
    pub package_flags: u32,
    pub not_always_loaded_for_editor_game: bool,
    pub is_asset: bool,
    pub generate_public_hash: bool,
    pub first_export_dependency: i32,
    pub serialization_before_serialization_dependencies: i32,
    pub create_before_serialization_dependencies: i32,
    pub serialization_before_create_dependencies: i32,
    pub create_before_create_dependencies: i32,
    pub script_serialization_start_offset: i64,
    pub script_serialization_end_offset: i64,
}

impl<R: Read + Seek> Parsable<ExportEntry> for UassetParser<R> {
    fn parse(&mut self) -> Result<ExportEntry> {
        let class_index = self.reader.read_i32::<LittleEndian>()?;
        let super_index = self.reader.read_i32::<LittleEndian>()?;
        let template_index = self.reader.read_i32::<LittleEndian>()?;
        let outer_index = self.reader.read_i32::<LittleEndian>()?;
        let object_name: FName = self.read()?;
        let object_flags: i32 = self.reader.read_i32::<LittleEndian>()?;
        let serial_size: i64 = self.reader.read_i64::<LittleEndian>()?;
        let serial_offset: i64 = self.reader.read_i64::<LittleEndian>()?;

        let force_export = self.reader.read_u32::<LittleEndian>()? != 0;
        let not_for_client = self.reader.read_u32::<LittleEndian>()? != 0;
        let not_for_server = self.reader.read_u32::<LittleEndian>()? != 0;

        if self.get_summary().file_version_ue5
            < EUnrealEngineObjectUE5Version::RemoveObjectExportPackageGuid as i32
        {
            self.reader.read_i128::<LittleEndian>()?;
        }

        let is_inherited_instance = if self.get_summary().file_version_ue5
            > EUnrealEngineObjectUE5Version::TrackObjectExportIsInherited as i32
        {
            self.reader.read_u32::<LittleEndian>()? != 0
        } else {
            false
        };

        let package_flags = self.reader.read_u32::<LittleEndian>()?;
        let not_always_loaded_for_editor_game = self.reader.read_u32::<LittleEndian>()? != 0;
        let is_asset = self.reader.read_u32::<LittleEndian>()? != 0;

        let generate_public_hash = if self.get_summary().file_version_ue5
            >= EUnrealEngineObjectUE5Version::OptionalResources as i32
        {
            self.reader.read_u32::<LittleEndian>()? != 0
        } else {
            false
        };

        let first_export_dependency = self.reader.read_i32::<LittleEndian>()?;
        let serialization_before_serialization_dependencies =
            self.reader.read_i32::<LittleEndian>()?;
        let create_before_serialization_dependencies = self.reader.read_i32::<LittleEndian>()?;
        let serialization_before_create_dependencies = self.reader.read_i32::<LittleEndian>()?;
        let create_before_create_dependencies = self.reader.read_i32::<LittleEndian>()?;

        let script_serialization_start_offset = self.reader.read_i64::<LittleEndian>()?;
        let script_serialization_end_offset = self.reader.read_i64::<LittleEndian>()?;

        let export = ExportEntry {
            class_index,
            super_index,
            template_index,
            outer_index,
            object_name,
            object_flags,
            serial_size,
            serial_offset,
            force_export,
            not_for_client,
            not_for_server,
            is_inherited_instance,
            package_flags,
            not_always_loaded_for_editor_game,
            is_asset,
            generate_public_hash,
            first_export_dependency,
            serialization_before_serialization_dependencies,
            create_before_serialization_dependencies,
            serialization_before_create_dependencies,
            create_before_create_dependencies,
            script_serialization_start_offset,
            script_serialization_end_offset,
        };

        Ok(export)
    }
}
