//! Type conversion system for format specifiers.
//!
//! Handles conversion of matched strings to their target types
//! based on format specifiers like :d (integer), :f (float), etc.

use crate::result::ParseValue;

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
    /// `F` - Fixed-point float (Decimal precision, treated as float in Rust).
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
    pub fn regex_pattern(&self) -> (&str, usize) {
        match self {
            FormatType::Default => (".+?", 0),
            FormatType::NonWhitespace => (r"\S+", 0),
            FormatType::Word => (r"\w+", 0),
            FormatType::NonWord => (r"\W+", 0),
            FormatType::Letters => ("[A-Za-z]+", 0),
            FormatType::Whitespace => (r"\s+", 0),
            FormatType::NonDigit => (r"\D+", 0),
            FormatType::Decimal => (r"[-+ ]?\d+", 0),
            FormatType::Binary => (r"(0[bB])?[01]+", 1),
            FormatType::Octal => (r"(0[oO])?[0-7]+", 1),
            FormatType::Hex => (r"(0[xX])?[0-9a-fA-F]+", 1),
            FormatType::NumberWithSeparator => (r"\d{1,3}([,._]\d{3})*", 1),
            FormatType::Float => (r"\d*\.\d+", 0),
            FormatType::FloatDecimal => (r"\d*\.\d+", 0),
            FormatType::Scientific => (
                r"\d*\.\d+[eE][-+]?\d+|nan|NAN|[-+]?inf|[-+]?INF",
                0,
            ),
            FormatType::GeneralFloat => (
                r"\d+(\.\d+)?([eE][-+]?\d+)?|nan|NAN|[-+]?inf|[-+]?INF",
                2,
            ),
            FormatType::Percentage => (r"\d+(\.\d+)?%", 1),
            FormatType::DateTimeISO => (
                r"\d{4}-\d\d-\d\d[T ]\d\d:\d\d(:\d\d(\.\d+)?)?(Z|[+-]\d\d:?\d\d)?|\d{4}-\d\d-\d\d",
                3,
            ),
            FormatType::DateTimeEmail => (
                r"\w{3},?\s+\d\d?\s+\w{3}\s+\d{4}\s+\d\d:\d\d(:\d\d)?\s*(\w+|[+-]\d{4})?",
                2,
            ),
            FormatType::DateTimeUS => (
                r"(\d\d?|\w{3})\s+\d\d?\s+\d{4}\s+\d\d:\d\d(:\d\d)?\s*([AP]M)?\s*(\w+|[+-]\d{4})?",
                4,
            ),
            FormatType::DateTimeGeneric => (
                r"\d\d?[-/](\d\d?|\w{3})[-/]\d{4}\s+\d\d:\d\d(:\d\d)?\s*([AP]M)?\s*(\w+|[+-]\d{4})?",
                4,
            ),
            FormatType::DateTimeHTTP => (
                r"\d\d?[-/]\w{3}[-/]\d{4}:\d\d:\d\d:\d\d\s*(\w+|[+-]\d{4})?",
                1,
            ),
            FormatType::DateTimeCTime => (
                r"\w{3}\s+\w{3}\s+\d\d?\s+\d\d:\d\d:\d\d\s+\d{4}",
                0,
            ),
            FormatType::TimeOnly => (
                r"\d\d:\d\d(:\d\d(\.\d+)?)?\s*([AP]M)?\s*(\w+|[+-]\d{4})?",
                4,
            ),
            FormatType::DateTimeSyslog => (
                r"\w{3}\s+\d\d?\s+\d\d:\d\d:\d\d",
                0,
            ),
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
        )
    }
}

