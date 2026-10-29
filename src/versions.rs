//! Engine object versions and custom versions referenced by the parser.
//! Values taken from Source/Runtime/Core/Public/UObject/ObjectVersion.h and the
//! various *ObjectVersion.h / *.inl headers of UE 5.8.

use crate::reader::Guid;

pub mod ue4 {
    pub const OLDEST_LOADABLE_PACKAGE: i32 = 214;
    pub const WORLD_LEVEL_INFO: i32 = 224;
    pub const ADDED_CHUNKID_TO_ASSETDATA_AND_UPACKAGE: i32 = 278;
    pub const ARRAY_PROPERTY_INNER_TAGS: i32 = 282;
    pub const CHANGED_CHUNKID_TO_BE_AN_ARRAY_OF_CHUNKIDS: i32 = 326;
    pub const EDGRAPHPINTYPE_SERIALIZATION: i32 = 324;
    pub const ENGINE_VERSION_OBJECT: i32 = 336;
    pub const MEMBERREFERENCE_IN_PINTYPE: i32 = 355;
    pub const LOAD_FOR_EDITOR_GAME: i32 = 365;
    pub const ADDED_NAMESPACE_AND_KEY_DATA_TO_FTEXT: i32 = 226;
    pub const PUBLIC_WORLDS: i32 = 337;
    pub const FTEXT_HISTORY: i32 = 368;
    pub const ADDED_CURRENCY_CODE_TO_FTEXT: i32 = 389;
    pub const ENUM_CLASS_SUPPORT: i32 = 390;
    pub const FTEXT_HISTORY_DATE_TIMEZONE: i32 = 422;
    pub const TIGHTLY_PACKED_ENUMS: i32 = 463;
    pub const ASSET_IMPORT_DATA_AS_JSON: i32 = 464;
    pub const ADD_STRING_ASSET_REFERENCES_MAP: i32 = 384;
    pub const STRUCT_GUID_IN_PROPERTY_TAG: i32 = 441;
    pub const PACKAGE_SUMMARY_HAS_COMPATIBLE_ENGINE_VERSION: i32 = 444;
    pub const SERIALIZE_BLUEPRINT_EVENTGRAPH_FASTCALLS_IN_UFUNCTION: i32 = 451;
    pub const SERIALIZE_PINTYPE_CONST: i32 = 456;
    pub const SERIALIZE_TEXT_IN_PACKAGES: i32 = 459;
    pub const COOKED_ASSETS_IN_EDITOR_SUPPORT: i32 = 485;
    pub const INNER_ARRAY_TAG_INFO: i32 = 500;
    pub const PROPERTY_GUID_IN_PROPERTY_TAG: i32 = 503;
    pub const NAME_HASHES_SERIALIZED: i32 = 504;
    pub const PRELOAD_DEPENDENCIES_IN_COOKED_EXPORTS: i32 = 507;
    pub const TEMPLATE_INDEX_IN_COOKED_EXPORTS: i32 = 508;
    pub const PROPERTY_TAG_SET_MAP_SUPPORT: i32 = 509;
    pub const ADDED_SEARCHABLE_NAMES: i32 = 510;
    pub const EXPORTMAP_64BIT_SERIALSIZES: i32 = 511;
    pub const ADDED_SOFT_OBJECT_PATH: i32 = 514;
    pub const ADDED_PACKAGE_SUMMARY_LOCALIZATION_ID: i32 = 516;
    pub const ADDED_PACKAGE_OWNER: i32 = 518;
    pub const NON_OUTER_PACKAGE_IMPORT: i32 = 520;
    pub const ASSETREGISTRY_DEPENDENCYFLAGS: i32 = 521;
    pub const CORRECT_LICENSEE_FLAG: i32 = 522;
}

