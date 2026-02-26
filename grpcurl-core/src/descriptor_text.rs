use prost_reflect::{
    EnumDescriptor, EnumValueDescriptor, FieldDescriptor, FileDescriptor, Kind, MessageDescriptor,
    MethodDescriptor, OneofDescriptor, ServiceDescriptor,
};

use crate::descriptor::SymbolDescriptor;

/// Format a symbol descriptor as proto source text, matching Go's protoprint output.
///
/// Go uses `protoprint.Printer` configured with: compact format, no non-doc comments,
/// sorted elements, fully-qualified names.
pub fn get_descriptor_text(sym: &SymbolDescriptor) -> String {
    match sym {
        SymbolDescriptor::Service(d) => format_service(d),
        SymbolDescriptor::Method(d) => format_method(d),
        SymbolDescriptor::Message(d) => format_message(d),
        SymbolDescriptor::Enum(d) => format_enum(d),
        SymbolDescriptor::Field(d) => format_field(d),
        SymbolDescriptor::Extension(d) => format_extension(d),
        SymbolDescriptor::OneOf(d) => format_oneof(d),
        SymbolDescriptor::EnumValue(d) => format_enum_value(d),
        SymbolDescriptor::File(_) => String::new(),
    }
}

/// Format a complete .proto file from a FileDescriptor.
///
/// Generates valid proto source text including syntax, package, imports,
/// file options, messages, enums, services, and extensions.
/// Matches Go's `protoprint.Printer.PrintProtoFile()` output.
pub fn format_proto_file(fd: &FileDescriptor) -> String {
    let proto = fd.file_descriptor_proto();
    let mut out = String::new();

    // Syntax
    let syntax = proto.syntax.as_deref().unwrap_or("proto2");
    out.push_str(&format!("syntax = \"{syntax}\";\n"));

    // Package
    if let Some(ref pkg) = proto.package {
        if !pkg.is_empty() {
            out.push('\n');
            out.push_str(&format!("package {pkg};\n"));
        }
    }

    // Imports
    if !proto.dependency.is_empty() {
        out.push('\n');
        let public_deps: std::collections::HashSet<usize> = proto
            .public_dependency
            .iter()
            .map(|&i| i as usize)
            .collect();
        let weak_deps: std::collections::HashSet<usize> =
            proto.weak_dependency.iter().map(|&i| i as usize).collect();
        for (i, dep) in proto.dependency.iter().enumerate() {
            if public_deps.contains(&i) {
                out.push_str(&format!("import public \"{dep}\";\n"));
            } else if weak_deps.contains(&i) {
                out.push_str(&format!("import weak \"{dep}\";\n"));
            } else {
                out.push_str(&format!("import \"{dep}\";\n"));
            }
        }
    }

    // File options
    if let Some(ref opts) = proto.options {
        let mut option_lines = Vec::new();
        if let Some(ref v) = opts.java_package {
            option_lines.push(format!("option java_package = \"{v}\";"));
        }
        if let Some(ref v) = opts.java_outer_classname {
            option_lines.push(format!("option java_outer_classname = \"{v}\";"));
        }
        if let Some(v) = opts.java_multiple_files {
            if v {
                option_lines.push("option java_multiple_files = true;".into());
            }
        }
        if let Some(ref v) = opts.go_package {
            option_lines.push(format!("option go_package = \"{v}\";"));
        }
        if let Some(ref v) = opts.csharp_namespace {
            option_lines.push(format!("option csharp_namespace = \"{v}\";"));
        }
        if let Some(ref v) = opts.objc_class_prefix {
            option_lines.push(format!("option objc_class_prefix = \"{v}\";"));
        }
        if let Some(ref v) = opts.php_namespace {
            option_lines.push(format!("option php_namespace = \"{v}\";"));
        }
        if let Some(ref v) = opts.ruby_package {
            option_lines.push(format!("option ruby_package = \"{v}\";"));
        }
        if let Some(ref v) = opts.swift_prefix {
            option_lines.push(format!("option swift_prefix = \"{v}\";"));
        }
        if let Some(v) = opts.cc_enable_arenas {
            if v {
                option_lines.push("option cc_enable_arenas = true;".into());
            }
        }
        // Protobuf OptimizeMode enum values from descriptor.proto
        const OPTIMIZE_SPEED: i32 = 1;
        const OPTIMIZE_CODE_SIZE: i32 = 2;
        const OPTIMIZE_LITE_RUNTIME: i32 = 3;

        if let Some(v) = opts.optimize_for {
            let name = match v {
                OPTIMIZE_SPEED => "SPEED",
                OPTIMIZE_CODE_SIZE => "CODE_SIZE",
                OPTIMIZE_LITE_RUNTIME => "LITE_RUNTIME",
                _ => "",
            };
            if !name.is_empty() {
                option_lines.push(format!("option optimize_for = {name};"));
            }
        }
        if !option_lines.is_empty() {
            out.push('\n');
            for line in &option_lines {
                out.push_str(line);
                out.push('\n');
            }
        }
    }

    let pkg = proto.package.as_deref().unwrap_or("");

    // Messages
    for msg in fd.messages() {
        out.push('\n');
        out.push_str(&file_format_message(&msg, pkg));
        out.push('\n');
    }

    // Enums
    for e in fd.enums() {
        out.push('\n');
        out.push_str(&file_format_enum(&e));
        out.push('\n');
    }

    // Extensions (top-level)
    let extensions: Vec<_> = fd.extensions().collect();
    if !extensions.is_empty() {
        // Group extensions by extendee
        let mut by_extendee: std::collections::BTreeMap<String, Vec<_>> =
            std::collections::BTreeMap::new();
        for ext in &extensions {
            let extendee = short_name(ext.containing_message().full_name(), pkg);
            by_extendee.entry(extendee).or_default().push(ext);
        }
        for (extendee, exts) in &by_extendee {
            out.push('\n');
            out.push_str(&format!("extend {extendee} {{\n"));
            for ext in exts {
                out.push_str("  ");
                out.push_str(&format_extension(ext));
                out.push('\n');
            }
            out.push_str("}\n");
        }
    }

    // Services (preserve original order, use short names)
    for svc in fd.services() {
        out.push('\n');
        out.push_str(&file_format_service(&svc, pkg));
        out.push('\n');
    }

    out
}

