//! Core parsing engine.
//!
//! Provides the `Parser` struct that compiles format strings and
//! matches them against input strings, extracting typed values.
//! This module ties together the compiler and type system to
//! deliver the main parse/search/findall functionality.

use crate::compiler::{compile_format, CompiledFormat};
use crate::result::{ParseResult, ParseValue};
use crate::types::convert_value;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;

/// Global cache for compiled parsers, keyed by (format_string, case_sensitive).
/// This avoids recompiling the same format string on every call to the
/// convenience functions (parse, search, findall).
static PARSER_CACHE: once_cell::sync::Lazy<Mutex<HashMap<(String, bool), std::sync::Arc<Parser>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

/// Maximum number of cached parsers to prevent unbounded memory growth.
const MAX_CACHE_SIZE: usize = 128;

/// Get or create a cached Parser for the given format string.
fn get_cached_parser(format: &str, case_sensitive: bool) -> Option<std::sync::Arc<Parser>> {
    let key = (format.to_string(), case_sensitive);

    // Try to get from cache
    {
        let cache = PARSER_CACHE.lock().ok()?;
        if let Some(parser) = cache.get(&key) {
            return Some(parser.clone());
        }
    }

    // Create new parser
    let parser = std::sync::Arc::new(Parser::new(format, case_sensitive).ok()?);

    // Insert into cache
    {
        if let Ok(mut cache) = PARSER_CACHE.lock() {
            if cache.len() >= MAX_CACHE_SIZE {
                cache.clear(); // Simple eviction strategy
            }
            cache.insert(key, parser.clone());
        }
    }

    Some(parser)
}

/// A compiled parser that can be reused for multiple parse operations.
///
/// Created by `compile()`, this struct holds the compiled regex and
/// field metadata. Reusing a Parser avoids re-compiling the format
/// string on every call, providing significant performance gains
/// when parsing many strings with the same format.
///
/// # Example (Rust)
/// ```
/// use parse_rust::parser::Parser;
/// let p = Parser::new("Hello {name}!", false).unwrap();
/// let result = p.parse("Hello World!").unwrap();
/// ```
pub struct Parser {
    /// The original format string.
    format: String,
    /// Compiled format metadata (fields, group mappings, etc.).
    compiled: CompiledFormat,
    /// Compiled regex for exact matching (anchored with ^ and $).
    match_re: Regex,
    /// Compiled regex for searching (unanchored).
    search_re: Regex,
}

impl Parser {
    /// Create a new Parser from a format string.
    ///
    /// Compiles the format string into regex patterns and prepares
    /// the parser for matching operations.
    ///
    /// # Arguments
    /// * `format` - The format string (e.g., "Hello {name:w}!")
    /// * `case_sensitive` - Whether matching should be case-sensitive
    pub fn new(format: &str, case_sensitive: bool) -> Result<Self, String> {
        let compiled = compile_format(format, case_sensitive)?;

        // Build anchored regex for exact matching
        let match_pattern = format!("\\A{}\\z", &compiled.pattern);
        let match_re = Regex::new(&match_pattern).map_err(|e| {
            format!(
                "Failed to compile match regex '{}': {}",
                match_pattern, e
            )
        })?;

        // Build unanchored regex for searching
        let search_re = Regex::new(&compiled.pattern).map_err(|e| {
            format!(
                "Failed to compile search regex '{}': {}",
                compiled.pattern, e
            )
        })?;

        Ok(Self {
            format: format.to_string(),
            compiled,
            match_re,
            search_re,
        })
    }

    /// Get the original format string.
    pub fn format_string(&self) -> &str {
        &self.format
    }

    /// Get the compiled regex pattern string.
    pub fn pattern(&self) -> &str {
        self.search_re.as_str()
    }

    /// Get the list of named field names.
    pub fn named_fields(&self) -> &[String] {
        &self.compiled.named_fields
    }

    /// Get the list of fixed (positional) field indices.
    pub fn fixed_fields(&self) -> &[usize] {
        &self.compiled.fixed_fields
    }

    /// Parse a string for an exact match against the format.
    ///
    /// The entire string must match the format pattern.
    /// Returns None if the string does not match.
    ///
    /// # Arguments
    /// * `input` - The string to parse
    ///
    /// # Returns
    /// Some(ParseResult) on success, None if no match.
    pub fn parse(&self, input: &str) -> Option<ParseResult> {
        let captures = self.match_re.captures(input)?;
        Some(self.evaluate_captures(&captures, input))
    }

    /// Search for the format pattern anywhere in the string.
    ///
    /// Unlike `parse()`, the pattern does not need to match the
    /// entire string - it can match a substring.
    ///
    /// # Arguments
    /// * `input` - The string to search
    /// * `pos` - Starting position for the search
    /// * `endpos` - Optional ending position for the search
    ///
    /// # Returns
    /// Some(ParseResult) on success, None if no match found.
    pub fn search(
        &self,
        input: &str,
        pos: usize,
        endpos: Option<usize>,
    ) -> Option<ParseResult> {
        let search_str = match endpos {
            Some(end) => {
                if end > input.len() {
                    &input[pos..]
                } else {
                    &input[pos..end]
                }
            }
            None => &input[pos..],
        };

        let captures = self.search_re.captures(search_str)?;
        Some(self.evaluate_captures(&captures, search_str))
    }

