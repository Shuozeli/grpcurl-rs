use std::collections::HashSet;
use std::fs;
use std::path::Path;

use async_trait::async_trait;
use prost::Message;
use prost_reflect::{DescriptorPool, ExtensionDescriptor, FieldDescriptor, MessageDescriptor};

use crate::error::{GrpcurlError, Result};

/// Abstraction over different sources of protobuf descriptors.
///
/// Equivalent to Go's `DescriptorSource` interface (desc_source.go:29-39).
/// Three implementations exist in Go:
/// - `fileSource` -- from protoset or .proto files (Phase 2B/2C)
/// - `serverSource` -- from gRPC server reflection (Phase 2D)
/// - `compositeSource` -- reflection + file fallback (Phase 2D)
#[allow(dead_code)]
#[async_trait]
pub trait DescriptorSource: Send + Sync {
    /// Return the names of all services exposed by this source.
    ///
    /// Equivalent to Go's `DescriptorSource.ListServices()`.
    async fn list_services(&self) -> Result<Vec<String>>;

    /// Find a descriptor by its fully-qualified name.
    ///
    /// The name can refer to a service, method, message, enum, field,
    /// extension, or any other protobuf element.
    ///
    /// Returns `GrpcurlError::NotFound` if the symbol does not exist.
    ///
    /// Equivalent to Go's `DescriptorSource.FindSymbol()`.
    async fn find_symbol(&self, fully_qualified_name: &str) -> Result<SymbolDescriptor>;

    /// Return all known extensions for the given fully-qualified message type.
    ///
    /// Equivalent to Go's `DescriptorSource.AllExtensionsForType()`.
    /// Returns ExtensionDescriptor (prost-reflect's type) rather than Go's
    /// FieldDescriptor since protobuf extensions are a distinct concept.
    async fn all_extensions_for_type(&self, type_name: &str) -> Result<Vec<ExtensionDescriptor>>;

    /// Return all file descriptors known to this source.
    ///
    /// Equivalent to Go's `sourceWithFiles.GetAllFiles()`. Not all sources
    /// support this (server reflection does not), so the default returns
    /// an error.
    async fn get_all_files(&self) -> Result<Vec<prost_types::FileDescriptorProto>> {
        Err(GrpcurlError::Other(
            "this descriptor source does not support listing all files".into(),
        ))
    }

    /// Return the underlying descriptor pool, if available.
    ///
    /// This is useful for creating DynamicMessage instances from descriptors
    /// found via this source.
    fn descriptor_pool(&self) -> Option<&DescriptorPool> {
        None
    }
}

/// A resolved protobuf symbol descriptor.
///
/// Go uses `desc.Descriptor` (an interface with many concrete types). In Rust
/// with prost-reflect, we use an enum to represent the different kinds of
/// descriptors a symbol can resolve to.
#[derive(Debug, Clone)]
pub enum SymbolDescriptor {
    Service(prost_reflect::ServiceDescriptor),
    Method(prost_reflect::MethodDescriptor),
    Message(MessageDescriptor),
    Enum(prost_reflect::EnumDescriptor),
    Field(FieldDescriptor),
    Extension(ExtensionDescriptor),
    OneOf(prost_reflect::OneofDescriptor),
    EnumValue(prost_reflect::EnumValueDescriptor),
    File(prost_reflect::FileDescriptor),
}

impl SymbolDescriptor {
    /// Return the fully-qualified name of this symbol.
    #[allow(dead_code)]
    pub fn full_name(&self) -> &str {
        match self {
            SymbolDescriptor::Service(d) => d.full_name(),
            SymbolDescriptor::Method(d) => d.full_name(),
            SymbolDescriptor::Message(d) => d.full_name(),
            SymbolDescriptor::Enum(d) => d.full_name(),
            SymbolDescriptor::Field(d) => d.full_name(),
            SymbolDescriptor::Extension(d) => d.full_name(),
            SymbolDescriptor::OneOf(d) => d.full_name(),
            SymbolDescriptor::EnumValue(d) => d.full_name(),
            // FileDescriptor uses name() (file path), not full_name()
            SymbolDescriptor::File(d) => d.name(),
        }
    }

