//! Type conversion system for format specifiers.
//!
//! Handles conversion of matched strings to their target types
//! based on format specifiers like :d (integer), :f (float), etc.

use crate::compiler::FormatSpec;
use crate::result::ParseValue;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// All supported format type specifiers.
///
/// Each variant corresponds to a Python format mini-language type character
/// or a parse-specific extension (datetime types, etc.).
#[derive(Debug, Clone, PartialEq)]
pub enum FormatType {
    /// Default: match any text, return as string.
    Default,
    /// `s` - Non-whitespace string.
    NonWhitespace,
    /// `w` - Word characters (alphanumeric + underscore).
    Word,
    /// `W` - Non-word characters.
    NonWord,
    /// `d` - Decimal integer (supports sign, base prefixes 0x/0o/0b).
    Decimal,
    /// `b` - Binary integer.
    Binary,
    /// `o` - Octal integer.
    Octal,
    /// `x` - Hexadecimal integer.
    Hex,
    /// `n` - Number with thousands separator (e.g., 1,000,000).
    NumberWithSeparator,
    /// `f` - Fixed-point float.
    Float,
    /// `F` - Fixed-point decimal.
    FloatDecimal,
    /// `e` - Scientific notation float.
    Scientific,
    /// `g` - General float (either fixed or scientific).
    GeneralFloat,
    /// `%` - Percentage (parsed as float, divided by 100).
    Percentage,
    /// `l` - Letters only [A-Za-z]+.
    Letters,
    /// `S` - Whitespace characters.
    Whitespace,
    /// `D` - Non-digit characters.
    NonDigit,
    /// `ti` - ISO 8601 datetime.
    DateTimeISO,
    /// `te` - RFC 2822 email datetime.
    DateTimeEmail,
    /// `ta` - US-style datetime (MM/DD/YYYY).
    DateTimeUS,
    /// `tg` - Generic datetime (DD/MM/YYYY).
    DateTimeGeneric,
    /// `th` - HTTP log datetime.
    DateTimeHTTP,
    /// `tc` - C standard ctime() datetime.
    DateTimeCTime,
    /// `tt` - Time only (HH:MM:SS).
    TimeOnly,
    /// `ts` - Syslog datetime (Mon DD HH:MM:SS).
    DateTimeSyslog,
    /// Custom strftime/strptime-like datetime format.
    CustomDateTime(String),
}

