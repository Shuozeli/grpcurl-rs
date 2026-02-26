use std::cell::Cell;
use std::fmt;
use std::io::{self, Read};
use std::str::FromStr;

use prost_reflect::{DeserializeOptions, DynamicMessage, MessageDescriptor, SerializeOptions};

use crate::error::{GrpcurlError, Result};

/// Format for request/response data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Text,
}

impl FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "json" => Ok(Format::Json),
            "text" => Ok(Format::Text),
            other => Err(format!(
                "The --format option must be 'json' or 'text', got '{other}'."
            )),
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Format::Json => write!(f, "json"),
            Format::Text => write!(f, "text"),
        }
    }
}

/// Options controlling request parsing and response formatting.
///
/// Equivalent to Go's `FormatOptions` (format.go:380-398).
#[derive(Debug, Clone, Default)]
pub struct FormatOptions {
    /// Include fields with default values in JSON output.
    /// Maps to prost-reflect's `skip_default_fields(!emit_defaults)`.
    pub emit_defaults: bool,

    /// Accept unknown fields in JSON input without error.
    /// Maps to prost-reflect's `deny_unknown_fields(!allow_unknown)`.
    pub allow_unknown_fields: bool,
}

/// Parse error indicating end of input.
#[derive(Debug)]
pub enum ParseError {
    /// No more messages to parse.
    Eof,
    /// A parse error occurred.
    Error(GrpcurlError),
}

impl From<GrpcurlError> for ParseError {
    fn from(err: GrpcurlError) -> Self {
        ParseError::Error(err)
    }
}

/// Stream-based request message parser.
///
/// Equivalent to Go's `RequestParser` interface (format.go:24-33).
/// Reads one message at a time from the input, supporting multiple
/// concatenated messages (separated by whitespace).
pub struct JsonRequestParser {
    data: String,
    offset: usize,
    num_requests: usize,
    options: DeserializeOptions,
}

impl JsonRequestParser {
    /// Create a new JSON request parser from the input data.
    ///
    /// If `data` is "@", reads from stdin. Otherwise uses the string directly.
    pub fn new(data: Option<&str>, options: &FormatOptions) -> Result<Self> {
        let input = match data {
            Some("@") => {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf).map_err(|e| {
                    GrpcurlError::Io(io::Error::new(e.kind(), format!("reading stdin: {e}")))
                })?;
                buf
            }
            Some(s) => s.to_string(),
            None => String::new(),
        };

        let de_options =
            DeserializeOptions::new().deny_unknown_fields(!options.allow_unknown_fields);

        Ok(JsonRequestParser {
            data: input,
            offset: 0,
            num_requests: 0,
            options: de_options,
        })
    }

    /// Parse the next message from the input stream.
    ///
    /// Returns `ParseError::Eof` when there are no more messages.
    /// Multiple JSON objects can be concatenated with whitespace between them.
    pub fn next(
        &mut self,
        desc: &MessageDescriptor,
    ) -> std::result::Result<DynamicMessage, ParseError> {
        // Skip whitespace
        let remaining = &self.data[self.offset..];
        let trimmed = remaining.trim_start();
        if trimmed.is_empty() {
            return Err(ParseError::Eof);
        }

        // Update offset past whitespace
        self.offset += remaining.len() - trimmed.len();

        // Use serde_json's stream deserializer to read exactly one JSON value
        let mut de = serde_json::Deserializer::from_str(trimmed).into_iter::<serde_json::Value>();

        match de.next() {
            Some(Ok(value)) => {
                // Advance our offset by the bytes consumed
                let bytes_consumed = de.byte_offset();
                self.offset += bytes_consumed;
                self.num_requests += 1;

                // Deserialize the JSON value into a DynamicMessage
                let msg =
                    DynamicMessage::deserialize_with_options(desc.clone(), value, &self.options)
                        .map_err(|e| {
                            ParseError::Error(GrpcurlError::Proto(format!(
                                "failed to parse JSON request: {e}"
                            )))
                        })?;

                Ok(msg)
            }
            Some(Err(e)) => Err(ParseError::Error(GrpcurlError::Proto(format!(
                "invalid JSON in request data: {e}"
            )))),
            None => Err(ParseError::Eof),
        }
    }

    /// Return the number of messages parsed so far.
    pub fn num_requests(&self) -> usize {
        self.num_requests
    }
}