    /// Return a human-readable type label for this symbol.
    ///
    /// These match the Go output strings used in `describe` output.
    pub fn type_label(&self) -> &'static str {
        match self {
            SymbolDescriptor::Service(_) => "a service",
            SymbolDescriptor::Method(_) => "a method",
            SymbolDescriptor::Message(d) => {
                if d.is_map_entry() {
                    "the entry type for a map field"
                } else {
                    "a message"
                }
            }
            SymbolDescriptor::Enum(_) => "an enum",
            SymbolDescriptor::Field(d) => {
                if d.is_group() {
                    "the type of a group field"
                } else {
                    "a field"
                }
            }
            SymbolDescriptor::Extension(_) => "an extension",
            SymbolDescriptor::OneOf(_) => "a one-of",
            SymbolDescriptor::EnumValue(_) => "an enum value",
            SymbolDescriptor::File(_) => "a file",
        }
    }

    /// If this is a message descriptor, return it.
    #[allow(dead_code)]
    pub fn as_message(&self) -> Option<&MessageDescriptor> {
        match self {
            SymbolDescriptor::Message(d) => Some(d),
            _ => None,
        }
    }

    /// If this is a service descriptor, return it.
    pub fn as_service(&self) -> Option<&prost_reflect::ServiceDescriptor> {
        match self {
            SymbolDescriptor::Service(d) => Some(d),
            _ => None,
        }
    }

    /// If this is a method descriptor, return it.
    #[allow(dead_code)]
    pub fn as_method(&self) -> Option<&prost_reflect::MethodDescriptor> {
        match self {
            SymbolDescriptor::Method(d) => Some(d),
            _ => None,
        }
    }

    /// Return the file descriptor that contains this symbol.
    ///
    /// Equivalent to Go's `d.GetFile()` on any descriptor.
    pub fn parent_file(&self) -> prost_reflect::FileDescriptor {
        match self {
            SymbolDescriptor::Service(d) => d.parent_file(),
            SymbolDescriptor::Method(d) => d.parent_service().parent_file(),
            SymbolDescriptor::Message(d) => d.parent_file(),
            SymbolDescriptor::Enum(d) => d.parent_file(),
            SymbolDescriptor::Field(d) => d.parent_message().parent_file(),
            SymbolDescriptor::Extension(d) => d.parent_file(),
            SymbolDescriptor::OneOf(d) => d.parent_message().parent_file(),
            SymbolDescriptor::EnumValue(d) => d.parent_enum().parent_file(),
            SymbolDescriptor::File(d) => d.clone(),
        }
    }
}

// -- Helper functions (equivalent to Go's top-level functions in grpcurl.go) --

/// List all services from a descriptor source, sorted.
///
/// Equivalent to Go's `ListServices()`.
pub async fn list_services(source: &dyn DescriptorSource) -> Result<Vec<String>> {
    let mut services = source.list_services().await?;
    services.sort();
    Ok(services)
}

/// List all methods for a service, sorted.
///
/// Equivalent to Go's `ListMethods()`.
pub async fn list_methods(source: &dyn DescriptorSource, service: &str) -> Result<Vec<String>> {
    let symbol = source.find_symbol(service).await?;
    let svc = symbol
        .as_service()
        .ok_or_else(|| GrpcurlError::Other(format!("Service not found: {service}").into()))?;

    let mut methods: Vec<String> = svc.methods().map(|m| m.full_name().to_string()).collect();
    methods.sort();
    Ok(methods)
}

/// Retrieve all file descriptors from a source, with fallback.
///
/// Equivalent to Go's `GetAllFiles()`. Tries `get_all_files()` first
/// (efficient for file-backed sources), then falls back to iterating
/// all services and collecting their file descriptors.
#[allow(dead_code)]
pub async fn get_all_files(
    source: &dyn DescriptorSource,
) -> Result<Vec<prost_types::FileDescriptorProto>> {
    // Try the direct path first
    if let Ok(files) = source.get_all_files().await {
        return Ok(files);
    }

    // Fallback: iterate services, collect file descriptors
    let pool = source.descriptor_pool().ok_or_else(|| {
        GrpcurlError::Other("cannot retrieve files: no descriptor pool available".into())
    })?;

    let files: Vec<prost_types::FileDescriptorProto> = pool
        .files()
        .map(|f| f.file_descriptor_proto().clone())
        .collect();
    Ok(files)
}