impl FormatType {
    /// Parse a type character string into a FormatType.
    ///
    /// Returns None if the type string is not recognized.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "" => Some(FormatType::Default),
            "s" => Some(FormatType::NonWhitespace),
            "w" => Some(FormatType::Word),
            "W" => Some(FormatType::NonWord),
            "d" => Some(FormatType::Decimal),
            "b" => Some(FormatType::Binary),
            "o" => Some(FormatType::Octal),
            "x" => Some(FormatType::Hex),
            "n" => Some(FormatType::NumberWithSeparator),
            "f" => Some(FormatType::Float),
            "F" => Some(FormatType::FloatDecimal),
            "e" => Some(FormatType::Scientific),
            "g" => Some(FormatType::GeneralFloat),
            "%" => Some(FormatType::Percentage),
            "l" => Some(FormatType::Letters),
            "S" => Some(FormatType::Whitespace),
            "D" => Some(FormatType::NonDigit),
            "ti" => Some(FormatType::DateTimeISO),
            "te" => Some(FormatType::DateTimeEmail),
            "ta" => Some(FormatType::DateTimeUS),
            "tg" => Some(FormatType::DateTimeGeneric),
            "th" => Some(FormatType::DateTimeHTTP),
            "tc" => Some(FormatType::DateTimeCTime),
            "tt" => Some(FormatType::TimeOnly),
            "ts" => Some(FormatType::DateTimeSyslog),
            _ => None,
        }
    }

    /// Get the regex pattern for this format type.
    ///
    /// Returns a regex pattern string that matches valid input
    /// for this type, along with the number of capture groups
    /// in the pattern.
    pub fn regex_pattern(&self) -> (String, usize) {
        match self {
            FormatType::Default => (".*?".to_string(), 0),
            FormatType::NonWhitespace => (r".+".to_string(), 0),
            FormatType::Word => (r"\w+".to_string(), 0),
            FormatType::NonWord => (r"\W+".to_string(), 0),
            FormatType::Letters => ("[A-Za-z]+".to_string(), 0),
            FormatType::Whitespace => (r"\S+".to_string(), 0),
            FormatType::NonDigit => (r"\D+".to_string(), 0),
            FormatType::Decimal => (r"\d+".to_string(), 0),
            FormatType::Binary => (r"(0[bB])?[01]+".to_string(), 1),
            FormatType::Octal => (r"(0[oO])?[0-7]+".to_string(), 1),
            FormatType::Hex => (r"(0[xX])?[0-9a-fA-F]+".to_string(), 1),
            FormatType::NumberWithSeparator => (r"[-+ ]?\d{1,3}([,._]\d{3})*".to_string(), 1),
            FormatType::Float => (r"\d*\.\d+".to_string(), 0),
            FormatType::FloatDecimal => (r"\d*\.\d+".to_string(), 0),
            FormatType::Scientific => (
                r"[-+]?(?:\d*\.\d+[eE][-+]?\d+|nan|NAN|inf|INF)".to_string(),
                0,
            ),
            FormatType::GeneralFloat => (
                r"[-+]?(?:\d+(\.\d+)?([eE][-+]?\d+)?|nan|NAN|inf|INF)".to_string(),
                2,
            ),
            FormatType::Percentage => (r"[-+]?\d+(\.\d+)?%".to_string(), 1),
            FormatType::DateTimeISO => (
                r"\d{4}-\d\d-\d\d(?:[T ]\d\d:\d\d(?::\d\d(?:\.\d+)?)?(?:\s*(?:Z|[+-]\d\d:?\d\d))?)?".to_string(),
                0,
            ),
            FormatType::DateTimeEmail => (
                r"(?:\w{3},?\s+)?\d\d?\s+\w{3}\s+\d{4}\s+\d\d:\d\d(?::\d\d)?\s*(?:\w+|[+-]\d\d:?\d\d)?".to_string(),
                0,
            ),
            FormatType::DateTimeUS => (
                r"(?:\d\d?|\w+)[/-](?:\d\d?|\w+)[/-]\d{4}(?:\s+\d\d:\d\d(?::\d\d)?\s*(?:[AP]M)?\s*(?:\w+|[+-]\d\d?:?\d\d)?)?".to_string(),
                0,
            ),
            FormatType::DateTimeGeneric => (
                r"\d\d?[-/](?:\d\d?|\w+)[-/]\d{4}(?:\s+\d\d:\d\d(?::\d\d)?\s*(?:[AP]M)?\s*(?:\w+|[+-]\d\d?:?\d\d)?)?".to_string(),
                0,
            ),
            FormatType::DateTimeHTTP => (
                r"\d\d?/\w+/\d{4}:\d\d:\d\d:\d\d\s*(?:\w+|[+-]\d\d:?\d\d)?".to_string(),
                0,
            ),
            FormatType::DateTimeCTime => (
                r"\w{3}\s+\w{3}\s+\d\d?\s+\d\d:\d\d:\d\d\s+\d{4}".to_string(),
                0,
            ),
            FormatType::TimeOnly => (
                r"\d{1,2}:\d{1,2}(?::\d{1,2}(?:\.\d+)?)?\s*(?:[AP]M)?\s*(?:Z|[+-]\d\d?:?\d\d)?".to_string(),
                0,
            ),
            FormatType::DateTimeSyslog => (
                r"\w{3}\s+\d\d?\s+\d\d:\d\d:\d\d".to_string(),
                0,
            ),
            FormatType::CustomDateTime(format) => (datetime_format_to_regex(format), 0),
        }
    }

    /// Check if this type is numeric (needs sign handling).
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            FormatType::Decimal
                | FormatType::Binary
                | FormatType::Octal
                | FormatType::Hex
                | FormatType::NumberWithSeparator
                | FormatType::Float
                | FormatType::FloatDecimal
                | FormatType::Scientific
                | FormatType::GeneralFloat
                | FormatType::Percentage
        )
    }

    /// Check if this type is a datetime type.
    pub fn is_datetime(&self) -> bool {
        matches!(
            self,
            FormatType::DateTimeISO
                | FormatType::DateTimeEmail
                | FormatType::DateTimeUS
                | FormatType::DateTimeGeneric
                | FormatType::DateTimeHTTP
                | FormatType::DateTimeCTime
                | FormatType::TimeOnly
                | FormatType::DateTimeSyslog
                | FormatType::CustomDateTime(_)
        )
    }
}