/// Shorten a fully-qualified name by removing the package prefix.
/// "test.v1.HelloRequest" with package "test.v1" -> "HelloRequest"
/// Names in other packages keep the fully-qualified form with leading dot.
fn short_name(full_name: &str, pkg: &str) -> String {
    if pkg.is_empty() {
        return full_name.to_string();
    }
    let prefix = format!("{pkg}.");
    if let Some(short) = full_name.strip_prefix(&prefix) {
        // Only shorten if it's a direct child (no more dots = top-level type in same package)
        short.to_string()
    } else {
        format!(".{full_name}")
    }
}

/// Format a service for proto file output (preserves original method order, short names).
fn file_format_service(svc: &ServiceDescriptor, pkg: &str) -> String {
    let mut out = format!("service {} {{\n", svc.name());

    // Preserve original order (don't sort)
    let methods: Vec<_> = svc.methods().collect();
    for (i, method) in methods.iter().enumerate() {
        out.push_str("  ");
        out.push_str(&file_format_method(method, pkg));
        out.push('\n');
        // Blank line between methods (matching Go's protoprint)
        if i + 1 < methods.len() {
            out.push('\n');
        }
    }

    out.push('}');
    out
}

/// Format a method for proto file output (uses short type names).
fn file_format_method(method: &MethodDescriptor, pkg: &str) -> String {
    let input = method.input();
    let output = method.output();

    let client_stream = if method.is_client_streaming() {
        "stream "
    } else {
        ""
    };
    let server_stream = if method.is_server_streaming() {
        "stream "
    } else {
        ""
    };

    format!(
        "rpc {} ( {}{} ) returns ( {}{} );",
        method.name(),
        client_stream,
        short_name(input.full_name(), pkg),
        server_stream,
        short_name(output.full_name(), pkg),
    )
}