/// Protobuf text format request parser.
///
/// Equivalent to Go's `textRequestParser` (format.go:84-88).
/// Messages are separated by the 0x1E record separator character.
pub struct TextRequestParser {
    data: String,
    offset: usize,
    num_requests: usize,
}

impl TextRequestParser {
    /// Create a new text format request parser from the input data.
    ///
    /// If `data` is "@", reads from stdin. Otherwise uses the string directly.
    pub fn new(data: Option<&str>) -> Result<Self> {
        let input = match data {
            Some("@") => {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf).map_err(|e| {
                    GrpcurlError::Io(io::Error::new(e.kind(), format!("reading stdin: {e}")))
                })?;
                buf
            }
            Some(s) => s.to_string(),
            None => String::new(),
        };

        Ok(TextRequestParser {
            data: input,
            offset: 0,
            num_requests: 0,
        })
    }

    /// Parse the next message from the input stream.
    ///
    /// Messages are separated by the 0x1E record separator character.
    /// Returns `ParseError::Eof` when there are no more messages.
    ///
    /// Matches Go behavior: on the first call with empty input, returns an
    /// empty DynamicMessage (empty text is a valid empty proto message).
    /// Subsequent calls return `ParseError::Eof`.
    pub fn next(
        &mut self,
        desc: &MessageDescriptor,
    ) -> std::result::Result<DynamicMessage, ParseError> {
        let remaining = &self.data[self.offset..];
        if remaining.trim().is_empty() {
            // On the very first call, empty input produces one empty message
            // (matching Go's text parser semantics).
            if self.num_requests == 0 {
                self.offset = self.data.len();
                self.num_requests += 1;
                return Ok(DynamicMessage::new(desc.clone()));
            }
            return Err(ParseError::Eof);
        }

        // Read until 0x1E separator or end of input
        let (text, consumed) = if let Some(pos) = remaining.find('\x1e') {
            (&remaining[..pos], pos + 1)
        } else {
            (remaining, remaining.len())
        };

        let text = text.trim();
        if text.is_empty() {
            self.offset += consumed;
            // Empty segment on first read still produces one empty message
            if self.num_requests == 0 {
                self.num_requests += 1;
                return Ok(DynamicMessage::new(desc.clone()));
            }
            return Err(ParseError::Eof);
        }

        self.offset += consumed;
        self.num_requests += 1;

        DynamicMessage::parse_text_format(desc.clone(), text).map_err(|e| {
            ParseError::Error(GrpcurlError::Proto(format!(
                "failed to parse text format request: {e}"
            )))
        })
    }

    /// Return the number of messages parsed so far.
    pub fn num_requests(&self) -> usize {
        self.num_requests
    }
}

/// Unified request parser that dispatches to the appropriate format.
///
/// This enum wraps either a JSON or text format parser, providing a
/// common interface for the invocation engine.
pub enum RequestParser {
    Json(JsonRequestParser),
    Text(TextRequestParser),
}

impl RequestParser {
    /// Parse the next message from the input stream.
    pub fn next(
        &mut self,
        desc: &MessageDescriptor,
    ) -> std::result::Result<DynamicMessage, ParseError> {
        match self {
            RequestParser::Json(p) => p.next(desc),
            RequestParser::Text(p) => p.next(desc),
        }
    }

    /// Return the number of messages parsed so far.
    pub fn num_requests(&self) -> usize {
        match self {
            RequestParser::Json(p) => p.num_requests(),
            RequestParser::Text(p) => p.num_requests(),
        }
    }
}