/// Convert a matched string to a ParseValue according to the FormatType.
///
/// This performs the actual type conversion after a successful regex match.
/// For datetime types, the raw string and format are preserved for Python-side conversion.
pub fn convert_value(s: &str, spec: &FormatSpec) -> Result<ParseValue, String> {
    match &spec.format_type {
        FormatType::Default
        | FormatType::NonWhitespace
        | FormatType::Word
        | FormatType::NonWord
        | FormatType::Letters
        | FormatType::Whitespace
        | FormatType::NonDigit => Ok(ParseValue::Str(s.to_string())),

        FormatType::Decimal => parse_decimal(s, spec),
        FormatType::Binary => parse_int_base(s, 2),
        FormatType::Octal => parse_int_base(s, 8),
        FormatType::Hex => parse_int_base(s, 16),
        FormatType::NumberWithSeparator => parse_number_with_sep(s),

        FormatType::Float => s
            .parse::<f64>()
            .map(ParseValue::Float)
            .map_err(|e| format!("Failed to parse float '{}': {}", s, e)),
        FormatType::FloatDecimal => Ok(ParseValue::Decimal(s.to_string())),
        FormatType::Scientific => s
            .parse::<f64>()
            .map(ParseValue::Float)
            .map_err(|e| format!("Failed to parse scientific '{}': {}", s, e)),
        FormatType::GeneralFloat => s
            .parse::<f64>()
            .map(ParseValue::Float)
            .map_err(|e| format!("Failed to parse general float '{}': {}", s, e)),
        FormatType::Percentage => {
            let num_str = s.trim_end_matches('%');
            num_str
                .parse::<f64>()
                .map(|v| ParseValue::Percent(v / 100.0))
                .map_err(|e| format!("Failed to parse percentage '{}': {}", s, e))
        }

        _ if spec.format_type.is_datetime() => Ok(ParseValue::DateTime {
            raw: s.to_string(),
            format: spec.type_name.clone(),
        }),

        _ => Ok(ParseValue::Str(s.to_string())),
    }
}

pub fn dt_format_to_regex_map(py: Python<'_>) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    for (k, v) in [
        ("%a", "(?:Sun|Mon|Tue|Wed|Thu|Fri|Sat)"),
        ("%A", "(?:Sunday|Monday|Tuesday|Wednesday|Thursday|Friday|Saturday)"),
        ("%w", "[0-6]"),
        ("%d", "[0-9]{1,2}"),
        ("%b", "(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)"),
        ("%B", "(?:January|February|March|April|May|June|July|August|September|October|November|December)"),
        ("%m", "[0-9]{1,2}"),
        ("%y", "[0-9]{2}"),
        ("%Y", "[0-9]{4}"),
        ("%H", "[0-9]{1,2}"),
        ("%I", "[0-9]{1,2}"),
        ("%p", "(?:AM|PM)"),
        ("%M", "[0-9]{2}"),
        ("%S", "[0-9]{2}"),
        ("%f", "[0-9]{1,6}"),
        ("%z", "[+|-][0-9]{2}(:?[0-9]{2})?(:?[0-9]{2})?"),
        ("%j", "[0-9]{1,3}"),
        ("%U", "[0-9]{1,2}"),
        ("%W", "[0-9]{1,2}"),
    ] {
        dict.set_item(k, v)?;
    }
    Ok(dict.into_any().unbind())
}

pub fn dt_format_to_regex(format: &str) -> String {
    datetime_format_to_regex(format)
}