/// Format a message for proto file output (uses short type names).
fn file_format_message(msg: &MessageDescriptor, pkg: &str) -> String {
    let mut out = format!("message {} {{\n", msg.name());

    let mut field_entries: Vec<FieldEntry> = Vec::new();

    let mut oneof_fields: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for oneof in msg.oneofs() {
        if is_synthetic_oneof(&oneof) {
            continue;
        }
        for field in oneof.fields() {
            oneof_fields.insert(field.number());
        }
    }

    for field in msg.fields() {
        if oneof_fields.contains(&field.number()) {
            continue;
        }
        field_entries.push(FieldEntry {
            number: field.number(),
            text: file_format_field(&field, pkg),
        });
    }

    for oneof in msg.oneofs() {
        if is_synthetic_oneof(&oneof) {
            continue;
        }
        let min_number = oneof.fields().map(|f| f.number()).min().unwrap_or(u32::MAX);
        field_entries.push(FieldEntry {
            number: min_number,
            text: file_format_oneof(&oneof, pkg),
        });
    }

    // Nested messages
    for nested in msg.child_messages() {
        // Skip map entry types (they're synthesized)
        if nested.is_map_entry() {
            continue;
        }
        let min_num = nested
            .fields()
            .map(|f| f.number())
            .min()
            .unwrap_or(u32::MAX);
        field_entries.push(FieldEntry {
            number: min_num,
            text: file_format_message(&nested, pkg),
        });
    }

    // Nested enums
    for nested_enum in msg.child_enums() {
        let min_num = nested_enum
            .values()
            .map(|v| v.number() as u32)
            .min()
            .unwrap_or(u32::MAX);
        field_entries.push(FieldEntry {
            number: min_num,
            text: file_format_enum(&nested_enum),
        });
    }

    field_entries.sort_by_key(|e| e.number);

    for (i, entry) in field_entries.iter().enumerate() {
        for line in entry.text.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
        // Blank line between entries (matching Go's protoprint)
        if i + 1 < field_entries.len() {
            out.push('\n');
        }
    }

    out.push('}');
    out
}

/// Format an enum for proto file output (blank lines between values).
fn file_format_enum(e: &EnumDescriptor) -> String {
    let mut out = format!("enum {} {{\n", e.name());

    let mut values: Vec<EnumValueDescriptor> = e.values().collect();
    values.sort_by_key(|v| v.number());

    for (i, val) in values.iter().enumerate() {
        out.push_str("  ");
        out.push_str(&format_enum_value(val));
        out.push('\n');
        if i + 1 < values.len() {
            out.push('\n');
        }
    }

    out.push('}');
    out
}

/// Format a field for proto file output (uses short type names).
fn file_format_field(field: &FieldDescriptor, pkg: &str) -> String {
    let options = format_field_options(field);

    if field.is_map() {
        if let Kind::Message(entry_msg) = field.kind() {
            let key_field = entry_msg
                .get_field_by_name("key")
                .expect("map entry has key");
            let val_field = entry_msg
                .get_field_by_name("value")
                .expect("map entry has value");
            let key_type = scalar_type_name(&key_field);
            let val_type = file_field_type_name(&val_field, pkg);
            return format!(
                "map<{}, {}> {} = {}{};",
                key_type,
                val_type,
                field.name(),
                field.number(),
                options
            );
        }
    }

    let type_name = file_field_type_name(field, pkg);
    let repeated = if field.is_list() { "repeated " } else { "" };
    format!(
        "{}{} {} = {}{};",
        repeated,
        type_name,
        field.name(),
        field.number(),
        options
    )
}