/// Write a FileDescriptorSet containing the descriptors for the given symbols
/// and their transitive dependencies to the specified file.
///
/// Equivalent to Go's `WriteProtoset()` in desc_source.go.
pub async fn write_protoset(
    path: &str,
    source: &dyn DescriptorSource,
    symbols: &[String],
) -> Result<()> {
    if symbols.is_empty() {
        return Ok(());
    }

    // Resolve symbols to their containing file descriptors
    let mut file_names = Vec::new();
    let mut files: std::collections::HashMap<String, prost_reflect::FileDescriptor> =
        std::collections::HashMap::new();

    for sym in symbols {
        let desc = source.find_symbol(sym).await?;
        let fd = desc.parent_file();
        let name = fd.name().to_string();
        if !files.contains_key(&name) {
            files.insert(name.clone(), fd);
            file_names.push(name);
        }
    }

    // Expand to include transitive dependencies (topologically sorted:
    // each file appears after all its dependencies)
    let mut expanded = HashSet::new();
    let mut all_files: Vec<prost_types::FileDescriptorProto> = Vec::new();

    for name in &file_names {
        collect_transitive_deps(&mut all_files, &mut expanded, &files[name]);
    }

    // Serialize and write
    let fds = prost_types::FileDescriptorSet { file: all_files };
    let bytes = fds.encode_to_vec();
    fs::write(Path::new(path), bytes).map_err(|e| {
        GrpcurlError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to write protoset file '{path}': {e}"),
        ))
    })?;

    Ok(())
}

/// Recursively collect a file descriptor and its dependencies.
///
/// Dependencies are added before the file itself (topological order),
/// matching Go's `addFilesToSet()`.
fn collect_transitive_deps(
    all_files: &mut Vec<prost_types::FileDescriptorProto>,
    expanded: &mut HashSet<String>,
    fd: &prost_reflect::FileDescriptor,
) {
    let name = fd.name().to_string();
    if expanded.contains(&name) {
        return;
    }
    expanded.insert(name);

    // Add all dependencies first
    for dep in fd.dependencies() {
        collect_transitive_deps(all_files, expanded, &dep);
    }

    all_files.push(fd.file_descriptor_proto().clone());
}

/// Recursively collect FileDescriptors (not protos) and their dependencies.
///
/// Similar to `collect_transitive_deps` but returns `FileDescriptor` objects
/// needed for proto file generation (which needs the full descriptor for text formatting).
fn collect_transitive_file_descriptors(
    all_files: &mut Vec<prost_reflect::FileDescriptor>,
    expanded: &mut HashSet<String>,
    fd: &prost_reflect::FileDescriptor,
) {
    let name = fd.name().to_string();
    if expanded.contains(&name) {
        return;
    }
    expanded.insert(name);

    for dep in fd.dependencies() {
        collect_transitive_file_descriptors(all_files, expanded, &dep);
    }

    all_files.push(fd.clone());
}

/// Write .proto source files for the given symbols and their transitive
/// dependencies to the specified directory.
///
/// Matches Go's `WriteProtoFiles()` in desc_source.go.
/// Each file is named using its proto file name (e.g., "google/protobuf/empty.proto")
/// and nested directories are created as needed.
pub async fn write_proto_files(
    dir: &str,
    source: &dyn DescriptorSource,
    symbols: &[String],
) -> Result<()> {
    if symbols.is_empty() {
        return Ok(());
    }

    // Resolve symbols to their containing file descriptors
    let mut file_names = Vec::new();
    let mut files: std::collections::HashMap<String, prost_reflect::FileDescriptor> =
        std::collections::HashMap::new();

    for sym in symbols {
        let desc = source.find_symbol(sym).await?;
        let fd = desc.parent_file();
        let name = fd.name().to_string();
        if !files.contains_key(&name) {
            files.insert(name.clone(), fd);
            file_names.push(name);
        }
    }

    // Expand to include transitive dependencies (topologically sorted)
    let mut expanded = HashSet::new();
    let mut all_files: Vec<prost_reflect::FileDescriptor> = Vec::new();

    for name in &file_names {
        collect_transitive_file_descriptors(&mut all_files, &mut expanded, &files[name]);
    }

    // Write each file
    let base = Path::new(dir);
    for fd in &all_files {
        let out_path = base.join(fd.name());
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                GrpcurlError::Io(std::io::Error::new(
                    e.kind(),
                    format!("failed to create directory '{}': {e}", parent.display()),
                ))
            })?;
        }
        let content = crate::descriptor_text::format_proto_file(fd);
        fs::write(&out_path, content).map_err(|e| {
            GrpcurlError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to write proto file '{}': {e}", out_path.display()),
            ))
        })?;
    }

    Ok(())
}

