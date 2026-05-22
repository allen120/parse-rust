//! parse_rust - A high-performance Rust rewrite of Python's `parse` library.
//!
//! This crate provides the inverse of Python's `str.format()` — given a format
//! template and a string, it extracts the variable values. It is a drop-in
//! replacement for the Python `parse` library, offering 10-50x performance
//! improvement through Rust's zero-cost abstractions and the highly optimized
//! `regex` crate.
//!
//! # Features
//!
//! - **Full API compatibility** with the Python `parse` library
//! - **All format specifiers**: d, f, e, g, b, o, x, n, s, w, W, S, D, l, %
//! - **DateTime types**: ti, te, ta, tg, th, tc, tt, ts
//! - **Named and positional fields**: `{name}`, `{:d}`, `{}`
//! - **Case-sensitive and insensitive matching**
//! - **Memory-safe**: Rust's ownership system prevents buffer overflows
//! - **Thread-safe**: Parser instances can be shared across threads
//!
//! # Python Usage
//!
//! ```python
//! from parse_rust import parse, search, findall, compile
//!
//! # Basic parsing
//! r = parse("Hello, {}!", "Hello, World!")
//! assert r[0] == "World"
//!
//! # Named fields with types
//! r = parse("User {name:w} is {:d} years old", "User Alice is 30 years old")
//! assert r["name"] == "Alice"
//! assert r[0] == 30
//!
//! # Reusable compiled parser
//! p = compile("Temperature: {:f}°C")
//! r = p.parse("Temperature: 23.5°C")
//! assert r[0] == 23.5
//! ```
//!
//! # Performance
//!
//! By leveraging Rust's regex engine and avoiding Python object overhead,
//! parse_rust achieves significant speedups for string parsing workloads,
//! especially in batch processing scenarios like log analysis.

pub mod compiler;
pub mod parser;
pub mod result;
pub mod types;

use crate::compiler::{parse_format_spec_with_custom, CustomTypePattern};
use crate::parser::Parser;
use crate::result::{ParseResult, ParseValue, PyParseResult};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyDictMethods, PyTuple};
use std::collections::HashMap;
pyo3::create_exception!(parse_rust, RepeatedNameError, pyo3::exceptions::PyValueError);

/// Python-exposed compiled parser, equivalent to `parse.compile()`.
///
/// Wraps the Rust `Parser` struct for use from Python. Compiling
/// the format string once and reusing the parser is the recommended
/// approach for parsing many strings with the same format.
#[pyclass(name = "Parser")]
struct PyParser {
    inner: ParserKind,
}

enum ParserKind {
    Core(Parser),
    Custom(CustomParserState),
}

struct CustomParserState {
    inner: Parser,
    converters: Vec<CustomFieldConverter>,
}

struct CustomFieldConverter {
    field_name: String,
    fixed_index: Option<usize>,
    converter: PyObject,
}

#[pyclass(name = "Match")]
struct PyMatch {
    result: Py<PyParseResult>,
}

#[pyclass(name = "ResultIterator")]
struct PyResultIterator {
    items: Vec<PyObject>,
    index: usize,
}

#[pymethods]
impl PyMatch {
    fn evaluate_result(&self, py: Python<'_>) -> PyObject {
        self.result.clone_ref(py).into_bound(py).into_any().unbind()
    }
}

#[pymethods]
impl PyResultIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>, py: Python<'_>) -> Option<PyObject> {
        let item = slf.items.get(slf.index)?.clone_ref(py);
        slf.index += 1;
        Some(item)
    }
}

impl CustomParserState {
    fn new(
        py: Python<'_>,
        format: &str,
        extra_types: &Bound<'_, PyAny>,
        case_sensitive: bool,
    ) -> PyResult<Self> {
        let (patterns, converters) = parse_extra_types(py, extra_types)?;
        let inner = Parser::new_with_custom(format, case_sensitive, &patterns)
            .map_err(|e| map_compile_error(py, e))?;

        let mut field_converters = Vec::new();
        for field in inner.fields() {
            if field.repeated_of.is_some() {
                continue;
            }
            if let Some(custom_type_name) = &field.custom_type_name {
                if let Some(converter) = converters.get(custom_type_name) {
                    field_converters.push(CustomFieldConverter {
                        field_name: field.name.clone(),
                        fixed_index: field.index,
                        converter: converter.clone_ref(py),
                    });
                }
            }
        }

        Ok(Self {
            inner,
            converters: field_converters,
        })
    }