fn datetime_format_to_regex(format: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '%' && i + 1 < chars.len() {
            let token = match chars[i + 1] {
                'a' => Some("(?:Sun|Mon|Tue|Wed|Thu|Fri|Sat)"),
                'A' => Some("(?:Sunday|Monday|Tuesday|Wednesday|Thursday|Friday|Saturday)"),
                'w' => Some("[0-6]"),
                'd' => Some("[0-9]{1,2}"),
                'b' => Some("(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)"),
                'B' => Some("(?:January|February|March|April|May|June|July|August|September|October|November|December)"),
                'm' => Some("[0-9]{1,2}"),
                'y' => Some("[0-9]{2}"),
                'Y' => Some("[0-9]{4}"),
                'H' => Some("[0-9]{1,2}"),
                'I' => Some("[0-9]{1,2}"),
                'p' => Some("(?:AM|PM)"),
                'M' => Some("[0-9]{2}"),
                'S' => Some("[0-9]{2}"),
                'f' => Some("[0-9]{1,6}"),
                'z' => Some("[+|-][0-9]{2}(:?[0-9]{2})?(:?[0-9]{2})?"),
                'j' => Some("[0-9]{1,3}"),
                'U' => Some("[0-9]{1,2}"),
                'W' => Some("[0-9]{1,2}"),
                _ => None,
            };

            if let Some(re) = token {
                result.push_str(re);
                i += 2;
                continue;
            }
        }

        match chars[i] {
            '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' => {
                result.push('\\');
                result.push(chars[i]);
            }
            c => result.push(c),
        }
        i += 1;
    }

    result
}

/// Parse a decimal integer string, handling sign and base prefixes.
///
/// Supports:
/// - Optional sign: +, -, space
/// - Base prefixes: 0x (hex), 0o (octal), 0b (binary)
/// - Plain decimal numbers
fn parse_decimal(s: &str, spec: &FormatSpec) -> Result<ParseValue, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty string".to_string());
    }

    let (sign, rest) = match s.as_bytes().first() {
        Some(b'-') => (-1i64, &s[1..]),
        Some(b'+') | Some(b' ') => (1i64, &s[1..]),
        _ => (1i64, s),
    };

    let normalized = if let Some(grouping) = spec.grouping {
        rest.replace(grouping, "")
    } else if spec.align == Some('=') {
        rest.replace(spec.fill.unwrap_or('0'), "")
    } else {
        rest.to_string()
    };

    let (base, num_str) = if normalized.len() > 2 {
        match &normalized[..2] {
            "0x" | "0X" => (16, &normalized[2..]),
            "0o" | "0O" => (8, &normalized[2..]),
            "0b" | "0B" => (2, &normalized[2..]),
            _ => (10, normalized.as_str()),
        }
    } else {
        (10, normalized.as_str())
    };

    i64::from_str_radix(num_str, base)
        .map(|v| ParseValue::Int(sign * v))
        .map_err(|e| format!("Failed to parse integer '{}': {}", s, e))
}

/// Parse an integer with explicit base, stripping base prefix if present.
fn parse_int_base(s: &str, base: u32) -> Result<ParseValue, String> {
    let s = s.trim();
    let (sign, rest) = match s.as_bytes().first() {
        Some(b'-') => (-1i64, &s[1..]),
        Some(b'+') => (1i64, &s[1..]),
        _ => (1i64, s),
    };

    // Strip base prefix
    let num_str = if rest.len() > 2 {
        match (base, &rest[..2]) {
            (2, "0b") | (2, "0B") => &rest[2..],
            (8, "0o") | (8, "0O") => &rest[2..],
            (16, "0x") | (16, "0X") => &rest[2..],
            _ => rest,
        }
    } else {
        rest
    };

    i64::from_str_radix(num_str, base)
        .map(|v| ParseValue::Int(sign * v))
        .map_err(|e| format!("Failed to parse base-{} integer '{}': {}", base, s, e))
}