/// Format a oneof for proto file output (uses short type names).
fn file_format_oneof(oneof: &OneofDescriptor, pkg: &str) -> String {
    let mut out = format!("oneof {} {{\n", oneof.name());

    let mut fields: Vec<FieldDescriptor> = oneof.fields().collect();
    fields.sort_by_key(|f| f.number());

    for field in &fields {
        out.push_str("  ");
        out.push_str(&file_format_field(field, pkg));
        out.push('\n');
    }

    out.push('}');
    out
}

/// Get the type name for a field using short names for same-package types.
fn file_field_type_name(field: &FieldDescriptor, pkg: &str) -> String {
    match field.kind() {
        Kind::Message(msg) => short_name(msg.full_name(), pkg),
        Kind::Enum(e) => short_name(e.full_name(), pkg),
        _ => scalar_type_name(field),
    }
}

fn format_service(svc: &ServiceDescriptor) -> String {
    let mut out = format!("service {} {{\n", svc.name());

    let mut methods: Vec<MethodDescriptor> = svc.methods().collect();
    methods.sort_by(|a, b| a.name().cmp(b.name()));

    for method in &methods {
        out.push_str("  ");
        out.push_str(&format_method(method));
        out.push('\n');
    }

    out.push('}');
    out
}

fn format_method(method: &MethodDescriptor) -> String {
    let input = method.input();
    let output = method.output();

    let client_stream = if method.is_client_streaming() {
        "stream "
    } else {
        ""
    };
    let server_stream = if method.is_server_streaming() {
        "stream "
    } else {
        ""
    };

    format!(
        "rpc {} ( {}{} ) returns ( {}{} );",
        method.name(),
        client_stream,
        fully_qualified_name(&input),
        server_stream,
        fully_qualified_name(&output),
    )
}

fn format_message(msg: &MessageDescriptor) -> String {
    let mut out = format!("message {} {{\n", msg.name());

    // Reserved ranges and names (at the top of the message, matching Go)
    for reserved_line in format_reserved_ranges(msg) {
        out.push_str("  ");
        out.push_str(&reserved_line);
        out.push('\n');
    }

    // Collect fields and oneofs
    let mut field_entries: Vec<FieldEntry> = Vec::new();

    // Track which fields are part of a oneof (we'll render them inside the oneof block)
    let mut oneof_fields: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for oneof in msg.oneofs() {
        // Skip synthetic oneofs (proto3 optional fields)
        if is_synthetic_oneof(&oneof) {
            continue;
        }
        for field in oneof.fields() {
            oneof_fields.insert(field.number());
        }
    }

    // Add regular fields (not in oneofs)
    for field in msg.fields() {
        if oneof_fields.contains(&field.number()) {
            continue;
        }
        field_entries.push(FieldEntry {
            number: field.number(),
            text: format_field(&field),
        });
    }

    // Add oneof blocks
    for oneof in msg.oneofs() {
        if is_synthetic_oneof(&oneof) {
            continue;
        }
        // Use the lowest field number in the oneof for ordering
        let min_number = oneof.fields().map(|f| f.number()).min().unwrap_or(u32::MAX);
        field_entries.push(FieldEntry {
            number: min_number,
            text: format_oneof(&oneof),
        });
    }

    // Sort by field number (preserves proto source order)
    field_entries.sort_by_key(|e| e.number);

    for entry in &field_entries {
        for line in entry.text.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }

    out.push('}');
    out
}

struct FieldEntry {
    number: u32,
    text: String,
}

fn format_field(field: &FieldDescriptor) -> String {
    let type_name = field_type_name(field);
    let options = format_field_options(field);

    if field.is_map() {
        // Map field: map<KeyType, ValueType> name = number;
        if let Kind::Message(entry_msg) = field.kind() {
            let key_field = entry_msg
                .get_field_by_name("key")
                .expect("map entry has key");
            let val_field = entry_msg
                .get_field_by_name("value")
                .expect("map entry has value");
            let key_type = scalar_type_name(&key_field);
            let val_type = field_type_name(&val_field);
            return format!(
                "map<{}, {}> {} = {}{};",
                key_type,
                val_type,
                field.name(),
                field.number(),
                options
            );
        }
    }

    let repeated = if field.is_list() { "repeated " } else { "" };
    format!(
        "{}{} {} = {}{};",
        repeated,
        type_name,
        field.name(),
        field.number(),
        options
    )
}