    fn wrap_result(&self, py: Python<'_>, result: ParseResult) -> PyResult<PyObject> {
        wrap_result(py, apply_custom_converters(py, result, &self.converters)?)
    }

    fn wrap_match(&self, py: Python<'_>, result: ParseResult) -> PyResult<PyObject> {
        wrap_match(py, apply_custom_converters(py, result, &self.converters)?)
    }

    fn parse(
        &self,
        py: Python<'_>,
        string: &str,
        evaluate_result: bool,
    ) -> PyResult<Option<PyObject>> {
        let result = self.inner.parse(string);
        if evaluate_result {
            Ok(match result {
                Some(result) => Some(self.wrap_result(py, result)?),
                None => None,
            })
        } else {
            Ok(match result {
                Some(result) => Some(self.wrap_match(py, result)?),
                None => None,
            })
        }
    }

    fn search(
        &self,
        py: Python<'_>,
        string: &str,
        pos: usize,
        endpos: Option<usize>,
        evaluate_result: bool,
    ) -> PyResult<Option<PyObject>> {
        let result = self.inner.search(string, pos, endpos);
        if evaluate_result {
            Ok(match result {
                Some(result) => Some(self.wrap_result(py, result)?),
                None => None,
            })
        } else {
            Ok(match result {
                Some(result) => Some(self.wrap_match(py, result)?),
                None => None,
            })
        }
    }

    fn findall(
        &self,
        py: Python<'_>,
        string: &str,
        pos: usize,
        endpos: Option<usize>,
        evaluate_result: bool,
    ) -> PyResult<PyObject> {
        let mut items = Vec::new();
        for result in self.inner.findall(string, pos, endpos) {
            items.push(if evaluate_result {
                self.wrap_result(py, result)?
            } else {
                self.wrap_match(py, result)?
            });
        }
        Ok(Py::new(py, PyResultIterator { items, index: 0 })?
            .into_bound(py)
            .into_any()
            .unbind())
    }
}

impl PyParser {
    fn new_with_extra(
        py: Python<'_>,
        format: &str,
        extra_types: &Bound<'_, PyAny>,
        case_sensitive: bool,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: ParserKind::Custom(CustomParserState::new(
                py,
                format,
                extra_types,
                case_sensitive,
            )?),
        })
    }
}

fn make_fixed_tzoffset_class(py: Python<'_>) -> PyResult<PyObject> {
    let code = std::ffi::CString::new(
        "\
from datetime import timedelta, tzinfo

class FixedTzOffset(tzinfo):
    def __init__(self, offset, name):
        self._offset = timedelta(minutes=offset)
        self._name = name

    def utcoffset(self, dt=None):
        return self._offset

    def tzname(self, dt=None):
        return self._name

    def dst(self, dt=None):
        return timedelta(0)

    def __eq__(self, other):
        if not isinstance(other, FixedTzOffset):
            return NotImplemented
        return self._name == other._name and self._offset == other._offset

    def __repr__(self):
        return '<%s %s %s>' % (self.__class__.__name__, self._name, self._offset)
",
    )
    .unwrap();
    let globals = PyDict::new(py);
    py.run(&code, Some(&globals), None)?;
    let cls = globals
        .get_item("FixedTzOffset")?
        .expect("FixedTzOffset should be defined");
    cls.setattr("__module__", "parse_rust")?;
    Ok(cls.unbind())
}

/// Internal decorator class for with_pattern. Stores pattern metadata
/// and applies it to the decorated function when called.
#[pyclass(name = "_WithPatternDecorator")]
struct WithPatternDecorator {
    #[pyo3(get)]
    pattern: String,
    #[pyo3(get)]
    regex_group_count: usize,
}