// -- FileSource implementation ------------------------------------------------

/// Descriptor source backed by pre-compiled file descriptors.
///
/// Equivalent to Go's `fileSource` (desc_source.go:151-202).
/// Used for both protoset files and parsed .proto files.
pub struct FileSource {
    pool: DescriptorPool,
}

impl FileSource {
    fn new(pool: DescriptorPool) -> Self {
        FileSource { pool }
    }
}

#[async_trait]
impl DescriptorSource for FileSource {
    async fn list_services(&self) -> Result<Vec<String>> {
        let services: Vec<String> = self
            .pool
            .services()
            .map(|s| s.full_name().to_string())
            .collect();
        Ok(services)
    }

    async fn find_symbol(&self, fully_qualified_name: &str) -> Result<SymbolDescriptor> {
        find_symbol_in_pool(&self.pool, fully_qualified_name)
    }

    async fn all_extensions_for_type(&self, type_name: &str) -> Result<Vec<ExtensionDescriptor>> {
        let exts: Vec<ExtensionDescriptor> = self
            .pool
            .all_extensions()
            .filter(|ext| ext.containing_message().full_name() == type_name)
            .collect();
        Ok(exts)
    }

    async fn get_all_files(&self) -> Result<Vec<prost_types::FileDescriptorProto>> {
        let files: Vec<prost_types::FileDescriptorProto> =
            self.pool.file_descriptor_protos().cloned().collect();
        Ok(files)
    }

    fn descriptor_pool(&self) -> Option<&DescriptorPool> {
        Some(&self.pool)
    }
}

// -- CompositeSource implementation -------------------------------------------

/// Descriptor source combining server reflection with a file-based fallback.
///
/// Equivalent to Go's `compositeSource` (cmd/grpcurl/grpcurl.go:248-287).
/// Uses reflection as the primary source for listing services, and falls
/// back to the file source for symbol resolution when reflection fails.
pub struct CompositeSource {
    reflection: Box<dyn DescriptorSource>,
    file: Box<dyn DescriptorSource>,
}

impl CompositeSource {
    pub fn new(reflection: Box<dyn DescriptorSource>, file: Box<dyn DescriptorSource>) -> Self {
        CompositeSource { reflection, file }
    }
}

#[async_trait]
impl DescriptorSource for CompositeSource {
    async fn list_services(&self) -> Result<Vec<String>> {
        // Always use reflection for listing services
        self.reflection.list_services().await
    }

    async fn find_symbol(&self, fully_qualified_name: &str) -> Result<SymbolDescriptor> {
        // Try reflection first, fall back to file source
        match self.reflection.find_symbol(fully_qualified_name).await {
            Ok(desc) => Ok(desc),
            Err(_) => self.file.find_symbol(fully_qualified_name).await,
        }
    }

    async fn all_extensions_for_type(&self, type_name: &str) -> Result<Vec<ExtensionDescriptor>> {
        // Try reflection first
        match self.reflection.all_extensions_for_type(type_name).await {
            Ok(ref_exts) => {
                // Merge with file source extensions (reflection takes priority)
                let mut tags: HashSet<u32> = HashSet::new();
                for ext in &ref_exts {
                    tags.insert(ext.number());
                }
                let mut all_exts = ref_exts;
                if let Ok(file_exts) = self.file.all_extensions_for_type(type_name).await {
                    for ext in file_exts {
                        if !tags.contains(&ext.number()) {
                            all_exts.push(ext);
                        }
                    }
                }
                Ok(all_exts)
            }
            Err(_) => self.file.all_extensions_for_type(type_name).await,
        }
    }
}

// -- Factory functions --------------------------------------------------------

/// Create a descriptor source from one or more protoset files.
///
/// Each file must contain a binary-encoded `FileDescriptorSet` (as produced by
/// `protoc --descriptor_set_out`).
///
/// Equivalent to Go's `DescriptorSourceFromProtoSets()`.
pub fn descriptor_source_from_protosets(paths: &[String]) -> Result<FileSource> {
    let mut pool = DescriptorPool::new();

    for path in paths {
        let bytes = fs::read(Path::new(path)).map_err(|e| {
            GrpcurlError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to read protoset file '{path}': {e}"),
            ))
        })?;

        let fds = prost_types::FileDescriptorSet::decode(bytes.as_slice()).map_err(|e| {
            GrpcurlError::Proto(format!("failed to decode protoset file '{path}': {e}"))
        })?;

        pool.add_file_descriptor_set(fds).map_err(|e| {
            GrpcurlError::Proto(format!(
                "failed to add descriptors from protoset file '{path}': {e}"
            ))
        })?;
    }

    Ok(FileSource::new(pool))
}