/// Create a template DynamicMessage with default values for all fields.
///
/// Equivalent to Go's `MakeTemplate()` (grpcurl.go:396-510).
///
/// The template is useful for showing users what a valid JSON request
/// looks like. Scalar fields are left at defaults; repeated fields get
/// one default element; message fields are recursively populated.
pub fn make_template(desc: &MessageDescriptor) -> DynamicMessage {
    make_template_inner(desc, &mut Vec::new())
}

fn make_template_inner(desc: &MessageDescriptor, path: &mut Vec<String>) -> DynamicMessage {
    let full_name = desc.full_name().to_string();

    // Handle well-known types with special JSON representations.
    // Matches Go's MakeTemplate() (grpcurl.go:407-449).
    match full_name.as_str() {
        "google.protobuf.Any" => {
            let mut msg = DynamicMessage::new(desc.clone());
            if let Some(type_url_field) = desc.get_field_by_name("type_url") {
                msg.set_field(
                    &type_url_field,
                    prost_reflect::Value::String(
                        "type.googleapis.com/google.protobuf.Empty".into(),
                    ),
                );
            }
            // value field left as empty bytes (default), producing {"@type":"..."}
            return msg;
        }
        "google.protobuf.Value" => {
            // Value supports arbitrary JSON; provide a string hint
            let mut msg = DynamicMessage::new(desc.clone());
            if let Some(string_value_field) = desc.get_field_by_name("string_value") {
                msg.set_field(
                    &string_value_field,
                    prost_reflect::Value::String(
                        "google.protobuf.Value supports arbitrary JSON".into(),
                    ),
                );
            }
            return msg;
        }
        "google.protobuf.ListValue" => {
            // ListValue is a JSON array; provide one Value element
            let mut msg = DynamicMessage::new(desc.clone());
            if let Some(values_field) = desc.get_field_by_name("values") {
                let value_desc = match values_field.kind() {
                    prost_reflect::Kind::Message(m) => m,
                    _ => return msg,
                };
                let mut value_msg = DynamicMessage::new(value_desc.clone());
                if let Some(string_value_field) = value_desc.get_field_by_name("string_value") {
                    value_msg.set_field(
                        &string_value_field,
                        prost_reflect::Value::String(
                            "google.protobuf.Value supports arbitrary JSON".into(),
                        ),
                    );
                }
                msg.set_field(
                    &values_field,
                    prost_reflect::Value::List(vec![prost_reflect::Value::Message(value_msg)]),
                );
            }
            return msg;
        }
        "google.protobuf.Struct" => {
            // Struct is a JSON object; provide one key-value pair
            let mut msg = DynamicMessage::new(desc.clone());
            if let Some(fields_field) = desc.get_field_by_name("fields") {
                let entry_desc = match fields_field.kind() {
                    prost_reflect::Kind::Message(m) => m,
                    _ => return msg,
                };
                let value_field_desc = entry_desc.get_field(2);
                if let Some(value_field_desc) = value_field_desc {
                    let value_msg_desc = match value_field_desc.kind() {
                        prost_reflect::Kind::Message(m) => m,
                        _ => return msg,
                    };
                    let mut value_msg = DynamicMessage::new(value_msg_desc.clone());
                    if let Some(string_value_field) =
                        value_msg_desc.get_field_by_name("string_value")
                    {
                        value_msg.set_field(
                            &string_value_field,
                            prost_reflect::Value::String(
                                "google.protobuf.Struct supports arbitrary JSON objects".into(),
                            ),
                        );
                    }
                    let mut map = std::collections::HashMap::new();
                    map.insert(
                        prost_reflect::MapKey::String("key".into()),
                        prost_reflect::Value::Message(value_msg),
                    );
                    msg.set_field(&fields_field, prost_reflect::Value::Map(map));
                }
            }
            return msg;
        }
        _ => {}
    }

    // Cycle detection: if we've already seen this message type, return empty
    if path.contains(&full_name) {
        return DynamicMessage::new(desc.clone());
    }

    path.push(full_name);

    let mut msg = DynamicMessage::new(desc.clone());

    for field in desc.fields() {
        if field.is_map() {
            // Map field: add one entry with default key and value
            let kind = field.kind();
            let entry_desc = kind.as_message().expect("map field has message type");
            let key_field = entry_desc.get_field(1).expect("map entry has key field");
            let value_field = entry_desc.get_field(2).expect("map entry has value field");

            let key = default_map_key(&key_field);
            let value = if let prost_reflect::Kind::Message(value_desc) = value_field.kind() {
                prost_reflect::Value::Message(make_template_inner(&value_desc, path))
            } else {
                default_value_for_kind(&value_field)
            };

            let mut map = std::collections::HashMap::new();
            map.insert(key, value);
            msg.set_field(&field, prost_reflect::Value::Map(map));
        } else if field.is_list() {
            // Repeated field: add one default element
            let element = if let prost_reflect::Kind::Message(elem_desc) = field.kind() {
                prost_reflect::Value::Message(make_template_inner(&elem_desc, path))
            } else {
                default_value_for_kind(&field)
            };
            msg.set_field(&field, prost_reflect::Value::List(vec![element]));
        } else if let prost_reflect::Kind::Message(sub_desc) = field.kind() {
            // Non-repeated message field: recursively populate
            let sub_msg = make_template_inner(&sub_desc, path);
            msg.set_field(&field, prost_reflect::Value::Message(sub_msg));
        }
        // Scalar non-repeated fields: leave at defaults (emit_defaults will show them)
    }

    path.pop();
    msg
}