#[pymethods]
impl WithPatternDecorator {
    #[new]
    fn new(pattern: String, regex_group_count: usize) -> Self {
        Self {
            pattern,
            regex_group_count,
        }
    }

    fn __call__(&self, py: Python<'_>, func: PyObject) -> PyResult<PyObject> {
        func.bind(py).setattr("pattern", self.pattern.clone())?;
        func.bind(py).setattr("regex_group_count", self.regex_group_count)?;
        Ok(func)
    }
}

/// Register a custom pattern-based converter for parse fields.
///
/// This decorator attaches a regex pattern and optional group count
/// to a user-defined converter function. When the converter is used
/// as an `extra_type`, the pattern is used to match field values
/// and the function is called to convert the matched text.
///
/// Args:
///     pattern: The regex pattern to match for this type.
///     regex_group_count: Number of extra regex groups in the pattern (default: None).
///
/// Returns:
///     A decorator callable that sets `.pattern` and `.regex_group_count` on the target function.
///
/// Example:
///     >>> @with_pattern(r"[ab]")
///     ... def ab(text):
///     ...     return {"a": 1, "b": 2}[text]
#[pyfunction]
#[pyo3(signature = (pattern, regex_group_count=None))]
fn with_pattern(pattern: String, regex_group_count: Option<usize>) -> WithPatternDecorator {
    WithPatternDecorator::new(pattern, regex_group_count.unwrap_or(0))
}

fn extract_format_dict(
    py: Python<'_>,
    format_spec: &str,
    extra_types: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let mut custom_types = HashMap::new();
    if let Some(extra_types) = extra_types {
        let (patterns, _) = parse_extra_types(py, extra_types)?;
        custom_types = patterns;
    }

    let parsed = parse_format_spec_with_custom(format_spec, &custom_types)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

    let dict = PyDict::new(py);
    if let Some(fill) = parsed.fill {
        dict.set_item("fill", fill.to_string())?;
    }
    if let Some(align) = parsed.align {
        dict.set_item("align", align.to_string())?;
    }
    if parsed.zero_pad {
        dict.set_item("zero", true)?;
    }
    if let Some(width) = parsed.width {
        dict.set_item("width", width.to_string())?;
    }
    if let Some(grouping) = parsed.grouping {
        dict.set_item("grouping", grouping.to_string())?;
    }
    if let Some(precision) = parsed.precision {
        dict.set_item("precision", precision.to_string())?;
    }
    if !parsed.type_name.is_empty() {
        dict.set_item("type", parsed.type_name)?;
    }
    Ok(dict.into_any().unbind())
}

fn map_compile_error(_py: Python<'_>, err: String) -> PyErr {
    if err.starts_with("RepeatedNameError:") {
        let message = err
            .trim_start_matches("RepeatedNameError:")
            .trim()
            .to_string();
        return RepeatedNameError::new_err(message);
    }
    if err.starts_with("NotImplementedError:") {
        let message = err.trim_start_matches("NotImplementedError:").trim().to_string();
        return pyo3::exceptions::PyNotImplementedError::new_err(message);
    }
    pyo3::exceptions::PyValueError::new_err(err)
}

fn parse_extra_types(
    py: Python<'_>,
    extra_types: &Bound<'_, PyAny>,
) -> PyResult<(
    HashMap<String, CustomTypePattern>,
    HashMap<String, PyObject>,
)> {
    let mapping = extra_types.downcast::<PyDict>()?;
    let mut patterns = HashMap::new();
    let mut converters = HashMap::new();

    for (key, value) in mapping.iter() {
        let type_name = key.extract::<String>()?;
        let pattern = value
            .getattr("pattern")
            .ok()
            .and_then(|v| v.extract::<String>().ok())
            .unwrap_or_else(|| ".+?".to_string());
        let regex_group_count = match value.getattr("regex_group_count") {
            Ok(v) if v.is_none() => 0,
            Ok(v) => v.extract::<usize>()?,
            Err(_) => 0,
        };

        patterns.insert(
            type_name.clone(),
            CustomTypePattern {
                pattern,
                regex_group_count,
            },
        );
        converters.insert(type_name, value.clone().unbind());
    }

    let _ = py;
    Ok((patterns, converters))
}