pub mod ue5 {
    pub const INITIAL_VERSION: i32 = 1000;
    pub const NAMES_REFERENCED_FROM_EXPORT_DATA: i32 = 1001;
    pub const PAYLOAD_TOC: i32 = 1002;
    pub const OPTIONAL_RESOURCES: i32 = 1003;
    pub const LARGE_WORLD_COORDINATES: i32 = 1004;
    pub const REMOVE_OBJECT_EXPORT_PACKAGE_GUID: i32 = 1005;
    pub const TRACK_OBJECT_EXPORT_IS_INHERITED: i32 = 1006;
    pub const FSOFTOBJECTPATH_REMOVE_ASSET_PATH_FNAMES: i32 = 1007;
    pub const ADD_SOFTOBJECTPATH_LIST: i32 = 1008;
    pub const DATA_RESOURCES: i32 = 1009;
    pub const SCRIPT_SERIALIZATION_OFFSET: i32 = 1010;
    pub const PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION: i32 = 1011;
    pub const PROPERTY_TAG_COMPLETE_TYPE_NAME: i32 = 1012;
    pub const ASSETREGISTRY_PACKAGEBUILDDEPENDENCIES: i32 = 1013;
    pub const METADATA_SERIALIZATION_OFFSET: i32 = 1014;
    pub const VERSE_CELLS: i32 = 1015;
    pub const PACKAGE_SAVED_HASH: i32 = 1016;
    pub const OS_SUB_OBJECT_SHADOW_SERIALIZATION: i32 = 1017;
    pub const IMPORT_TYPE_HIERARCHIES: i32 = 1018;
    /// Newest version this parser knows about.
    pub const LATEST: i32 = IMPORT_TYPE_HIERARCHIES;
}

/// Custom version GUIDs (DevObjectVersion.cpp) and the enum values we branch on.
pub mod custom {
    use super::Guid;

    pub fn blueprints() -> Guid {
        Guid::from_u32s(0xB0D832E4, 0x1F894F0D, 0xACCF7EB7, 0x36FD4AA2)
    }
    pub const BP_EDGRAPH_PIN_OPTIMIZED: i32 = 4;
    pub const BP_ADVANCED_CONTAINER_SUPPORT: i32 = 6;

    pub fn framework() -> Guid {
        Guid::from_u32s(0xCFFC743F, 0x43B04480, 0x939114DF, 0x171D2073)
    }
    pub const FW_REMOVE_UFIELD_NEXT: i32 = 29;
    pub const FW_EDGRAPH_PIN_CONTAINER_TYPE: i32 = 26;
    pub const FW_PINS_STORE_FNAME: i32 = 31;

    pub fn core() -> Guid {
        Guid::from_u32s(0x375EC13C, 0x06E448FB, 0xB50084F0, 0x262A717E)
    }
    pub const CORE_MATERIAL_INPUT_NATIVE_SERIALIZE: i32 = 1;
    pub const CORE_ENUM_PROPERTIES: i32 = 2;
    pub const CORE_FPROPERTIES: i32 = 4;

    pub fn editor() -> Guid {
        Guid::from_u32s(0xE4B068ED, 0xF49442E9, 0xA231DA0B, 0x2E46BB41)
    }
    pub const ED_ROOT_META_DATA_SUPPORT: i32 = 3;
    pub const ED_TEXT_FORMAT_ARGUMENT_DATA_IS_VARIANT: i32 = 5;
    pub const ED_CULTURE_INVARIANT_TEXT_SERIALIZATION_KEY_STABILITY: i32 = 32;

    pub fn release() -> Guid {
        Guid::from_u32s(0x9C54D522, 0xA8264FBE, 0x94210746, 0x61B482D0)
    }
    pub const REL_PIN_TYPE_INCLUDES_UOBJECT_WRAPPER_FLAG: i32 = 32;

    pub fn ue5_main() -> Guid {
        Guid::from_u32s(0x697DD581, 0xE64F41AB, 0xAA4A51EC, 0xBEB7B628)
    }
    pub const UE5M_EDGRAPH_PIN_SOURCE_INDEX: i32 = 50;

    pub fn ue5_release() -> Guid {
        Guid::from_u32s(0xD89B5E42, 0x24BD4D46, 0x8412ACA8, 0xDF641779)
    }
    pub const UE5R_SERIALIZE_FLOAT_PIN_DEFAULT_VALUES_AS_SINGLE_PRECISION: i32 = 36;

    pub fn fortnite_main() -> Guid {
        Guid::from_u32s(0x601D1886, 0xAC644F84, 0xAA16D3DE, 0x0DEAC7D6)
    }
    pub const FN_MATERIAL_INPUT_USES_LINEAR_COLOR: i32 = 171;
    pub const FN_ADD_DEV_NOTES_TO_FTEXT: i32 = 260;
}
