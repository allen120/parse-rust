//! Format string compiler.
//!
//! Compiles Python-style format strings (e.g., "Hello {name:d}!")
//! into regular expressions for matching against input strings.
//! This is the core of the parse library - it translates the
//! format mini-language into regex capture groups.

use crate::types::FormatType;
use std::collections::HashMap;

#[derive(Debug)]
pub enum EvaluateError {
    RepeatedNameMismatch(String),
}

impl EvaluateError {
    pub fn into_message(self) -> String {
        match self {
            EvaluateError::RepeatedNameMismatch(message) => message,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomTypePattern {
    pub pattern: String,
    pub regex_group_count: usize,
}

/// Characters that have special meaning in regex and need escaping.
const REGEX_SPECIAL: &[char] = &[
    '.', '^', '$', '*', '+', '?', '(', ')', '[', ']', '{', '}', '|', '\\',
];

/// A parsed format specification from a format field.
///
/// Represents the components of a format spec like `{name:<10.2f}`:
/// - fill: padding character (default space)
/// - align: alignment direction (<, >, ^, =)
/// - sign: sign display mode (+, -, space)
/// - zero_pad: whether 0-padding is enabled
/// - width: minimum field width
/// - grouping: thousands grouping character (_, ,)
/// - precision: decimal precision
/// - format_type: the type specifier
#[derive(Debug, Clone)]
pub struct FormatSpec {
    pub fill: Option<char>,
    pub align: Option<char>,
    pub sign: Option<char>,
    pub zero_pad: bool,
    pub width: Option<usize>,
    pub grouping: Option<char>,
    pub precision: Option<usize>,
    pub format_string: String,
    pub type_name: String,
    pub format_type: FormatType,
}

impl Default for FormatSpec {
    fn default() -> Self {
        Self {
            fill: None,
            align: None,
            sign: None,
            zero_pad: false,
            width: None,
            grouping: None,
            precision: None,
            format_string: String::new(),
            type_name: String::new(),
            format_type: FormatType::Default,
        }
    }
}

/// A compiled field from the format string.
///
/// Each `{...}` in the format string becomes a FieldInfo after compilation.
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// The field name (empty string for anonymous/positional fields).
    pub name: String,
    /// The regex group name used to extract this field, if named.
    pub group_name: Option<String>,
    /// Whether this is a named field (vs positional).
    pub is_named: bool,
    /// If set, this field must equal an earlier named field.
    pub repeated_of: Option<String>,
    /// The positional index for anonymous fields.
    pub index: Option<usize>,
    /// The parsed format specification.
    pub spec: FormatSpec,
    /// The format type for this field.
    pub format_type: FormatType,
    /// The custom type name, if this field is backed by a Python converter.
    pub custom_type_name: Option<String>,
    /// Number of extra regex groups consumed by this field.
    pub extra_group_count: usize,
}

/// Result of compiling a format string.
///
/// Contains the generated regex pattern, field metadata, and
/// mappings needed for result extraction after matching.
#[derive(Debug)]
pub struct CompiledFormat {
    /// The compiled regex pattern string.
    pub pattern: String,
    /// Ordered list of field information.
    pub fields: Vec<FieldInfo>,
    /// Map from regex group name to field name.
    pub group_to_field: HashMap<String, String>,
    /// Map from field name to regex group name.
    pub field_to_group: HashMap<String, String>,
    /// Names of named fields, in order.
    pub named_fields: Vec<String>,
    /// Indices of positional (fixed) fields.
    pub fixed_fields: Vec<usize>,
    /// Total number of capture groups in the pattern.
    pub group_count: usize,
    /// Regex capture group index for each positional field, in positional order.
    pub fixed_group_indices: Vec<usize>,
}

/// Compile a format string into a regex pattern and field metadata.
///
/// This is the main entry point for the compiler. It takes a format
/// string like `"Hello {name}! You are {:d} years old."` and produces
/// a CompiledFormat containing the regex and all metadata needed for
/// parsing.
///
/// # Arguments
/// * `format` - The format string to compile
/// * `case_sensitive` - Whether matching should be case-sensitive
///
/// # Returns
/// A CompiledFormat on success, or an error message on failure.
pub fn compile_format(format: &str, case_sensitive: bool) -> Result<CompiledFormat, String> {
    compile_format_with_custom(format, case_sensitive, &HashMap::new())
}

pub fn compile_format_with_custom(
    format: &str,
    case_sensitive: bool,
    custom_types: &HashMap<String, CustomTypePattern>,
) -> Result<CompiledFormat, String> {
    let mut compiler = FormatCompiler::new(case_sensitive);
    compiler.compile(format, custom_types)
}

/// Internal compiler state for format string compilation.
struct FormatCompiler {
    case_sensitive: bool,
    group_index: usize,
    fields: Vec<FieldInfo>,
    group_to_field: HashMap<String, String>,
    field_to_group: HashMap<String, String>,
    named_fields: Vec<String>,
    fixed_fields: Vec<usize>,
    fixed_count: usize,
    used_group_names: HashMap<String, String>,
    name_types: HashMap<String, String>,
}

impl FormatCompiler {
    fn new(case_sensitive: bool) -> Self {
        Self {
            case_sensitive,
            group_index: 0,
            fields: Vec::new(),
            group_to_field: HashMap::new(),
            field_to_group: HashMap::new(),
            named_fields: Vec::new(),
            fixed_fields: Vec::new(),
            fixed_count: 0,
            used_group_names: HashMap::new(),
            name_types: HashMap::new(),
        }
    }

    /// Compile the format string into regex pattern and metadata.
    fn compile(
        &mut self,
        format: &str,
        custom_types: &HashMap<String, CustomTypePattern>,
    ) -> Result<CompiledFormat, String> {
        let parts = self.split_format(format);
        let mut pattern = String::new();

        if self.case_sensitive {
            pattern.push_str("(?s)");
        } else {
            pattern.push_str("(?is)");
        }

        for part in &parts {
            match part {
                FormatPart::Literal(text) => {
                    pattern.push_str(&escape_regex(text));
                }
                FormatPart::EscapedOpen => {
                    pattern.push_str(r"\{");
                }
                FormatPart::EscapedClose => {
                    pattern.push_str(r"\}");
                }
                FormatPart::Field(field_str) => {
                    let field_pattern = self.handle_field(field_str, custom_types)?;
                    pattern.push_str(&field_pattern);
                }
            }
        }

        Ok(CompiledFormat {
            pattern,
            fields: self.fields.clone(),
            group_to_field: self.group_to_field.clone(),
            field_to_group: self.field_to_group.clone(),
            named_fields: self.named_fields.clone(),
            fixed_fields: self.fixed_fields.clone(),
            group_count: self.group_index,
            fixed_group_indices: self
                .fields
                .iter()
                .scan(1usize, |group_idx, field| {
                    let current = *group_idx;
                    *group_idx += 1 + field.extra_group_count;
                    Some((field, current))
                })
                .filter_map(|(field, current)| (!field.is_named).then_some(current))
                .collect(),
        })
    }

    /// Split format string into literal text, escaped braces, and field specs.
    fn split_format(&self, format: &str) -> Vec<FormatPart> {
        let mut parts = Vec::new();
        let mut chars = format.chars().peekable();
        let mut current_literal = String::new();
        let mut unmatched_open = false;

        while let Some(ch) = chars.next() {
            match ch {
                '{' => {
                    if chars.peek() == Some(&'{') {
                        // Escaped opening brace: {{
                        if !current_literal.is_empty() {
                            parts.push(FormatPart::Literal(std::mem::take(&mut current_literal)));
                        }
                        chars.next();
                        parts.push(FormatPart::EscapedOpen);
                    } else {
                        // Start of field
                        if !current_literal.is_empty() {
                            parts.push(FormatPart::Literal(std::mem::take(&mut current_literal)));
                        }
                        let mut field = String::new();
                        let mut depth = 1;
                        let mut closed = false;
                        while let Some(fch) = chars.next() {
                            if fch == '{' {
                                depth += 1;
                                field.push(fch);
                            } else if fch == '}' {
                                depth -= 1;
                                if depth == 0 {
                                    closed = true;
                                    break;
                                }
                                field.push(fch);
                            } else {
                                field.push(fch);
                            }
                        }
                        if closed {
                            parts.push(FormatPart::Field(field));
                        } else {
                            unmatched_open = true;
                            current_literal.push('{');
                            current_literal.push_str(&field);
                            break;
                        }
                    }
                }
                '}' => {
                    if chars.peek() == Some(&'}') {
                        // Escaped closing brace: }}
                        if !current_literal.is_empty() {
                            parts.push(FormatPart::Literal(std::mem::take(&mut current_literal)));
                        }
                        chars.next();
                        parts.push(FormatPart::EscapedClose);
                    } else {
                        current_literal.push(ch);
                    }
                }
                _ => {
                    current_literal.push(ch);
                }
            }
        }

        if !current_literal.is_empty() {
            parts.push(FormatPart::Literal(current_literal));
        }

        if unmatched_open {
            parts.clear();
            parts.push(FormatPart::Literal(format.to_string()));
        }

        parts
    }

    /// Process a single field specification and return its regex pattern.
    ///
    /// Handles both named fields (`{name:d}`) and positional fields (`{:d}` or `{}`).
    fn handle_field(
        &mut self,
        field_str: &str,
        custom_types: &HashMap<String, CustomTypePattern>,
    ) -> Result<String, String> {
        // Split into name and format spec
        let (name, format_spec_str) = if let Some(colon_pos) = field_str.find(':') {
            (
                field_str[..colon_pos].to_string(),
                &field_str[colon_pos + 1..],
            )
        } else {
            (field_str.to_string(), "")
        };

        // Parse the format specification
        let spec = parse_format_spec_with_custom(format_spec_str, custom_types)?;
        let spec_text = spec.format_string.clone();
        let custom_type_name = custom_types
            .contains_key(&spec.type_name)
            .then(|| spec.type_name.clone());

        // Determine if named or positional
        let is_named = !name.is_empty()
            && name
                .chars()
                .next()
                .map_or(false, |c| c.is_alphabetic() || c == '_');

        let group_name = if is_named {
            if let Some(existing_type) = self.name_types.get(&name) {
                if existing_type != &spec_text {
                    return Err(format!(
                        "RepeatedNameError: field type '{}' for field '{}' does not match previous type '{}'",
                        spec_text, name, existing_type
                    ));
                }
                let repeated_group = self
                    .to_group_name(&format!("{}_repeat_{}", name, self.group_index))?;
                let field_info = FieldInfo {
                    name: name.clone(),
                    group_name: Some(repeated_group.clone()),
                    is_named: true,
                    repeated_of: Some(name.clone()),
                    index: None,
                    spec: spec.clone(),
                    format_type: spec.format_type.clone(),
                    custom_type_name: custom_type_name.clone(),
                    extra_group_count: 0,
                };
                self.fields.push(field_info);

                let (type_pattern, extra_groups) =
                    build_field_pattern(&spec, Some(repeated_group.as_str()), custom_types);
                if let Some(last) = self.fields.last_mut() {
                    last.extra_group_count = extra_groups;
                }

                self.group_index += 1 + extra_groups;
                return Ok(type_pattern);
            }
            let gn = self.to_group_name(&name)?;
            self.name_types.insert(name.clone(), spec_text);
            self.named_fields.push(gn.clone());
            self.group_to_field.insert(gn.clone(), name.clone());
            self.field_to_group.insert(name.clone(), gn.clone());
            Some(gn)
        } else {
            let index = self.fixed_count;
            self.fixed_count += 1;
            self.fixed_fields.push(index);
            None
        };

        let field_info = FieldInfo {
            name: name.clone(),
            group_name: group_name.clone(),
            is_named,
            repeated_of: None,
            index: if is_named {
                None
            } else {
                Some(self.fixed_count - 1)
            },
            spec: spec.clone(),
            format_type: spec.format_type.clone(),
            custom_type_name,
            extra_group_count: 0,
        };
        self.fields.push(field_info);

        // Get the regex pattern for this type
        let (pattern, extra_groups) =
            build_field_pattern(&spec, group_name.as_deref(), custom_types);
        if let Some(last) = self.fields.last_mut() {
            last.extra_group_count = extra_groups;
        }

        self.group_index += 1 + extra_groups;

        Ok(pattern)
    }

    /// Convert a field name to a valid regex group name.
    ///
    /// Replaces characters not valid in regex group names
    /// (dots, brackets, hyphens) with underscores, and handles
    /// name collisions by appending underscores.
    fn to_group_name(&mut self, field_name: &str) -> Result<String, String> {
        if field_name.contains('\'') || field_name.contains('"') {
            return Err(format!(
                "NotImplementedError: Group names (e.g. (?P<name>) can cause failure, as they are not escaped properly: '{}'",
                format!("\\A(?P<{}>.+?)\\Z", field_name.replace('[', "_").replace(']', "_"))
            ));
        }
        let mut group_name = field_name
            .replace('.', "_")
            .replace('[', "_")
            .replace(']', "_")
            .replace('-', "_");

        let mut n = 1;
        while self.used_group_names.contains_key(&group_name)
            && self.used_group_names[&group_name] != field_name
        {
            n += 1;
            group_name = if field_name.contains('.') {
                field_name.replace('.', &"_".repeat(n))
            } else if field_name.contains('_') {
                field_name.replace('_', &"_".repeat(n))
            } else if field_name.contains('-') {
                field_name.replace('-', &"_".repeat(n))
            } else {
                format!("{}{}", group_name, "_")
            };
        }

        self.used_group_names
            .insert(group_name.clone(), field_name.to_string());
        Ok(group_name)
    }
}

fn build_numeric_core_pattern(spec: &FormatSpec, fmt_type: &FormatType) -> (String, usize) {
    match fmt_type {
        FormatType::Decimal => {
            if let Some(grouping) = spec.grouping {
                let grouped = match grouping {
                    ',' => r"\d{1,3}(?:,\d{3})*".to_string(),
                    '_' => r"\d{1,3}(?:_\d{3})*".to_string(),
                    _ => r"\d+".to_string(),
                };
                (
                    format!(
                        "(?:{grouped}|0[xX][0-9a-fA-F]+|0[bB][01]+|0[oO][0-7]+)",
                        grouped = grouped
                    ),
                    0,
                )
            } else {
                let width_pattern = if let Some(width) = spec.width {
                    format!("{{1,{}}}", width)
                } else {
                    "+".to_string()
                };

                (
                    format!(
                        "(?:[0-9]{w}|0[xX][0-9a-fA-F]+|0[bB][01]+|0[oO][0-7]+)",
                        w = width_pattern
                    ),
                    0,
                )
            }
        }
        _ => fmt_type.regex_pattern(),
    }
}

fn build_field_pattern(
    spec: &FormatSpec,
    group_name: Option<&str>,
    custom_types: &HashMap<String, CustomTypePattern>,
) -> (String, usize) {
    let fmt_type = &spec.format_type;
    let is_numeric = fmt_type.is_numeric();
    let (mut core_pattern, extra_groups) = if let Some(custom) = custom_types.get(&spec.type_name) {
        (custom.pattern.clone(), custom.regex_group_count)
    } else {
        match fmt_type {
            FormatType::Default => {
                if let Some(precision) = spec.precision {
                    if let Some(width) = spec.width {
                        (format!(".{{{},{}}}?", width, precision), 0)
                    } else {
                        (format!(".{{1,{}}}?", precision), 0)
                    }
                } else if let Some(width) = spec.width {
                    (format!(".{{{},}}?", width), 0)
                } else {
                    (".+?".to_string(), 0)
                }
            }
            _ if is_numeric => build_numeric_core_pattern(spec, fmt_type),
            _ => fmt_type.regex_pattern(),
        }
    };

    let mut align = spec.align;
    let fill = spec.fill.unwrap_or(' ');

    if is_numeric {
        let already_handles_sign = matches!(
            fmt_type,
            FormatType::NumberWithSeparator
                | FormatType::Scientific
                | FormatType::GeneralFloat
                | FormatType::Percentage
        );
        if align == Some('=') {
            let fill_pattern = escape_fill(fill);
            core_pattern = format!("[-+ ]?(?:{}*{})", fill_pattern, core_pattern);
        } else if !already_handles_sign {
            core_pattern = format!("[-+ ]?{}", core_pattern);
        }
    }

    if let Some(width) = spec.width {
        if align.is_none() {
            let _ = width;
            align = Some('>');
        }
    }

    let escaped_fill = escape_fill(fill);
    let wrapped = if let Some(group_name) = group_name {
        format!("(?P<{}>{})", group_name, core_pattern)
    } else {
        format!("({})", core_pattern)
    };
    let pattern = match align {
        Some('<') => format!("{}{}*", wrapped, escaped_fill),
        Some('>') => format!("{}*{}", escaped_fill, wrapped),
        Some('^') => format!("{}*{}{}*", escaped_fill, wrapped, escaped_fill),
        _ => wrapped,
    };

    (pattern, extra_groups)
}

fn escape_fill(fill: char) -> String {
    if r".\+?*[](){}^$".contains(fill) {
        format!("\\{}", fill)
    } else {
        fill.to_string()
    }
}

/// Parts of a split format string.
#[derive(Debug)]
enum FormatPart {
    Literal(String),
    EscapedOpen,
    EscapedClose,
    Field(String),
}

/// Parse a format specification string into a FormatSpec.
///
/// The format spec follows Python's format mini-language:
/// `[[fill]align][sign][#][0][width][grouping_option][.precision][type]`
///
/// Examples:
/// - `"d"` → type=Decimal
/// - `"<10s"` → align=<, width=10, type=NonWhitespace
/// - `".2f"` → precision=2, type=Float
pub fn parse_format_spec(spec: &str) -> Result<FormatSpec, String> {
    parse_format_spec_with_custom(spec, &HashMap::new())
}

pub fn parse_format_spec_with_custom(
    spec: &str,
    custom_types: &HashMap<String, CustomTypePattern>,
) -> Result<FormatSpec, String> {
    if spec.is_empty() {
        return Ok(FormatSpec::default());
    }

    let mut result = FormatSpec::default();
    result.format_string = spec.to_string();
    let chars: Vec<char> = spec.chars().collect();
    let len = chars.len();
    let mut pos = 0;

    // Check for fill and align
    // Align chars: <, >, ^, =
    if len >= 2 && is_align_char(chars[1]) {
        result.fill = Some(chars[0]);
        result.align = Some(chars[1]);
        pos = 2;
    } else if len >= 1 && is_align_char(chars[0]) {
        result.align = Some(chars[0]);
        pos = 1;
    }

    // Check for sign: +, -, space
    if pos < len && (chars[pos] == '+' || chars[pos] == '-' || chars[pos] == ' ') {
        result.sign = Some(chars[pos]);
        pos += 1;
    }

    // Check for zero-pad
    if pos < len && chars[pos] == '0' {
        result.zero_pad = true;
        pos += 1;
    }

    // Check for width (digits)
    let width_start = pos;
    while pos < len && chars[pos].is_ascii_digit() {
        pos += 1;
    }
    if pos > width_start {
        let width_str: String = chars[width_start..pos].iter().collect();
        result.width = Some(
            width_str
                .parse()
                .map_err(|_| format!("Invalid width: {}", width_str))?,
        );
    }

    // Check for grouping option (, or _)
    if pos < len && (chars[pos] == ',' || chars[pos] == '_') {
        result.grouping = Some(chars[pos]);
        pos += 1;
    }

    // Check for precision (.digits)
    if pos < len && chars[pos] == '.' {
        pos += 1;
        let prec_start = pos;
        while pos < len && chars[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos > prec_start {
            let prec_str: String = chars[prec_start..pos].iter().collect();
            result.precision = Some(
                prec_str
                    .parse()
                    .map_err(|_| format!("Invalid precision: {}", prec_str))?,
            );
        }
    }

    // Remaining characters form the type specifier
    if pos < len {
        let type_str: String = chars[pos..].iter().collect();
        result.type_name = type_str.clone();
        result.format_type = if let Some(fmt_type) = FormatType::from_str(&type_str) {
            fmt_type
        } else if custom_types.contains_key(&type_str) {
            FormatType::Default
        } else if type_str.contains('%') {
            FormatType::CustomDateTime(type_str)
        } else if type_str.is_empty() {
            FormatType::Default
        } else {
            return Err(format!("Unknown format type: '{}'", type_str));
        };
    }

    Ok(result)
}

/// Check if a character is an alignment specifier.
fn is_align_char(c: char) -> bool {
    matches!(c, '<' | '>' | '^' | '=')
}

/// Escape a string for use in a regex pattern.
///
/// All regex special characters are prefixed with a backslash.
pub fn escape_regex(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        if REGEX_SPECIAL.contains(&ch) {
            result.push('\\');
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_regex() {
        assert_eq!(escape_regex("hello"), "hello");
        assert_eq!(escape_regex("a.b"), r"a\.b");
        assert_eq!(escape_regex("a[0]"), r"a\[0\]");
        assert_eq!(escape_regex("$100"), r"\$100");
    }

    #[test]
    fn test_split_format_simple() {
        let compiler = FormatCompiler::new(false);
        let parts = compiler.split_format("Hello {}!");
        assert_eq!(parts.len(), 3); // "Hello ", field, "!"
    }

    #[test]
    fn test_split_format_named() {
        let compiler = FormatCompiler::new(false);
        let parts = compiler.split_format("User {name} from {city}");
        assert_eq!(parts.len(), 4); // "User ", field, " from ", field
    }

    #[test]
    fn test_split_format_escaped_braces() {
        let compiler = FormatCompiler::new(false);
        let parts = compiler.split_format("{{literal}}");
        // "{{" -> EscapedOpen, "literal" -> Literal, "}}" -> EscapedClose
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_parse_format_spec_zero_padded_decimal() {
        let spec = parse_format_spec("05d").unwrap();
        assert_eq!(spec.zero_pad, true);
        assert_eq!(spec.width, Some(5));
        assert_eq!(spec.type_name, "d");
        assert_eq!(spec.format_type, FormatType::Decimal);
    }

    #[test]
    fn test_parse_format_spec_compound_numeric_flags() {
        let spec = parse_format_spec(".^010d").unwrap();
        assert_eq!(spec.fill, Some('.'));
        assert_eq!(spec.align, Some('^'));
        assert_eq!(spec.zero_pad, true);
        assert_eq!(spec.width, Some(10));
        assert_eq!(spec.type_name, "d");
    }

    #[test]
    fn test_parse_format_spec_with_custom_type() {
        let mut custom_types = HashMap::new();
        custom_types.insert(
            "Number".to_string(),
            CustomTypePattern {
                pattern: r"\d+".to_string(),
                regex_group_count: 0,
            },
        );
        let spec = parse_format_spec_with_custom("Number", &custom_types).unwrap();
        assert_eq!(spec.type_name, "Number");
        assert_eq!(spec.format_type, FormatType::Default);
    }

    #[test]
    fn test_parse_format_spec_unknown_type_errors() {
        let err = parse_format_spec("zzz").unwrap_err();
        assert!(err.contains("Unknown format type"));
    }

    #[test]
    fn test_parse_format_spec_type_only() {
        let spec = parse_format_spec("d").unwrap();
        assert_eq!(spec.format_type, FormatType::Decimal);
    }

    #[test]
    fn test_parse_format_spec_with_width() {
        let spec = parse_format_spec("10d").unwrap();
        assert_eq!(spec.format_type, FormatType::Decimal);
        assert_eq!(spec.width, Some(10));
    }

    #[test]
    fn test_parse_format_spec_full() {
        let spec = parse_format_spec("<10.2f").unwrap();
        assert_eq!(spec.align, Some('<'));
        assert_eq!(spec.width, Some(10));
        assert_eq!(spec.precision, Some(2));
        assert_eq!(spec.format_type, FormatType::Float);
    }

    #[test]
    fn test_parse_format_spec_alignment_without_type() {
        let centered = parse_format_spec("^2").unwrap();
        assert_eq!(centered.align, Some('^'));
        assert_eq!(centered.width, Some(2));
        assert_eq!(centered.format_type, FormatType::Default);

        let left = parse_format_spec("<2").unwrap();
        assert_eq!(left.align, Some('<'));
        assert_eq!(left.width, Some(2));
        assert_eq!(left.format_type, FormatType::Default);
    }

    #[test]
    fn test_build_field_pattern_float_keeps_optional_sign() {
        let spec = parse_format_spec("f").unwrap();
        let (pattern, extra_groups) = build_field_pattern(&spec, None, &HashMap::new());
        assert_eq!(extra_groups, 0);
        assert_eq!(pattern, r"([-+ ]?\d*\.\d+)");
    }

    #[test]
    fn test_compile_simple() {
        let result = compile_format("Hello {}!", false).unwrap();
        assert_eq!(result.fixed_fields.len(), 1);
        assert!(result.named_fields.is_empty());
    }

    #[test]
    fn test_compile_named() {
        let result = compile_format("User {name} from {city}", false).unwrap();
        assert_eq!(result.named_fields.len(), 2);
        assert!(result.named_fields.contains(&"name".to_string()));
        assert!(result.named_fields.contains(&"city".to_string()));
    }

    #[test]
    fn test_compile_typed() {
        let result = compile_format("{name:w} is {:d} years old", false).unwrap();
        assert_eq!(result.named_fields.len(), 1);
        assert_eq!(result.fixed_fields.len(), 1);
    }

    #[test]
    fn test_compile_mixed() {
        let result = compile_format(
            "User {name} performed {action} on {target} at {time}",
            false,
        )
        .unwrap();
        assert_eq!(result.named_fields.len(), 4);
        assert_eq!(result.fixed_fields.len(), 0);
    }

    #[test]
    fn test_compile_datetime() {
        let result = compile_format("Date: {:ti}", false).unwrap();
        assert_eq!(result.fixed_fields.len(), 1);
    }

    #[test]
    fn test_compile_escaped_braces() {
        let result = compile_format("{{not a field}}", false).unwrap();
        assert!(result.fields.is_empty());
        assert!(result.pattern.contains(r"\{"));
        assert!(result.pattern.contains(r"\}"));
    }
}