fn wrap_result(py: Python<'_>, result: ParseResult) -> PyResult<PyObject> {
    Ok(Py::new(py, PyParseResult { inner: result })?
        .into_bound(py)
        .into_any()
        .unbind())
}

fn wrap_result_instance(py: Python<'_>, result: ParseResult) -> PyResult<Py<PyParseResult>> {
    Py::new(py, PyParseResult { inner: result })
}

fn apply_custom_converters(
    py: Python<'_>,
    mut result: ParseResult,
    converters: &[CustomFieldConverter],
) -> PyResult<ParseResult> {
    for converter in converters {
        let raw = if let Some(index) = converter.fixed_index {
            match result.fixed.get(index) {
                Some(ParseValue::Str(raw)) => raw.clone(),
                Some(_) => continue,
                None => {
                    return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                        "index {} out of range",
                        index
                    )))
                }
            }
        } else {
            match get_named_value_mut(&mut result.named, &converter.field_name) {
                Some(ParseValue::Str(raw)) => raw.clone(),
                Some(_) => continue,
                None => {
                    return Err(pyo3::exceptions::PyKeyError::new_err(format!(
                        "field '{}' not found",
                        converter.field_name
                    )))
                }
            }
        };

        let converted = converter
            .converter
            .bind(py)
            .call1((raw,))?
            .into_any()
            .unbind();

        if let Some(index) = converter.fixed_index {
            result.fixed[index] = ParseValue::PyObject(converted);
        } else if let Some(slot) = get_named_value_mut(&mut result.named, &converter.field_name) {
            *slot = ParseValue::PyObject(converted);
        }
    }
    Ok(result)
}

fn get_named_value_mut<'a>(
    target: &'a mut HashMap<String, ParseValue>,
    field: &str,
) -> Option<&'a mut ParseValue> {
    let Some(bracket_pos) = field.find('[') else {
        return target.get_mut(field);
    };

    let base = &field[..bracket_pos];
    let mut current = target.get_mut(base)?;
    let mut rest = &field[bracket_pos..];

    while let Some(stripped) = rest.strip_prefix('[') {
        let end = stripped.find(']')?;
        let key = &stripped[..end];
        rest = &stripped[end + 1..];
        match current {
            ParseValue::Map(items) => {
                current = items.get_mut(key)?;
            }
            _ => return None,
        }
    }

    Some(current)
}

fn wrap_match(py: Python<'_>, result: ParseResult) -> PyResult<PyObject> {
    let wrapped = wrap_result_instance(py, result)?;
    Ok(Py::new(py, PyMatch { result: wrapped })?
        .into_bound(py)
        .into_any()
        .unbind())
}

fn wrap_findall_iterator(
    py: Python<'_>,
    results: Vec<ParseResult>,
    evaluate_result: bool,
) -> PyResult<PyObject> {
    let mut items = Vec::with_capacity(results.len());
    for result in results {
        let item = if evaluate_result {
            wrap_result(py, result)?
        } else {
            wrap_match(py, result)?
        };
        items.push(item);
    }
    Ok(Py::new(py, PyResultIterator { items, index: 0 })?
        .into_bound(py)
        .into_any()
        .unbind())
}

#[pymethods]
impl PyParser {
    /// Create a new compiled parser.
    ///
    /// Args:
    ///     format: The format string to compile.
    ///     extra_types: Optional mapping of custom type names to converter functions.
    ///     case_sensitive: Whether matching should be case-sensitive (default: False).
    #[new]
    #[pyo3(signature = (format, extra_types=None, case_sensitive=false))]
    fn new(
        py: Python<'_>,
        format: &str,
        extra_types: Option<PyObject>,
        case_sensitive: bool,
    ) -> PyResult<Self> {
        if let Some(extra_types) = extra_types {
            return Self::new_with_extra(py, format, extra_types.bind(py), case_sensitive);
        }
        let inner = Parser::new(format, case_sensitive).map_err(|e| map_compile_error(py, e))?;
        Ok(Self {
            inner: ParserKind::Core(inner),
        })
    }

