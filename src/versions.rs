#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub enum EUnrealEngineObjectUE5Version {
    InitialVersion = 1000,

    // Support stripping names that are not referenced from export data
    NamesReferencedFromExportData,

    // Added a payload table of contents to the package summary
    PayloadToc,

    // Added data to identify references from and to optional package
    OptionalResources,

    // Large world coordinates converts a number of core types to double components by default.
    LargeWorldCoordinates,

    // Remove package GUID from FObjectExport
    RemoveObjectExportPackageGuid,

    // Add IsInherited to the FObjectExport entry
    TrackObjectExportIsInherited,

    // Replace FName asset path in FSoftObjectPath with (package name, asset name) pair FTopLevelAssetPath
    FSoftObjectPathRemoveAssetPathFNames,

    // Add a soft object path list to the package summary for fast remap
    AddSoftObjectPathList,

    // Added bulk/data resource table
    DataResources,

    // Added script property serialization offset to export table entries for saved, versioned packages
    ScriptSerializationOffset,

    // Adding property tag extension,
    // Support for overridable serialization on UObject,
    // Support for overridable logic in containers
    PropertyTagExtensionAndOverridableSerialization,

    // Added property tag complete type name and serialization type
    PropertyTagCompleteTypeName,

    // Changed UE::AssetRegistry::WritePackageData to include PackageBuildDependencies
    AssetRegistryPackageBuildDependencies,

    // Added meta data serialization offset to for saved, versioned packages
    MetadataSerializationOffset,

    // Added VCells to the object graph
    VerseCells,

    // Changed PackageFileSummary to write FIoHash PackageSavedHash instead of FGuid Guid
    PackageSavedHash,

    // OS shadow serialization of subobjects
    OsSubObjectShadowSerialization,
}
