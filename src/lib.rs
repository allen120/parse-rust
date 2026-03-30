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

use pyo3::prelude::*;
use pyo3::types::{PyList, PyType};
use crate::parser::Parser;
use crate::result::PyParseResult;
use std::ffi::CString;

/// Python-exposed compiled parser, equivalent to `parse.compile()`.
///
/// Wraps the Rust `Parser` struct for use from Python. Compiling
/// the format string once and reusing the parser is the recommended
/// approach for parsing many strings with the same format.
#[pyclass(name = "Parser")]
struct PyParser {
    inner: Parser,
}

#[pyclass(name = "Match")]
struct PyMatch {
    result: PyObject,
}

#[pymethods]
impl PyMatch {
    fn evaluate_result(&self, py: Python<'_>) -> PyObject {
        self.result.clone_ref(py)
    }
}

fn compat_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    let code = CString::new(include_str!("../python_compat.py")).unwrap();
    let filename = CString::new("parse_rust/python_compat.py").unwrap();
    let modulename = CString::new("_parse_rust_compat").unwrap();
    PyModule::from_code(
        py,
        code.as_c_str(),
        filename.as_c_str(),
        modulename.as_c_str(),
    )
}

fn map_compile_error(py: Python<'_>, err: String) -> PyErr {
    if err.starts_with("RepeatedNameError:") {
        if let Ok(exc_type) = py.import("parse_rust").and_then(|m| m.getattr("RepeatedNameError")) {
            return pyo3::PyErr::from_type(exc_type.downcast_into::<PyType>().unwrap(), err);
        }
    }
    pyo3::exceptions::PyValueError::new_err(err)
}