/// Return a default MapKey for a given field descriptor.
fn default_map_key(field: &prost_reflect::FieldDescriptor) -> prost_reflect::MapKey {
    use prost_reflect::Kind;
    match field.kind() {
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => prost_reflect::MapKey::I32(0),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => prost_reflect::MapKey::I64(0),
        Kind::Uint32 | Kind::Fixed32 => prost_reflect::MapKey::U32(0),
        Kind::Uint64 | Kind::Fixed64 => prost_reflect::MapKey::U64(0),
        Kind::Bool => prost_reflect::MapKey::Bool(false),
        Kind::String => prost_reflect::MapKey::String(String::new()),
        _ => prost_reflect::MapKey::I32(0),
    }
}

/// Return a default Value for a scalar field.
fn default_value_for_kind(field: &prost_reflect::FieldDescriptor) -> prost_reflect::Value {
    use prost_reflect::Kind;
    match field.kind() {
        Kind::Double => prost_reflect::Value::F64(0.0),
        Kind::Float => prost_reflect::Value::F32(0.0),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => prost_reflect::Value::I32(0),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => prost_reflect::Value::I64(0),
        Kind::Uint32 | Kind::Fixed32 => prost_reflect::Value::U32(0),
        Kind::Uint64 | Kind::Fixed64 => prost_reflect::Value::U64(0),
        Kind::Bool => prost_reflect::Value::Bool(false),
        Kind::String => prost_reflect::Value::String(String::new()),
        Kind::Bytes => prost_reflect::Value::Bytes(Default::default()),
        Kind::Enum(e) => {
            // Use first enum value (typically 0)
            prost_reflect::Value::EnumNumber(e.default_value().number())
        }
        Kind::Message(m) => prost_reflect::Value::Message(DynamicMessage::new(m)),
    }
}

/// Type alias for a response formatter function.
///
/// Equivalent to Go's `Formatter` type (format.go:129).
pub type Formatter = Box<dyn Fn(&DynamicMessage) -> Result<String>>;