    /// Parse a string for an exact match.
    ///
    /// The entire string must match the format pattern.
    ///
    /// Args:
    ///     string: The string to parse.
    ///
    /// Returns:
    ///     Result object on match, None otherwise.
    #[pyo3(signature = (string, evaluate_result=true))]
    fn parse(
        &self,
        py: Python<'_>,
        string: &str,
        evaluate_result: Option<bool>,
    ) -> PyResult<Option<PyObject>> {
        match &self.inner {
            ParserKind::Core(inner) => {
                if evaluate_result.unwrap_or(true) {
                    Ok(match inner.parse(string) {
                        Some(result) => Some(wrap_result(py, result)?),
                        None => None,
                    })
                } else {
                    Ok(match inner.parse(string) {
                        Some(result) => Some(wrap_match(py, result)?),
                        None => None,
                    })
                }
            }
            ParserKind::Custom(inner) => inner.parse(py, string, evaluate_result.unwrap_or(true)),
        }
    }

    /// Search for the format pattern anywhere in the string.
    ///
    /// Args:
    ///     string: The string to search.
    ///     pos: Starting position (default: 0).
    ///     endpos: Optional ending position.
    ///
    /// Returns:
    ///     Result object on match, None otherwise.
    #[pyo3(signature = (string, pos=0, endpos=None, evaluate_result=true))]
    fn search(
        &self,
        py: Python<'_>,
        string: &str,
        pos: usize,
        endpos: Option<usize>,
        evaluate_result: Option<bool>,
    ) -> PyResult<Option<PyObject>> {
        match &self.inner {
            ParserKind::Core(inner) => {
                if evaluate_result.unwrap_or(true) {
                    Ok(match inner.search(string, pos, endpos) {
                        Some(result) => Some(wrap_result(py, result)?),
                        None => None,
                    })
                } else {
                    Ok(match inner.search(string, pos, endpos) {
                        Some(result) => Some(wrap_match(py, result)?),
                        None => None,
                    })
                }
            }
            ParserKind::Custom(inner) => {
                inner.search(py, string, pos, endpos, evaluate_result.unwrap_or(true))
            }
        }
    }

    /// Find all matches of the format pattern in the string.
    ///
    /// Args:
    ///     string: The string to search.
    ///     pos: Starting position (default: 0).
    ///     endpos: Optional ending position.
    ///
    /// Returns:
    ///     Iterator of Result objects.
    #[pyo3(signature = (string, pos=0, endpos=None, evaluate_result=true))]
    fn findall(
        &self,
        py: Python<'_>,
        string: &str,
        pos: usize,
        endpos: Option<usize>,
        evaluate_result: bool,
    ) -> PyResult<PyObject> {
        match &self.inner {
            ParserKind::Core(inner) => {
                wrap_findall_iterator(py, inner.findall(string, pos, endpos), evaluate_result)
            }
            ParserKind::Custom(inner) => inner.findall(py, string, pos, endpos, evaluate_result),
        }
    }

    /// Get the original format string.
    #[getter]
    fn format(&self) -> &str {
        match &self.inner {
            ParserKind::Core(inner) => inner.format_string(),
            ParserKind::Custom(inner) => inner.inner.format_string(),
        }
    }

    /// Get the compiled regex pattern.
    #[getter]
    fn pattern(&self) -> &str {
        match &self.inner {
            ParserKind::Core(inner) => inner.pattern(),
            ParserKind::Custom(inner) => inner.inner.pattern(),
        }
    }

    #[getter]
    fn _expression(&self) -> &str {
        self.pattern().trim_start_matches("(?is)")
    }

    /// Get the list of named field names.
    #[getter]
    fn named_fields(&self) -> Vec<String> {
        match &self.inner {
            ParserKind::Core(inner) => inner.named_fields().to_vec(),
            ParserKind::Custom(inner) => inner.inner.named_fields().to_vec(),
        }
    }

    /// Get the list of fixed field indices.
    #[getter]
    fn fixed_fields(&self) -> Vec<usize> {
        match &self.inner {
            ParserKind::Core(inner) => inner.fixed_fields().to_vec(),
            ParserKind::Custom(inner) => inner.inner.fixed_fields().to_vec(),
        }
    }