/// Parse a number with thousands separators (commas, dots, underscores).
fn parse_number_with_sep(s: &str) -> Result<ParseValue, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty string".to_string());
    }

    let (sign, rest) = match s.as_bytes().first() {
        Some(b'-') => (-1i64, &s[1..]),
        Some(b'+') | Some(b' ') => (1i64, &s[1..]),
        _ => (1i64, s),
    };

    let cleaned: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    cleaned
        .parse::<i64>()
        .map(|value| ParseValue::Int(sign * value))
        .map_err(|e| format!("Failed to parse number '{}': {}", s, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(format_string: &str, format_type: FormatType) -> FormatSpec {
        FormatSpec {
            format_string: format_string.to_string(),
            format_type,
            ..FormatSpec::default()
        }
    }

    #[test]
    fn test_format_type_from_str() {
        assert_eq!(FormatType::from_str("d"), Some(FormatType::Decimal));
        assert_eq!(FormatType::from_str("f"), Some(FormatType::Float));
        assert_eq!(FormatType::from_str("ti"), Some(FormatType::DateTimeISO));
        assert_eq!(FormatType::from_str(""), Some(FormatType::Default));
        assert_eq!(FormatType::from_str("z"), None);
    }

    #[test]
    fn test_convert_decimal() {
        match convert_value("42", &spec("d", FormatType::Decimal)) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, 42),
            other => panic!("Expected Int(42), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_negative_decimal() {
        match convert_value("-17", &spec("d", FormatType::Decimal)) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, -17),
            other => panic!("Expected Int(-17), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_hex() {
        match convert_value("0xFF", &spec("x", FormatType::Hex)) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, 255),
            other => panic!("Expected Int(255), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_binary() {
        match convert_value("0b1010", &spec("b", FormatType::Binary)) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, 10),
            other => panic!("Expected Int(10), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_octal() {
        match convert_value("0o77", &spec("o", FormatType::Octal)) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, 63),
            other => panic!("Expected Int(63), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_float() {
        match convert_value("3.14", &spec("f", FormatType::Float)) {
            Ok(ParseValue::Float(v)) => assert!((v - 3.14).abs() < 1e-10),
            other => panic!("Expected Float(3.14), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_percentage() {
        match convert_value("50%", &spec("%", FormatType::Percentage)) {
            Ok(ParseValue::Percent(v)) => assert!((v - 0.5).abs() < 1e-10),
            other => panic!("Expected Percent(0.5), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_number_with_sep() {
        match convert_value("1,000,000", &spec("n", FormatType::NumberWithSeparator)) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, 1_000_000),
            other => panic!("Expected Int(1000000), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_string_types() {
        match convert_value("hello", &spec("w", FormatType::Word)) {
            Ok(ParseValue::Str(v)) => assert_eq!(v, "hello"),
            other => panic!("Expected Str(hello), got {:?}", other),
        }
    }

    #[test]
    fn test_string_type_regex_semantics() {
        let (string_regex, string_groups) = FormatType::NonWhitespace.regex_pattern();
        assert_eq!(string_regex, r".+");
        assert_eq!(string_groups, 0);

        let (non_space_regex, non_space_groups) = FormatType::Whitespace.regex_pattern();
        assert_eq!(non_space_regex, r"\S+");
        assert_eq!(non_space_groups, 0);
    }

    #[test]
    fn test_convert_float_decimal_preserves_text() {
        match convert_value("12.3400", &spec("F", FormatType::FloatDecimal)) {
            Ok(ParseValue::Decimal(v)) => assert_eq!(v, "12.3400"),
            other => panic!("Expected Decimal(12.3400), got {:?}", other),
        }
    }

    #[test]
    fn test_datetime_token_regex_helpers() {
        assert_eq!(dt_format_to_regex("%Y-%m-%d"), "[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}");
        assert_eq!(
            dt_format_to_regex("%H:%M:%S"),
            "[0-9]{1,2}:[0-9]{2}:[0-9]{2}"
        );
    }

    #[test]
    fn test_is_numeric() {
        assert!(FormatType::Decimal.is_numeric());
        assert!(FormatType::Float.is_numeric());
        assert!(!FormatType::Word.is_numeric());
        assert!(!FormatType::Default.is_numeric());
    }

    #[test]
    fn test_is_datetime() {
        assert!(FormatType::DateTimeISO.is_datetime());
        assert!(FormatType::TimeOnly.is_datetime());
        assert!(!FormatType::Decimal.is_datetime());
    }

    #[test]
    fn test_custom_datetime_regex() {
        let (regex, groups) = FormatType::CustomDateTime("%Y-%m-%d".to_string()).regex_pattern();
        assert_eq!(groups, 0);
        assert_eq!(regex, "[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}");
    }
}