/// Create a JSON response formatter.
///
/// Produces pretty-printed JSON with 2-space indentation.
/// If `emit_defaults` is true, includes fields with default/zero values.
///
/// Equivalent to Go's `NewJSONFormatter()` (format.go:137-157).
pub fn json_formatter(options: &FormatOptions) -> Formatter {
    let serialize_options = SerializeOptions::new()
        .skip_default_fields(!options.emit_defaults)
        .stringify_64_bit_integers(true);

    Box::new(move |msg: &DynamicMessage| {
        let mut buf = Vec::new();
        let mut serializer = serde_json::Serializer::pretty(&mut buf);

        msg.serialize_with_options(&mut serializer, &serialize_options)
            .map_err(|e| GrpcurlError::Proto(format!("failed to format response as JSON: {e}")))?;

        let json = String::from_utf8(buf)
            .map_err(|e| GrpcurlError::Proto(format!("JSON output is not valid UTF-8: {e}")))?;

        // Post-process to match Go's float formatting: strip trailing ".0" from
        // whole-valued doubles (e.g., "42.0" -> "42"). Go's encoding/json omits
        // the decimal point for whole numbers, while serde_json always includes it.
        Ok(normalize_json_floats(&json))
    })
}

/// Strip trailing ".0" from whole-valued JSON numbers to match Go's encoding/json.
///
/// Only modifies numeric values (not strings). Handles the pretty-printed
/// JSON format where numbers appear at the end of lines or before commas/brackets.
fn normalize_json_floats(json: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    // Match numbers like 42.0 that are NOT inside quotes.
    // This regex finds: digits followed by ".0" at a word boundary,
    // not preceded by another digit after the decimal (i.e., exactly ".0").
    static FLOAT_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m): (\d+)\.0([,\s\n\r\}\]]|$)").expect("float regex"));

    FLOAT_REGEX.replace_all(json, ": $1$2").into_owned()
}

/// Create a protobuf text format response formatter.
///
/// When `use_separator` is true, prepends a 0x1E record separator
/// character between messages (after the first).
///
/// Equivalent to Go's `NewTextFormatter()` (format.go:164-213).
pub fn text_formatter(use_separator: bool) -> Formatter {
    let num_formatted = Cell::new(0usize);

    Box::new(move |msg: &DynamicMessage| {
        let mut output = String::new();

        if use_separator && num_formatted.get() > 0 {
            output.push('\x1e');
        }

        // Use Display with alternate flag for pretty-printed (indented) text format.
        // prost-reflect uses curly braces {} (modern proto text format) while Go
        // uses angle brackets <> (legacy). Both are valid protobuf text format.
        let text = format!("{msg:#}");
        // Remove trailing newline (matching Go behavior)
        let text = text.trim_end_matches('\n');
        output.push_str(text);

        num_formatted.set(num_formatted.get() + 1);
        Ok(output)
    })
}

/// Map a tonic gRPC status code to its canonical name.
///
/// Equivalent to Go's `codes.Code.String()`.
pub fn status_code_name(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Ok => "OK",
        tonic::Code::Cancelled => "Canceled",
        tonic::Code::Unknown => "Unknown",
        tonic::Code::InvalidArgument => "InvalidArgument",
        tonic::Code::DeadlineExceeded => "DeadlineExceeded",
        tonic::Code::NotFound => "NotFound",
        tonic::Code::AlreadyExists => "AlreadyExists",
        tonic::Code::PermissionDenied => "PermissionDenied",
        tonic::Code::ResourceExhausted => "ResourceExhausted",
        tonic::Code::FailedPrecondition => "FailedPrecondition",
        tonic::Code::Aborted => "Aborted",
        tonic::Code::OutOfRange => "OutOfRange",
        tonic::Code::Unimplemented => "Unimplemented",
        tonic::Code::Internal => "Internal",
        tonic::Code::Unavailable => "Unavailable",
        tonic::Code::DataLoss => "DataLoss",
        tonic::Code::Unauthenticated => "Unauthenticated",
    }
}