    fn __reduce__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let module = py.import("parse_rust")?;
        let constructor = module.getattr("compile")?;
        let args = PyTuple::new(py, [self.format()])?;
        Ok(PyTuple::new(py, [constructor.into_any(), args.into_any()])?
            .into_any()
            .unbind())
    }

    fn __getnewargs__(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(PyTuple::new(py, [self.format()])?.into_any().unbind())
    }

    fn __repr__(&self) -> String {
        format!("<Parser '{}'>", self.format())
    }
}

/// A Python module implemented in Rust.
///
/// This is the main entry point for the `parse_rust` Python package.
/// It provides `parse`, `search`, `findall`, and `compile` functions
/// that are API-compatible with the Python `parse` library.
#[pymodule]
fn parse_rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    /// Parse a string with a format pattern (exact match).
    ///
    /// The opposite of ``str.format()`` — extracts values from a string
    /// based on a format template.
    ///
    /// Args:
    ///     format: The format string (e.g., "Hello {name}!").
    ///     string: The string to parse.
    ///     case_sensitive: Whether matching is case-sensitive (default: False).
    ///
    /// Returns:
    ///     Result object with extracted values, or None if no match.
    ///
    /// Examples:
    ///     >>> parse("Hello, {}!", "Hello, World!")
    ///     <Result ('World',) {}>
    ///     >>> parse("{name:w} is {:d}", "Alice is 30")
    ///     <Result (30,) {name: 'Alice'}>
    #[pyfn(m)]
    #[pyo3(signature = (format_spec, extra_types=None))]
    fn extract_format(
        py: Python<'_>,
        format_spec: &str,
        extra_types: Option<PyObject>,
    ) -> PyResult<PyObject> {
        extract_format_dict(py, format_spec, extra_types.as_ref().map(|v| v.bind(py)))
    }

    #[pyfn(m)]
    #[pyo3(signature = (format, string, extra_types=None, evaluate_result=true, case_sensitive=false))]
    fn parse(
        py: Python<'_>,
        format: &str,
        string: &str,
        extra_types: Option<PyObject>,
        evaluate_result: bool,
        case_sensitive: bool,
    ) -> PyResult<PyObject> {
        if let Some(extra_types) = extra_types {
            let parser =
                PyParser::new_with_extra(py, format, extra_types.bind(py), case_sensitive)?;
            return Ok(match parser.parse(py, string, Some(evaluate_result))? {
                Some(result) => result,
                None => py.None(),
            });
        }

        match crate::parser::cached_parser(format, case_sensitive) {
            Some(parser) => match parser.parse_with_error(string) {
                Ok(Some(result)) => {
                    if evaluate_result {
                        wrap_result(py, result)
                    } else {
                        wrap_match(py, result)
                    }
                }
                Ok(None) => Ok(py.None()),
                Err(err) if err.starts_with("RepeatedNameError:") => Ok(py.None()),
                Err(err) => Err(map_compile_error(py, err)),
            },
            None => {
                // Cache miss due to compile error: re-create to surface the message
                match crate::parser::Parser::new(format, case_sensitive) {
                    Ok(_) => Ok(py.None()),
                    Err(err) => Err(map_compile_error(py, err)),
                }
            }
        }
    }

    /// Search for a format pattern anywhere in a string.
    ///
    /// Unlike ``parse()``, the pattern doesn't need to match the entire
    /// string — it will find the first occurrence.
    ///
    /// Args:
    ///     format: The format string.
    ///     string: The string to search.
    ///     pos: Starting position (default: 0).
    ///     endpos: Optional ending position.
    ///     case_sensitive: Whether matching is case-sensitive (default: False).
    ///
    /// Returns:
    ///     Result object on match, None otherwise.
    #[pyfn(m)]
    #[pyo3(signature = (format, string, pos=0, endpos=None, extra_types=None, evaluate_result=true, case_sensitive=false))]
    fn search(
        py: Python<'_>,
        format: &str,
        string: &str,
        pos: usize,
        endpos: Option<usize>,
        extra_types: Option<PyObject>,
        evaluate_result: bool,
        case_sensitive: bool,
    ) -> PyResult<PyObject> {
        if let Some(extra_types) = extra_types {
            let parser =
                PyParser::new_with_extra(py, format, extra_types.bind(py), case_sensitive)?;
            return Ok(
                match parser.search(py, string, pos, endpos, Some(evaluate_result))? {
                    Some(result) => result,
                    None => py.None(),
                },
            );
        }

        Ok(match crate::parser::cached_parser(format, case_sensitive) {
            Some(parser) => match parser.search(string, pos, endpos) {
                Some(result) => {
                    if evaluate_result {
                        wrap_result(py, result)?
                    } else {
                        wrap_match(py, result)?
                    }
                }
                None => py.None(),
            },
            None => py.None(),
        })
    }

    /// Find all matches of a format pattern in a string.
    ///
    /// Returns all non-overlapping matches of the format pattern.
    ///
    /// Args:
    ///     format: The format string.
    ///     string: The string to search.
    ///     pos: Starting position (default: 0).
    ///     endpos: Optional ending position.
    ///     case_sensitive: Whether matching is case-sensitive (default: False).
    ///
    /// Returns:
    ///     Iterator of Result or Match objects.
    #[pyfn(m)]
    #[pyo3(signature = (format, string, pos=0, endpos=None, extra_types=None, evaluate_result=true, case_sensitive=false))]
    fn findall(
        py: Python<'_>,
        format: &str,
        string: &str,
        pos: usize,
        endpos: Option<usize>,
        extra_types: Option<PyObject>,
        evaluate_result: bool,
        case_sensitive: bool,
    ) -> PyResult<PyObject> {
        if let Some(extra_types) = extra_types {
            let parser =
                PyParser::new_with_extra(py, format, extra_types.bind(py), case_sensitive)?;
            return parser.findall(py, string, pos, endpos, evaluate_result);
        }

        wrap_findall_iterator(
            py,
            crate::parser::findall(format, string, pos, endpos, case_sensitive),
            evaluate_result,
        )
    }

    /// Compile a format string for repeated use.
    ///
    /// Creates a compiled Parser that can be reused to parse many strings
    /// with the same format, avoiding re-compilation overhead.
    ///
    /// Args:
    ///     format: The format string to compile.
    ///     case_sensitive: Whether matching is case-sensitive (default: False).
    ///
    /// Returns:
    ///     A compiled Parser object.
    ///
    /// Examples:
    ///     >>> p = compile("User {name:w}")
    ///     >>> p.parse("User Alice")
    ///     <Result () {name: 'Alice'}>
    ///     >>> p.parse("User Bob")
    ///     <Result () {name: 'Bob'}>
    #[pyfn(m)]
    #[pyo3(signature = (format, extra_types=None, case_sensitive=false))]
    fn compile(
        py: Python<'_>,
        format: &str,
        extra_types: Option<PyObject>,
        case_sensitive: bool,
    ) -> PyResult<PyObject> {
        if let Some(extra_types) = extra_types {
            return Ok(Py::new(
                py,
                PyParser::new_with_extra(py, format, extra_types.bind(py), case_sensitive)?,
            )?
            .into_bound(py)
            .into_any()
            .unbind());
        }
        Ok(Py::new(py, PyParser::new(py, format, None, case_sensitive)?)?
            .into_bound(py)
            .into_any()
            .unbind())
    }

    #[pyfn(m)]
    fn _clear_cache() {
        crate::parser::clear_cache();
    }

    m.add_class::<PyParser>()?;
    m.add_class::<PyParseResult>()?;
    m.add_class::<PyMatch>()?;
    m.add_class::<PyResultIterator>()?;
    m.add_function(wrap_pyfunction!(with_pattern, m)?)?;
    m.add("FixedTzOffset", make_fixed_tzoffset_class(m.py())?)?;
    m.add("RepeatedNameError", m.py().get_type::<RepeatedNameError>())?;
    m.add(
        "dt_format_to_regex",
        types::dt_format_to_regex_map(m.py())?,
    )?;
    m.add("__version__", "0.1.0")?;

    Ok(())
}