/// Format field options in brackets, e.g. ` [deprecated = true, json_name = "foo"]`.
/// Returns empty string if no options are set.
fn format_field_options(field: &FieldDescriptor) -> String {
    let proto = field.field_descriptor_proto();
    let mut opts = Vec::new();

    if let Some(ref field_opts) = proto.options {
        if field_opts.deprecated == Some(true) {
            opts.push("deprecated = true".to_string());
        }
        if field_opts.packed == Some(true) {
            opts.push("packed = true".to_string());
        }
        if field_opts.packed == Some(false) {
            opts.push("packed = false".to_string());
        }
        if let Some(ref js_type) = field_opts.jstype {
            let js_name = match *js_type {
                1 => "JS_STRING",
                2 => "JS_NUMBER",
                _ => "",
            };
            if !js_name.is_empty() {
                opts.push(format!("jstype = {js_name}"));
            }
        }
    }

    // Include json_name if it differs from the default snake_case->camelCase mapping
    if let Some(ref json_name) = proto.json_name {
        let default_json = to_lower_camel_case(field.name());
        if *json_name != default_json {
            opts.push(format!("json_name = \"{json_name}\""));
        }
    }

    if opts.is_empty() {
        String::new()
    } else {
        format!(" [{}]", opts.join(", "))
    }
}

/// Convert snake_case to lowerCamelCase (protobuf default json_name mapping).
fn to_lower_camel_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;
    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Format reserved ranges and names for a message descriptor.
fn format_reserved_ranges(msg: &MessageDescriptor) -> Vec<String> {
    let proto = msg.descriptor_proto();
    let mut lines = Vec::new();

    // Reserved ranges
    if !proto.reserved_range.is_empty() {
        let ranges: Vec<String> = proto
            .reserved_range
            .iter()
            .map(|r| {
                let start = r.start.unwrap_or(0);
                let end = r.end.unwrap_or(0) - 1; // proto uses exclusive end
                if start == end {
                    format!("{start}")
                } else if end == i32::MAX - 1 || end >= 536870911 {
                    format!("{start} to max")
                } else {
                    format!("{start} to {end}")
                }
            })
            .collect();
        lines.push(format!("reserved {};", ranges.join(", ")));
    }

    // Reserved names
    if !proto.reserved_name.is_empty() {
        let names: Vec<String> = proto
            .reserved_name
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect();
        lines.push(format!("reserved {};", names.join(", ")));
    }

    lines
}

fn format_extension(ext: &prost_reflect::ExtensionDescriptor) -> String {
    let type_name = extension_type_name(ext);
    let repeated = if ext.is_list() { "repeated " } else { "" };
    format!(
        "{}{} {} = {};",
        repeated,
        type_name,
        ext.name(),
        ext.number()
    )
}

fn format_enum(e: &EnumDescriptor) -> String {
    let mut out = format!("enum {} {{\n", e.name());

    let mut values: Vec<EnumValueDescriptor> = e.values().collect();
    values.sort_by_key(|v| v.number());

    for val in &values {
        out.push_str("  ");
        out.push_str(&format_enum_value(val));
        out.push('\n');
    }

    out.push('}');
    out
}

fn format_enum_value(val: &EnumValueDescriptor) -> String {
    format!("{} = {};", val.name(), val.number())
}

fn format_oneof(oneof: &OneofDescriptor) -> String {
    let mut out = format!("oneof {} {{\n", oneof.name());

    let mut fields: Vec<FieldDescriptor> = oneof.fields().collect();
    fields.sort_by_key(|f| f.number());

    for field in &fields {
        out.push_str("  ");
        out.push_str(&format_field(field));
        out.push('\n');
    }

    out.push('}');
    out
}