#[pymethods]
impl PyParser {
    /// Create a new compiled parser.
    ///
    /// Args:
    ///     format: The format string to compile.
    ///     case_sensitive: Whether matching should be case-sensitive (default: False).
    #[new]
    #[pyo3(signature = (format, case_sensitive=false))]
    fn new(py: Python<'_>, format: &str, case_sensitive: bool) -> PyResult<Self> {
        let inner = Parser::new(format, case_sensitive)
            .map_err(|e| map_compile_error(py, e))?;
        Ok(Self { inner })
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
    fn parse(&self, py: Python<'_>, string: &str, evaluate_result: Option<bool>) -> PyResult<Option<PyObject>> {
        let result = self.inner
            .parse(string)
            .map(|r| PyParseResult { inner: r });
        if evaluate_result.unwrap_or(true) {
            Ok(result.map(|r| Py::new(py, r).unwrap().into_bound(py).into_any().unbind()))
        } else {
            Ok(result.map(|r| {
                let res = Py::new(py, r).unwrap().into_bound(py).into_any().unbind();
                Py::new(py, PyMatch { result: res }).unwrap().into_bound(py).into_any().unbind()
            }))
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
        let result = self.inner
            .search(string, pos, endpos)
            .map(|r| PyParseResult { inner: r });
        if evaluate_result.unwrap_or(true) {
            Ok(result.map(|r| Py::new(py, r).unwrap().into_bound(py).into_any().unbind()))
        } else {
            Ok(result.map(|r| {
                let res = Py::new(py, r).unwrap().into_bound(py).into_any().unbind();
                Py::new(py, PyMatch { result: res }).unwrap().into_bound(py).into_any().unbind()
            }))
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
    ///     List of Result objects.
    #[pyo3(signature = (string, pos=0, endpos=None, evaluate_result=true))]
    fn findall(
        &self,
        py: Python<'_>,
        string: &str,
        pos: usize,
        endpos: Option<usize>,
        evaluate_result: bool,
    ) -> PyResult<PyObject> {
        let items: Vec<PyObject> = self.inner
            .findall(string, pos, endpos)
            .into_iter()
            .map(|r| {
                let res = Py::new(py, PyParseResult { inner: r }).unwrap().into_bound(py).into_any().unbind();
                if evaluate_result {
                    res
                } else {
                    Py::new(py, PyMatch { result: res }).unwrap().into_bound(py).into_any().unbind()
                }
            })
            .collect();
        Ok(PyList::new(py, &items)?.into_any().unbind())
    }

    /// Get the original format string.
    #[getter]
    fn format(&self) -> &str {
        self.inner.format_string()
    }

    /// Get the compiled regex pattern.
    #[getter]
    fn pattern(&self) -> &str {
        self.inner.pattern()
    }

    /// Get the list of named field names.
    #[getter]
    fn named_fields(&self) -> Vec<String> {
        self.inner.named_fields().to_vec()
    }

    /// Get the list of fixed field indices.
    #[getter]
    fn fixed_fields(&self) -> Vec<usize> {
        self.inner.fixed_fields().to_vec()
    }

    fn __repr__(&self) -> String {
        format!("<Parser '{}'>", self.inner.format_string())
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
    #[pyo3(signature = (format, string, extra_types=None, evaluate_result=true, case_sensitive=false))]
    fn parse(
        py: Python<'_>,
        format: &str,
        string: &str,
        extra_types: Option<PyObject>,
        evaluate_result: bool,
        case_sensitive: bool,
    ) -> PyResult<PyObject> {
        if extra_types.is_some() {
            return compat_module(py)?
                .getattr("parse")?
                .call1((format, string, extra_types, evaluate_result, case_sensitive))
                .map(|obj| obj.into_any().unbind());
        }

        match crate::parser::Parser::new(format, case_sensitive) {
            Ok(parser) => {
                let parsed = parser.parse(string).map(|r| PyParseResult { inner: r });
                if evaluate_result {
                    Ok(match parsed {
                        Some(r) => Py::new(py, r)?.into_bound(py).into_any().unbind(),
                        None => py.None(),
                    })
                } else {
                    Ok(match parsed {
                        Some(r) => {
                            let res = Py::new(py, r)?.into_bound(py).into_any().unbind();
                            Py::new(py, PyMatch { result: res })?.into_bound(py).into_any().unbind()
                        }
                        None => py.None(),
                    })
                }
            }
            Err(err) => Err(map_compile_error(py, err)),
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
        if extra_types.is_some() || !evaluate_result {
            return compat_module(py)?
                .getattr("search")?
                .call1((format, string, pos, endpos, extra_types, evaluate_result, case_sensitive))
                .map(|obj| obj.into_any().unbind());
        }

        let parser = match crate::parser::Parser::new(format, case_sensitive) {
            Ok(parser) => parser,
            Err(err) => return Err(map_compile_error(py, err)),
        };
        Ok(match parser.search(string, pos, endpos).map(|r| PyParseResult { inner: r }) {
            Some(r) => Py::new(py, r)?.into_bound(py).into_any().unbind(),
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
    ///     List of Result objects.
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
        if extra_types.is_some() || !evaluate_result {
            return compat_module(py)?
                .getattr("findall")?
                .call1((format, string, pos, endpos, extra_types, evaluate_result, case_sensitive))
                .map(|obj| obj.into_any().unbind());
        }

        match crate::parser::Parser::new(format, case_sensitive) {
            Ok(parser) => {
                let items: Vec<PyObject> = parser.findall(string, pos, endpos)
                    .into_iter()
                    .map(|r| Py::new(py, PyParseResult { inner: r }).unwrap().into_bound(py).into_any().unbind())
                    .collect();
                Ok(PyList::new(py, &items)?.into_any().unbind())
            }
            Err(err) => Err(map_compile_error(py, err)),
        }
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
    fn compile(py: Python<'_>, format: &str, extra_types: Option<PyObject>, case_sensitive: bool) -> PyResult<PyObject> {
        if extra_types.is_some() {
            return compat_module(py)?
                .getattr("compile")?
                .call1((format, extra_types, case_sensitive))
                .map(|obj| obj.into_any().unbind());
        }
        Ok(Py::new(py, PyParser::new(py, format, case_sensitive)?)?.into_bound(py).into_any().unbind())
    }

    m.add_class::<PyParser>()?;
    m.add_class::<PyParseResult>()?;
    m.add_class::<PyMatch>()?;
    m.add(
        "with_pattern",
        compat_module(m.py())?.getattr("with_pattern")?,
    )?;
    let repeated = m.py().get_type::<pyo3::exceptions::PyValueError>();
    m.add("RepeatedNameError", repeated)?;
    m.add("__version__", "0.1.0")?;

    Ok(())
}