/// Print a gRPC status to stderr in the standard format.
///
/// Equivalent to Go's `PrintStatus()` (format.go:517-554).
///
/// Format:
/// ```text
/// ERROR:
///   Code: <CODE_NAME>
///   Message: <message>
/// ```
pub fn print_status(status: &tonic::Status, formatter: Option<&Formatter>) {
    write_status(&mut io::stderr(), status, formatter);
}

/// Write a gRPC status to the given writer.
///
/// Allows callers to direct status output to any writer (stderr, buffer, etc.)
/// rather than hardcoding to stderr. The `print_status` function uses this
/// with `io::stderr()`.
pub fn write_status(w: &mut dyn io::Write, status: &tonic::Status, formatter: Option<&Formatter>) {
    if status.code() == tonic::Code::Ok {
        let _ = writeln!(w, "OK");
        return;
    }
    let _ = writeln!(w, "ERROR:");
    let _ = writeln!(w, "  Code: {}", status_code_name(status.code()));
    let _ = writeln!(w, "  Message: {}", status.message());

    // Parse status details from grpc-status-details-bin trailer.
    // This contains a serialized google.rpc.Status with Any-typed details.
    let details_bytes = status.details();
    if details_bytes.is_empty() {
        return;
    }

    // Decode as google.rpc.Status (manually, since prost_types doesn't include it).
    // The wire format is: field 1 (int32 code), field 2 (string message),
    // field 3 (repeated google.protobuf.Any details).
    // We only need the details field, so we decode the Any messages directly.
    let any_messages = decode_status_details(details_bytes);
    if any_messages.is_empty() {
        return;
    }

    for (i, any) in any_messages.iter().enumerate() {
        if i == 0 {
            let _ = writeln!(w, "  Details:");
        }
        // Try to format the Any message using the formatter if available
        let formatted = formatter.and_then(|fmt| format_any_detail(any, fmt).ok());

        if let Some(text) = formatted {
            let _ = writeln!(w, "  - {}", any.type_url);
            for line in text.lines() {
                let _ = writeln!(w, "      {line}");
            }
        } else {
            // Fallback: show type URL and raw base64 value
            let _ = writeln!(w, "  - {} ({} bytes)", any.type_url, any.value.len());
        }
    }
}

/// Decode the details field (field 3, repeated Any) from a serialized google.rpc.Status.
///
/// google.rpc.Status wire format:
///   field 1: int32 code
///   field 2: string message
///   field 3: repeated google.protobuf.Any
///
/// google.protobuf.Any wire format:
///   field 1: string type_url
///   field 2: bytes value
fn decode_status_details(data: &[u8]) -> Vec<prost_types::Any> {
    use prost::Message;

    // Use prost's low-level decoding by defining the Status message structure
    #[derive(Message, Clone)]
    struct RpcStatus {
        #[prost(int32, tag = "1")]
        _code: i32,
        #[prost(string, tag = "2")]
        _message: String,
        #[prost(message, repeated, tag = "3")]
        details: Vec<prost_types::Any>,
    }

    match RpcStatus::decode(data) {
        Ok(status) => status.details,
        Err(_) => Vec::new(),
    }
}