/// Convert a matched string to a ParseValue according to the FormatType.
///
/// This performs the actual type conversion after a successful regex match.
/// For datetime types, the raw string is preserved for Python-side conversion.
pub fn convert_value(s: &str, fmt_type: &FormatType) -> Result<ParseValue, String> {
    match fmt_type {
        FormatType::Default | FormatType::NonWhitespace | FormatType::Word
        | FormatType::NonWord | FormatType::Letters | FormatType::Whitespace
        | FormatType::NonDigit => Ok(ParseValue::Str(s.to_string())),

        FormatType::Decimal => parse_decimal(s),
        FormatType::Binary => parse_int_base(s, 2),
        FormatType::Octal => parse_int_base(s, 8),
        FormatType::Hex => parse_int_base(s, 16),
        FormatType::NumberWithSeparator => parse_number_with_sep(s),

        FormatType::Float | FormatType::FloatDecimal => {
            s.parse::<f64>()
                .map(ParseValue::Float)
                .map_err(|e| format!("Failed to parse float '{}': {}", s, e))
        }
        FormatType::Scientific => {
            s.parse::<f64>()
                .map(ParseValue::Float)
                .map_err(|e| format!("Failed to parse scientific '{}': {}", s, e))
        }
        FormatType::GeneralFloat => {
            s.parse::<f64>()
                .map(ParseValue::Float)
                .map_err(|e| format!("Failed to parse general float '{}': {}", s, e))
        }
        FormatType::Percentage => {
            let num_str = s.trim_end_matches('%');
            num_str
                .parse::<f64>()
                .map(|v| ParseValue::Percent(v / 100.0))
                .map_err(|e| format!("Failed to parse percentage '{}': {}", s, e))
        }

        // DateTime types: preserve matched string for Python conversion
        _ if fmt_type.is_datetime() => Ok(ParseValue::DateTime(s.to_string())),

        _ => Ok(ParseValue::Str(s.to_string())),
    }
}

/// Parse a decimal integer string, handling sign and base prefixes.
///
/// Supports:
/// - Optional sign: +, -, space
/// - Base prefixes: 0x (hex), 0o (octal), 0b (binary)
/// - Plain decimal numbers
fn parse_decimal(s: &str) -> Result<ParseValue, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty string".to_string());
    }

    let (sign, rest) = match s.as_bytes().first() {
        Some(b'-') => (-1i64, &s[1..]),
        Some(b'+') | Some(b' ') => (1i64, &s[1..]),
        _ => (1i64, s),
    };

    // Detect base from prefix
    let (base, num_str) = if rest.len() > 2 {
        match &rest[..2] {
            "0x" | "0X" => (16, &rest[2..]),
            "0o" | "0O" => (8, &rest[2..]),
            "0b" | "0B" => (2, &rest[2..]),
            _ => (10, rest),
        }
    } else {
        (10, rest)
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
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    cleaned
        .parse::<i64>()
        .map(ParseValue::Int)
        .map_err(|e| format!("Failed to parse number '{}': {}", s, e))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        match convert_value("42", &FormatType::Decimal) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, 42),
            other => panic!("Expected Int(42), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_negative_decimal() {
        match convert_value("-17", &FormatType::Decimal) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, -17),
            other => panic!("Expected Int(-17), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_hex() {
        match convert_value("0xFF", &FormatType::Hex) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, 255),
            other => panic!("Expected Int(255), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_binary() {
        match convert_value("0b1010", &FormatType::Binary) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, 10),
            other => panic!("Expected Int(10), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_octal() {
        match convert_value("0o77", &FormatType::Octal) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, 63),
            other => panic!("Expected Int(63), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_float() {
        match convert_value("3.14", &FormatType::Float) {
            Ok(ParseValue::Float(v)) => assert!((v - 3.14).abs() < 1e-10),
            other => panic!("Expected Float(3.14), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_percentage() {
        match convert_value("50%", &FormatType::Percentage) {
            Ok(ParseValue::Percent(v)) => assert!((v - 0.5).abs() < 1e-10),
            other => panic!("Expected Percent(0.5), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_number_with_sep() {
        match convert_value("1,000,000", &FormatType::NumberWithSeparator) {
            Ok(ParseValue::Int(v)) => assert_eq!(v, 1_000_000),
            other => panic!("Expected Int(1000000), got {:?}", other),
        }
    }

    #[test]
    fn test_convert_string_types() {
        match convert_value("hello", &FormatType::Word) {
            Ok(ParseValue::Str(v)) => assert_eq!(v, "hello"),
            other => panic!("Expected Str(hello), got {:?}", other),
        }
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
}