/// Create a descriptor source from .proto source files.
///
/// Parses proto files using the `protox` compiler with the given import paths.
///
/// Equivalent to Go's `DescriptorSourceFromProtoFiles()`.
pub fn descriptor_source_from_proto_files(
    import_paths: &[String],
    proto_files: &[String],
) -> Result<FileSource> {
    let includes: Vec<&str> = if import_paths.is_empty() {
        // Default to current directory if no import paths specified (matches Go)
        vec!["."]
    } else {
        import_paths.iter().map(String::as_str).collect()
    };

    let fds = protox::compile(proto_files, includes)
        .map_err(|e| GrpcurlError::Proto(format!("failed to compile proto files: {e}")))?;

    descriptor_source_from_file_descriptor_set(fds)
}

/// Create a descriptor source from a `FileDescriptorSet`.
///
/// Equivalent to Go's `DescriptorSourceFromFileDescriptorSet()`.
pub fn descriptor_source_from_file_descriptor_set(
    fds: prost_types::FileDescriptorSet,
) -> Result<FileSource> {
    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .map_err(|e| GrpcurlError::Proto(format!("failed to build descriptor pool: {e}")))?;
    Ok(FileSource::new(pool))
}

// -- Symbol lookup in a DescriptorPool ----------------------------------------