/// Check if a oneof is synthetic (created by proto3 optional).
/// Synthetic oneofs have exactly one field and are not declared in the source.
fn is_synthetic_oneof(oneof: &OneofDescriptor) -> bool {
    let fields: Vec<_> = oneof.fields().collect();
    if fields.len() != 1 {
        return false;
    }
    // In proto3, synthetic oneofs are generated for optional fields.
    // prost-reflect marks the parent oneof in the field descriptor proto.
    // A synthetic oneof has exactly one field and the field has proto3_optional set.
    fields[0]
        .field_descriptor_proto()
        .proto3_optional
        .unwrap_or(false)
}

/// Get the fully-qualified type name for a Kind (message/enum get leading dot).
fn kind_to_type_name(kind: Kind) -> String {
    match kind {
        Kind::Double => "double".into(),
        Kind::Float => "float".into(),
        Kind::Int64 => "int64".into(),
        Kind::Uint64 => "uint64".into(),
        Kind::Int32 => "int32".into(),
        Kind::Fixed64 => "fixed64".into(),
        Kind::Fixed32 => "fixed32".into(),
        Kind::Bool => "bool".into(),
        Kind::String => "string".into(),
        Kind::Bytes => "bytes".into(),
        Kind::Uint32 => "uint32".into(),
        Kind::Sfixed32 => "sfixed32".into(),
        Kind::Sfixed64 => "sfixed64".into(),
        Kind::Sint32 => "sint32".into(),
        Kind::Sint64 => "sint64".into(),
        Kind::Message(msg) => fully_qualified_name(&msg),
        Kind::Enum(e) => format!(".{}", e.full_name()),
    }
}

/// Get the type name for a field, fully qualifying message/enum types.
fn field_type_name(field: &FieldDescriptor) -> String {
    kind_to_type_name(field.kind())
}

/// Get the type name for an extension field.
fn extension_type_name(ext: &prost_reflect::ExtensionDescriptor) -> String {
    kind_to_type_name(ext.kind())
}

/// Get the scalar type name for a regular field.
fn scalar_type_name(field: &FieldDescriptor) -> String {
    kind_to_type_name(field.kind())
}