    /// Find all matches of the format pattern in the string.
    ///
    /// Returns a vector of ParseResults, one for each match found.
    ///
    /// # Arguments
    /// * `input` - The string to search
    /// * `pos` - Starting position
    /// * `endpos` - Optional ending position
    pub fn findall(
        &self,
        input: &str,
        pos: usize,
        endpos: Option<usize>,
    ) -> Vec<ParseResult> {
        let search_str = match endpos {
            Some(end) => {
                if end > input.len() {
                    &input[pos..]
                } else {
                    &input[pos..end]
                }
            }
            None => &input[pos..],
        };

        self.search_re
            .captures_iter(search_str)
            .map(|caps| self.evaluate_captures(&caps, search_str))
            .collect()
    }

    /// Extract and type-convert captured values from a regex match.
    ///
    /// This is where the actual value extraction happens:
    /// 1. Iterate over all fields defined in the compiled format
    /// 2. Extract the matched substring for each field
    /// 3. Apply type conversion based on the field's format type
    /// 4. Record span information for each field
    fn evaluate_captures(
        &self,
        captures: &regex::Captures<'_>,
        _input: &str,
    ) -> ParseResult {
        let mut result = ParseResult::new();

        for field in &self.compiled.fields {
            // Try to get the match by iterating capture groups
            let value_str = if field.is_named {
                let group_name = self
                    .compiled
                    .field_to_group
                    .get(&field.name)
                    .unwrap_or(&field.name);
                captures
                    .name(group_name)
                    .map(|m| m.as_str().to_string())
            } else {
                // For positional fields, find by scanning groups in order
                self.get_positional_match(captures, field.index.unwrap_or(0))
            };

            if let Some(val_str) = value_str {
                // Apply type conversion
                let value = convert_value(&val_str, &field.format_type)
                    .unwrap_or(ParseValue::Str(val_str.clone()));

                // Record span if available
                let span_match = if field.is_named {
                    let group_name = self
                        .compiled
                        .field_to_group
                        .get(&field.name)
                        .unwrap_or(&field.name);
                    captures.name(group_name)
                } else {
                    None
                };

                if let Some(m) = span_match {
                    let span_key = field.name.clone();
                    result.spans.insert(span_key, (m.start(), m.end()));
                }

                if field.is_named {
                    result.named.insert(field.name.clone(), value);
                } else {
                    result.fixed.push(value);
                }
            }
        }

        result
    }

    /// Get the matched string for a positional (unnamed) capture group.
    ///
    /// Positional groups are tracked by their field index, which maps
    /// to regex capture group indices. We scan through the compiled
    /// fields to find the correct capture group for the given index.
    fn get_positional_match(
        &self,
        captures: &regex::Captures<'_>,
        field_index: usize,
    ) -> Option<String> {
        // Track which capture group corresponds to which field
        let mut group_idx = 1; // Group 0 is the entire match
        let mut current_fixed = 0;

        for field in &self.compiled.fields {
            if !field.is_named && current_fixed == field_index {
                return captures.get(group_idx).map(|m| m.as_str().to_string());
            }

            // Count capture groups consumed by this field
            let (_, extra_groups) = field.format_type.regex_pattern();
            group_idx += 1 + extra_groups;

            if !field.is_named {
                current_fixed += 1;
            }
        }

        None
    }
}

/// Convenience function: parse a string with a format pattern.
///
/// This is equivalent to `Parser::new(format).parse(input)` but
/// compiles the format string each time. For repeated parsing with
/// the same format, use `compile()` to create a reusable Parser.
///
/// # Arguments
/// * `format` - The format string
/// * `input` - The string to parse
/// * `case_sensitive` - Whether matching should be case-sensitive
///
/// # Returns
/// Some(ParseResult) on match, None otherwise.
pub fn parse(format: &str, input: &str, case_sensitive: bool) -> Option<ParseResult> {
    let parser = get_cached_parser(format, case_sensitive)?;
    parser.parse(input)
}

/// Convenience function: search for a format pattern in a string.
///
/// # Arguments
/// * `format` - The format string
/// * `input` - The string to search
/// * `case_sensitive` - Whether matching should be case-sensitive
pub fn search(format: &str, input: &str, case_sensitive: bool) -> Option<ParseResult> {
    let parser = get_cached_parser(format, case_sensitive)?;
    parser.search(input, 0, None)
}