/// Find any symbol by fully-qualified name in a descriptor pool.
///
/// Tries all top-level descriptor types (service, message, enum, extension),
/// then falls back to sub-element lookups (methods, fields, oneofs, enum values)
/// by splitting the name at the last dot and looking up the parent first.
pub(crate) fn find_symbol_in_pool(pool: &DescriptorPool, name: &str) -> Result<SymbolDescriptor> {
    // Try top-level types first (most common lookups)
    if let Some(svc) = pool.get_service_by_name(name) {
        return Ok(SymbolDescriptor::Service(svc));
    }
    if let Some(msg) = pool.get_message_by_name(name) {
        return Ok(SymbolDescriptor::Message(msg));
    }
    if let Some(e) = pool.get_enum_by_name(name) {
        return Ok(SymbolDescriptor::Enum(e));
    }
    if let Some(ext) = pool.get_extension_by_name(name) {
        return Ok(SymbolDescriptor::Extension(ext));
    }

    // Try sub-elements by splitting at the last dot
    if let Some((parent_name, child_name)) = name.rsplit_once('.') {
        // Try method (parent = service)
        if let Some(svc) = pool.get_service_by_name(parent_name) {
            for method in svc.methods() {
                if method.name() == child_name {
                    return Ok(SymbolDescriptor::Method(method));
                }
            }
        }

        // Try field or oneof (parent = message)
        if let Some(msg) = pool.get_message_by_name(parent_name) {
            for field in msg.fields() {
                if field.name() == child_name {
                    return Ok(SymbolDescriptor::Field(field));
                }
            }
            for oneof in msg.oneofs() {
                if oneof.name() == child_name {
                    return Ok(SymbolDescriptor::OneOf(oneof));
                }
            }
        }

        // Try enum value (parent = enum)
        if let Some(e) = pool.get_enum_by_name(parent_name) {
            for val in e.values() {
                if val.name() == child_name {
                    return Ok(SymbolDescriptor::EnumValue(val));
                }
            }
        }
    }

    // Try file by name
    for file in pool.files() {
        if file.name() == name {
            return Ok(SymbolDescriptor::File(file));
        }
    }

    Err(GrpcurlError::NotFound(name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_pool() -> DescriptorPool {
        // Build a minimal FileDescriptorSet with a service, message, and enum.
        let fds = prost_types::FileDescriptorSet {
            file: vec![prost_types::FileDescriptorProto {
                name: Some("test.proto".into()),
                package: Some("test.v1".into()),
                message_type: vec![prost_types::DescriptorProto {
                    name: Some("HelloRequest".into()),
                    field: vec![prost_types::FieldDescriptorProto {
                        name: Some("name".into()),
                        number: Some(1),
                        r#type: Some(9), // TYPE_STRING
                        label: Some(1),  // LABEL_OPTIONAL
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                enum_type: vec![prost_types::EnumDescriptorProto {
                    name: Some("Status".into()),
                    value: vec![
                        prost_types::EnumValueDescriptorProto {
                            name: Some("UNKNOWN".into()),
                            number: Some(0),
                            ..Default::default()
                        },
                        prost_types::EnumValueDescriptorProto {
                            name: Some("ACTIVE".into()),
                            number: Some(1),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                service: vec![prost_types::ServiceDescriptorProto {
                    name: Some("Greeter".into()),
                    method: vec![prost_types::MethodDescriptorProto {
                        name: Some("SayHello".into()),
                        input_type: Some(".test.v1.HelloRequest".into()),
                        output_type: Some(".test.v1.HelloRequest".into()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                syntax: Some("proto3".into()),
                ..Default::default()
            }],
        };
        DescriptorPool::from_file_descriptor_set(fds).unwrap()
    }

    #[tokio::test]
    async fn file_source_list_services() {
        let pool = make_test_pool();
        let source = FileSource::new(pool);
        let services = source.list_services().await.unwrap();
        assert_eq!(services, vec!["test.v1.Greeter"]);
    }

    #[tokio::test]
    async fn file_source_find_service() {
        let pool = make_test_pool();
        let source = FileSource::new(pool);
        let sym = source.find_symbol("test.v1.Greeter").await.unwrap();
        assert_eq!(sym.type_label(), "a service");
        assert_eq!(sym.full_name(), "test.v1.Greeter");
    }

    #[tokio::test]
    async fn file_source_find_message() {
        let pool = make_test_pool();
        let source = FileSource::new(pool);
        let sym = source.find_symbol("test.v1.HelloRequest").await.unwrap();
        assert_eq!(sym.type_label(), "a message");
    }

    #[tokio::test]
    async fn file_source_find_method() {
        let pool = make_test_pool();
        let source = FileSource::new(pool);
        let sym = source
            .find_symbol("test.v1.Greeter.SayHello")
            .await
            .unwrap();
        assert_eq!(sym.type_label(), "a method");
    }

    #[tokio::test]
    async fn file_source_find_field() {
        let pool = make_test_pool();
        let source = FileSource::new(pool);
        let sym = source
            .find_symbol("test.v1.HelloRequest.name")
            .await
            .unwrap();
        assert_eq!(sym.type_label(), "a field");
    }

    #[tokio::test]
    async fn file_source_find_enum() {
        let pool = make_test_pool();
        let source = FileSource::new(pool);
        let sym = source.find_symbol("test.v1.Status").await.unwrap();
        assert_eq!(sym.type_label(), "an enum");
    }

    #[tokio::test]
    async fn file_source_find_enum_value() {
        let pool = make_test_pool();
        let source = FileSource::new(pool);
        let sym = source.find_symbol("test.v1.Status.ACTIVE").await.unwrap();
        assert_eq!(sym.type_label(), "an enum value");
    }

    #[tokio::test]
    async fn file_source_find_not_found() {
        let pool = make_test_pool();
        let source = FileSource::new(pool);
        let result = source.find_symbol("does.not.Exist").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GrpcurlError::NotFound(_)));
    }

    #[tokio::test]
    async fn file_source_get_all_files() {
        let pool = make_test_pool();
        let source = FileSource::new(pool);
        let files = source.get_all_files().await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name(), "test.proto");
    }

    #[tokio::test]
    async fn list_methods_helper() {
        let pool = make_test_pool();
        let source = FileSource::new(pool);
        let methods = list_methods(&source, "test.v1.Greeter").await.unwrap();
        assert_eq!(methods, vec!["test.v1.Greeter.SayHello"]);
    }

    #[tokio::test]
    async fn descriptor_source_from_fds() {
        let fds = prost_types::FileDescriptorSet {
            file: vec![prost_types::FileDescriptorProto {
                name: Some("simple.proto".into()),
                package: Some("simple".into()),
                service: vec![prost_types::ServiceDescriptorProto {
                    name: Some("Echo".into()),
                    ..Default::default()
                }],
                syntax: Some("proto3".into()),
                ..Default::default()
            }],
        };
        let source = descriptor_source_from_file_descriptor_set(fds).unwrap();
        let services = source.list_services().await.unwrap();
        assert_eq!(services, vec!["simple.Echo"]);
    }
}