/// Format a message descriptor as a fully-qualified name with leading dot.
fn fully_qualified_name(msg: &MessageDescriptor) -> String {
    format!(".{}", msg.full_name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::SymbolDescriptor;
    use prost_reflect::DescriptorPool;

    fn make_pool() -> DescriptorPool {
        let fds = prost_types::FileDescriptorSet {
            file: vec![prost_types::FileDescriptorProto {
                name: Some("test.proto".into()),
                package: Some("test.v1".into()),
                message_type: vec![
                    prost_types::DescriptorProto {
                        name: Some("HelloRequest".into()),
                        field: vec![prost_types::FieldDescriptorProto {
                            name: Some("name".into()),
                            number: Some(1),
                            r#type: Some(9), // TYPE_STRING
                            label: Some(1),  // LABEL_OPTIONAL
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                    prost_types::DescriptorProto {
                        name: Some("HelloReply".into()),
                        field: vec![prost_types::FieldDescriptorProto {
                            name: Some("message".into()),
                            number: Some(1),
                            r#type: Some(9),
                            label: Some(1),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                ],
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
                    method: vec![
                        prost_types::MethodDescriptorProto {
                            name: Some("SayHello".into()),
                            input_type: Some(".test.v1.HelloRequest".into()),
                            output_type: Some(".test.v1.HelloReply".into()),
                            ..Default::default()
                        },
                        prost_types::MethodDescriptorProto {
                            name: Some("SayGoodbye".into()),
                            input_type: Some(".test.v1.HelloRequest".into()),
                            output_type: Some(".test.v1.HelloReply".into()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                syntax: Some("proto3".into()),
                ..Default::default()
            }],
        };
        DescriptorPool::from_file_descriptor_set(fds).unwrap()
    }

    #[test]
    fn service_text() {
        let pool = make_pool();
        let svc = pool.get_service_by_name("test.v1.Greeter").unwrap();
        let text = format_service(&svc);
        assert!(text.contains("service Greeter {"));
        assert!(text.contains("rpc SayGoodbye"));
        assert!(text.contains("rpc SayHello"));
        // Methods should be sorted alphabetically
        let goodbye_pos = text.find("SayGoodbye").unwrap();
        let hello_pos = text.find("SayHello").unwrap();
        assert!(goodbye_pos < hello_pos);
    }

    #[test]
    fn message_text() {
        let pool = make_pool();
        let msg = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let text = format_message(&msg);
        assert_eq!(text, "message HelloRequest {\n  string name = 1;\n}");
    }

    #[test]
    fn method_text() {
        let pool = make_pool();
        let svc = pool.get_service_by_name("test.v1.Greeter").unwrap();
        let method = svc.methods().find(|m| m.name() == "SayHello").unwrap();
        let text = format_method(&method);
        assert_eq!(
            text,
            "rpc SayHello ( .test.v1.HelloRequest ) returns ( .test.v1.HelloReply );"
        );
    }

    #[test]
    fn enum_text() {
        let pool = make_pool();
        let e = pool.get_enum_by_name("test.v1.Status").unwrap();
        let text = format_enum(&e);
        assert!(text.contains("enum Status {"));
        assert!(text.contains("UNKNOWN = 0;"));
        assert!(text.contains("ACTIVE = 1;"));
    }

    #[test]
    fn field_text() {
        let pool = make_pool();
        let msg = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let field = msg.get_field_by_name("name").unwrap();
        let text = format_field(&field);
        assert_eq!(text, "string name = 1;");
    }

    #[test]
    fn enum_value_text() {
        let pool = make_pool();
        let e = pool.get_enum_by_name("test.v1.Status").unwrap();
        let val = e.get_value_by_name("ACTIVE").unwrap();
        let text = format_enum_value(&val);
        assert_eq!(text, "ACTIVE = 1;");
    }

    #[test]
    fn get_descriptor_text_dispatch() {
        let pool = make_pool();
        let svc = pool.get_service_by_name("test.v1.Greeter").unwrap();
        let sym = SymbolDescriptor::Service(svc);
        let text = get_descriptor_text(&sym);
        assert!(text.starts_with("service Greeter {"));
    }

    #[test]
    fn format_proto_file_output() {
        let pool = make_pool();
        let file = pool.get_file_by_name("test.proto").unwrap();
        let text = format_proto_file(&file);

        // Check syntax and package
        assert!(text.starts_with("syntax = \"proto3\";\n"));
        assert!(text.contains("package test.v1;\n"));

        // Check messages use short names (within same package)
        assert!(text.contains("message HelloRequest {"));
        assert!(text.contains("message HelloReply {"));

        // Check service preserves method order and uses short names
        assert!(text.contains("service Greeter {"));
        assert!(text.contains("rpc SayHello ( HelloRequest ) returns ( HelloReply );"));
        assert!(text.contains("rpc SayGoodbye ( HelloRequest ) returns ( HelloReply );"));

        // Check methods preserve original order (SayHello before SayGoodbye)
        let hello_pos = text.find("rpc SayHello").unwrap();
        let goodbye_pos = text.find("rpc SayGoodbye").unwrap();
        assert!(hello_pos < goodbye_pos);

        // Check enum with blank lines between values
        assert!(text.contains("enum Status {"));
        assert!(text.contains("UNKNOWN = 0;"));
        assert!(text.contains("ACTIVE = 1;"));
    }

    #[test]
    fn short_name_same_package() {
        assert_eq!(
            short_name("test.v1.HelloRequest", "test.v1"),
            "HelloRequest"
        );
        assert_eq!(short_name("other.pkg.Foo", "test.v1"), ".other.pkg.Foo");
        assert_eq!(short_name("Foo", ""), "Foo");
    }
}