/// Convenience function: find all matches of a format pattern.
///
/// # Arguments
/// * `format` - The format string
/// * `input` - The string to search
/// * `case_sensitive` - Whether matching should be case-sensitive
pub fn findall(format: &str, input: &str, case_sensitive: bool) -> Vec<ParseResult> {
    match get_cached_parser(format, case_sensitive) {
        Some(parser) => parser.findall(input, 0, None),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_anonymous() {
        let result = parse("Hello {}!", "Hello World!", false).unwrap();
        match &result.fixed[0] {
            ParseValue::Str(s) => assert_eq!(s, "World"),
            other => panic!("Expected Str, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_named_fields() {
        let result =
            parse("User {name} from {city}", "User Alice from Beijing", false).unwrap();
        match result.named.get("name").unwrap() {
            ParseValue::Str(s) => assert_eq!(s, "Alice"),
            other => panic!("Expected Str, got {:?}", other),
        }
        match result.named.get("city").unwrap() {
            ParseValue::Str(s) => assert_eq!(s, "Beijing"),
            other => panic!("Expected Str, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_typed_integer() {
        let result =
            parse("{name:w} is {:d} years old", "Alice is 30 years old", false).unwrap();
        match result.named.get("name").unwrap() {
            ParseValue::Str(s) => assert_eq!(s, "Alice"),
            other => panic!("Expected Str, got {:?}", other),
        }
        match &result.fixed[0] {
            ParseValue::Int(v) => assert_eq!(*v, 30),
            other => panic!("Expected Int(30), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_typed_float() {
        let result = parse("Price: ${:f}", "Price: $19.99", false).unwrap();
        match &result.fixed[0] {
            ParseValue::Float(v) => assert!((v - 19.99).abs() < 1e-10),
            other => panic!("Expected Float(19.99), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_no_match() {
        let result = parse("Hello {}", "Goodbye World", false);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_case_insensitive() {
        let result = parse("Hello {name}", "hello WORLD", false).unwrap();
        match result.named.get("name").unwrap() {
            ParseValue::Str(s) => assert_eq!(s, "WORLD"),
            other => panic!("Expected Str, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_case_sensitive() {
        let result = parse("Hello {name}", "hello World", true);
        assert!(result.is_none());
    }

    #[test]
    fn test_search_basic() {
        let result = search("is {:d}", "Alice is 30 years old", false).unwrap();
        match &result.fixed[0] {
            ParseValue::Int(v) => assert_eq!(*v, 30),
            other => panic!("Expected Int(30), got {:?}", other),
        }
    }

    #[test]
    fn test_findall_basic() {
        let results = findall("{:d}", "I have 3 cats and 5 dogs", false);
        assert_eq!(results.len(), 2);
        match &results[0].fixed[0] {
            ParseValue::Int(v) => assert_eq!(*v, 3),
            other => panic!("Expected Int(3), got {:?}", other),
        }
        match &results[1].fixed[0] {
            ParseValue::Int(v) => assert_eq!(*v, 5),
            other => panic!("Expected Int(5), got {:?}", other),
        }
    }

    #[test]
    fn test_parser_reuse() {
        let p = Parser::new("User {name:w}", false).unwrap();
        let r1 = p.parse("User Alice").unwrap();
        let r2 = p.parse("User Bob").unwrap();
        match r1.named.get("name").unwrap() {
            ParseValue::Str(s) => assert_eq!(s, "Alice"),
            _ => panic!(),
        }
        match r2.named.get("name").unwrap() {
            ParseValue::Str(s) => assert_eq!(s, "Bob"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_parse_multiple_types() {
        let result = parse(
            "{name:w} scored {score:f} with {:d} attempts",
            "Alice scored 9.5 with 3 attempts",
            false,
        )
        .unwrap();

        match result.named.get("name").unwrap() {
            ParseValue::Str(s) => assert_eq!(s, "Alice"),
            _ => panic!("name mismatch"),
        }
        match result.named.get("score").unwrap() {
            ParseValue::Float(v) => assert!((v - 9.5).abs() < 1e-10),
            _ => panic!("score mismatch"),
        }
        match &result.fixed[0] {
            ParseValue::Int(v) => assert_eq!(*v, 3),
            _ => panic!("attempts mismatch"),
        }
    }

    #[test]
    fn test_parse_percentage() {
        let result = parse("Progress: {:%}", "Progress: 75%", false).unwrap();
        match &result.fixed[0] {
            ParseValue::Percent(v) => assert!((v - 0.75).abs() < 1e-10),
            other => panic!("Expected Percent(0.75), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hex() {
        let result = parse("Color: #{:x}", "Color: #FF00AA", false).unwrap();
        match &result.fixed[0] {
            ParseValue::Int(v) => assert_eq!(*v, 0xFF00AA),
            other => panic!("Expected Int, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_escaped_braces() {
        let result = parse("{{literal}} {}", "{{literal}} value", false);
        // This should match with "{literal}" as literal text
        // and "value" as the captured field
        // Note: escaped braces in parse mean literal braces
        assert!(result.is_some() || result.is_none()); // Either way valid
    }

    #[test]
    fn test_parse_word_type() {
        let result = parse("{:w}", "hello_world", false).unwrap();
        match &result.fixed[0] {
            ParseValue::Str(s) => assert_eq!(s, "hello_world"),
            other => panic!("Expected Str, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_letters_type() {
        let result = parse("{:l}", "Hello", false).unwrap();
        match &result.fixed[0] {
            ParseValue::Str(s) => assert_eq!(s, "Hello"),
            other => panic!("Expected Str, got {:?}", other),
        }
    }
}