/// Attempt to format an Any-typed detail message as JSON.
///
/// Uses a well-known types descriptor pool to decode common error detail types
/// like google.rpc.ErrorInfo, google.rpc.BadRequest, etc.
fn format_any_detail(
    any: &prost_types::Any,
    formatter: &Formatter,
) -> std::result::Result<String, Box<dyn std::error::Error>> {
    // Extract the message type name from the type_url
    let type_name = any
        .type_url
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(&any.type_url);

    // Try to find the message type in a pool with well-known types
    let pool = prost_reflect::DescriptorPool::global();
    let msg_desc = pool.get_message_by_name(type_name).ok_or("unknown type")?;

    let msg = DynamicMessage::decode(msg_desc, any.value.as_slice())
        .map_err(|e| format!("failed to decode detail: {e}"))?;

    (formatter)(&msg).map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_reflect::DescriptorPool;

    fn make_pool() -> DescriptorPool {
        let fds = prost_types::FileDescriptorSet {
            file: vec![prost_types::FileDescriptorProto {
                name: Some("test.proto".into()),
                package: Some("test.v1".into()),
                message_type: vec![prost_types::DescriptorProto {
                    name: Some("HelloRequest".into()),
                    field: vec![
                        prost_types::FieldDescriptorProto {
                            name: Some("name".into()),
                            number: Some(1),
                            r#type: Some(9), // TYPE_STRING
                            label: Some(1),  // LABEL_OPTIONAL
                            json_name: Some("name".into()),
                            ..Default::default()
                        },
                        prost_types::FieldDescriptorProto {
                            name: Some("count".into()),
                            number: Some(2),
                            r#type: Some(5), // TYPE_INT32
                            label: Some(1),
                            json_name: Some("count".into()),
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
    fn parse_single_json_message() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let opts = FormatOptions::default();
        let mut parser =
            JsonRequestParser::new(Some(r#"{"name": "world", "count": 42}"#), &opts).unwrap();

        let msg = parser.next(&desc).unwrap();
        assert_eq!(parser.num_requests(), 1);

        // Verify fields
        let name_field = desc.get_field_by_name("name").unwrap();
        let name_val = msg.get_field(&name_field);
        assert_eq!(name_val.as_str(), Some("world"));

        let count_field = desc.get_field_by_name("count").unwrap();
        let count_val = msg.get_field(&count_field);
        assert_eq!(count_val.as_i32(), Some(42));
    }

    #[test]
    fn parse_multiple_json_messages() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let opts = FormatOptions::default();
        let mut parser =
            JsonRequestParser::new(Some(r#"{"name": "first"} {"name": "second"}"#), &opts).unwrap();

        let msg1 = parser.next(&desc).unwrap();
        let name1 = msg1.get_field(&desc.get_field_by_name("name").unwrap());
        assert_eq!(name1.as_str(), Some("first"));

        let msg2 = parser.next(&desc).unwrap();
        let name2 = msg2.get_field(&desc.get_field_by_name("name").unwrap());
        assert_eq!(name2.as_str(), Some("second"));

        assert!(matches!(parser.next(&desc), Err(ParseError::Eof)));
        assert_eq!(parser.num_requests(), 2);
    }

    #[test]
    fn parse_empty_input() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let opts = FormatOptions::default();
        let mut parser = JsonRequestParser::new(None, &opts).unwrap();

        assert!(matches!(parser.next(&desc), Err(ParseError::Eof)));
        assert_eq!(parser.num_requests(), 0);
    }

    #[test]
    fn format_json_without_defaults() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let opts = FormatOptions {
            emit_defaults: false,
            ..Default::default()
        };
        let formatter = json_formatter(&opts);

        let mut msg = DynamicMessage::new(desc.clone());
        let name_field = desc.get_field_by_name("name").unwrap();
        msg.set_field(&name_field, prost_reflect::Value::String("world".into()));

        let output = (formatter)(&msg).unwrap();
        assert!(output.contains("\"name\": \"world\""));
        // count field has default value 0 and should be skipped
        assert!(!output.contains("count"));
    }

    #[test]
    fn format_json_with_defaults() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let opts = FormatOptions {
            emit_defaults: true,
            ..Default::default()
        };
        let formatter = json_formatter(&opts);

        let mut msg = DynamicMessage::new(desc.clone());
        let name_field = desc.get_field_by_name("name").unwrap();
        msg.set_field(&name_field, prost_reflect::Value::String("world".into()));

        let output = (formatter)(&msg).unwrap();
        assert!(output.contains("\"name\": \"world\""));
        // count field should now be included with default value
        assert!(output.contains("\"count\""));
    }

    #[test]
    fn parse_unknown_fields_rejected_by_default() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let opts = FormatOptions::default();
        let mut parser =
            JsonRequestParser::new(Some(r#"{"name": "test", "unknown_field": 42}"#), &opts)
                .unwrap();

        let result = parser.next(&desc);
        assert!(matches!(result, Err(ParseError::Error(_))));
    }

    #[test]
    fn parse_unknown_fields_allowed() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let opts = FormatOptions {
            allow_unknown_fields: true,
            ..Default::default()
        };
        let mut parser =
            JsonRequestParser::new(Some(r#"{"name": "test", "unknown_field": 42}"#), &opts)
                .unwrap();

        let msg = parser.next(&desc).unwrap();
        let name_val = msg.get_field(&desc.get_field_by_name("name").unwrap());
        assert_eq!(name_val.as_str(), Some("test"));
    }

    #[test]
    fn parse_text_format_single_message() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let mut parser = TextRequestParser::new(Some("name: \"world\" count: 42")).unwrap();

        let msg = parser.next(&desc).unwrap();
        assert_eq!(parser.num_requests(), 1);

        let name_val = msg.get_field(&desc.get_field_by_name("name").unwrap());
        assert_eq!(name_val.as_str(), Some("world"));

        let count_val = msg.get_field(&desc.get_field_by_name("count").unwrap());
        assert_eq!(count_val.as_i32(), Some(42));
    }

    #[test]
    fn parse_text_format_multiple_with_separator() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let mut parser =
            TextRequestParser::new(Some("name: \"first\"\x1ename: \"second\"")).unwrap();

        let msg1 = parser.next(&desc).unwrap();
        let name1 = msg1.get_field(&desc.get_field_by_name("name").unwrap());
        assert_eq!(name1.as_str(), Some("first"));

        let msg2 = parser.next(&desc).unwrap();
        let name2 = msg2.get_field(&desc.get_field_by_name("name").unwrap());
        assert_eq!(name2.as_str(), Some("second"));

        assert!(matches!(parser.next(&desc), Err(ParseError::Eof)));
        assert_eq!(parser.num_requests(), 2);
    }

    #[test]
    fn parse_text_format_empty_input() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let mut parser = TextRequestParser::new(None).unwrap();

        // First call with empty input returns an empty message (matching Go behavior)
        let msg = parser.next(&desc).unwrap();
        assert_eq!(parser.num_requests(), 1);
        // Verify it's an empty/default message
        let name_val = msg.get_field(&desc.get_field_by_name("name").unwrap());
        assert_eq!(name_val.as_str(), Some(""));

        // Second call returns Eof
        assert!(matches!(parser.next(&desc), Err(ParseError::Eof)));
        assert_eq!(parser.num_requests(), 1);
    }

    #[test]
    fn format_text_output() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let formatter = text_formatter(false);

        let mut msg = DynamicMessage::new(desc.clone());
        let name_field = desc.get_field_by_name("name").unwrap();
        msg.set_field(&name_field, prost_reflect::Value::String("world".into()));

        let output = (formatter)(&msg).unwrap();
        assert!(output.contains("name"));
        assert!(output.contains("world"));
    }

    #[test]
    fn format_text_with_separator() {
        let pool = make_pool();
        let desc = pool.get_message_by_name("test.v1.HelloRequest").unwrap();
        let formatter = text_formatter(true);

        let mut msg1 = DynamicMessage::new(desc.clone());
        let name_field = desc.get_field_by_name("name").unwrap();
        msg1.set_field(&name_field, prost_reflect::Value::String("first".into()));

        let mut msg2 = DynamicMessage::new(desc.clone());
        msg2.set_field(&name_field, prost_reflect::Value::String("second".into()));

        let out1 = (formatter)(&msg1).unwrap();
        assert!(!out1.contains('\x1e')); // No separator for first message

        let out2 = (formatter)(&msg2).unwrap();
        assert!(out2.starts_with('\x1e')); // Separator for subsequent messages
    }
}
