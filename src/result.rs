//! Result types for parse operations.
//!
//! Provides the `ParseResult` struct that holds parsed values,
//! supporting both positional (indexed) and named field access.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::collections::HashMap;

/// Represents the result of a successful parse operation.
///
/// Contains both positional (fixed) and named captured values,
/// along with span information indicating where each field was
/// matched in the original string.
#[derive(Debug, Clone)]
pub struct ParseResult {
    /// Positional captured values, in order of appearance.
    pub fixed: Vec<ParseValue>,
    /// Named captured values, keyed by field name.
    pub named: HashMap<String, ParseValue>,
    /// Span (start, end) positions for each captured field.
    /// Keys are field names (String) or indices (as string).
    pub spans: HashMap<String, (usize, usize)>,
}

/// A parsed value that can be one of several types.
///
/// This enum represents the possible types that a format specifier
/// can produce after type conversion.
#[derive(Debug, Clone)]
pub enum ParseValue {
    /// A string value (default, or from :s, :l, :w, :W, :S, :D specifiers).
    Str(String),
    /// An integer value (from :d, :b, :o, :x, :n specifiers).
    Int(i64),
    /// A floating-point value (from :f, :e, :g specifiers).
    Float(f64),
    /// A datetime string value (from :ti, :te, etc. specifiers).
    /// Stored as the matched string for Python-side conversion.
    DateTime(String),
    /// A percentage value (from :% specifier), stored as float (already divided by 100).
    Percent(f64),
}

impl ParseValue {
    /// Convert this ParseValue to a Python object.
    pub fn to_pyobject(&self, py: Python<'_>) -> PyObject {
        match self {
            ParseValue::Str(s) => s.into_pyobject(py).unwrap().into_any().unbind(),
            ParseValue::Int(i) => i.into_pyobject(py).unwrap().into_any().unbind(),
            ParseValue::Float(f) => f.into_pyobject(py).unwrap().into_any().unbind(),
            ParseValue::DateTime(s) => s.into_pyobject(py).unwrap().into_any().unbind(),
            ParseValue::Percent(f) => f.into_pyobject(py).unwrap().into_any().unbind(),
        }
    }
}

impl ParseResult {
    /// Create a new empty ParseResult.
    pub fn new() -> Self {
        Self {
            fixed: Vec::new(),
            named: HashMap::new(),
            spans: HashMap::new(),
        }
    }

    /// Get a fixed (positional) field by index.
    pub fn get_fixed(&self, index: usize) -> Option<&ParseValue> {
        self.fixed.get(index)
    }

    /// Get a named field by name.
    pub fn get_named(&self, name: &str) -> Option<&ParseValue> {
        self.named.get(name)
    }

    /// Get the span of a field by its key (name or index as string).
    pub fn get_span(&self, key: &str) -> Option<(usize, usize)> {
        self.spans.get(key).copied()
    }
}

/// Python wrapper for ParseResult, exposed as `Result` to Python.
///
/// Supports indexing by integer (for positional fields) and by
/// string (for named fields), matching the original parse library API.
#[pyclass(name = "Result")]
pub struct PyParseResult {
    pub inner: ParseResult,
}

#[pymethods]
impl PyParseResult {
    /// Access parsed values by index (int) or name (str).
    ///
    /// Examples:
    ///     result[0]       # first positional field
    ///     result['name']  # named field 'name'
    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        if let Ok(index) = key.extract::<usize>() {
            match self.inner.get_fixed(index) {
                Some(val) => Ok(val.to_pyobject(py)),
                None => Err(pyo3::exceptions::PyIndexError::new_err(
                    format!("index {} out of range", index),
                )),
            }
        } else if let Ok(name) = key.extract::<String>() {
            match self.inner.get_named(&name) {
                Some(val) => Ok(val.to_pyobject(py)),
                None => Err(pyo3::exceptions::PyKeyError::new_err(
                    format!("field '{}' not found", name),
                )),
            }
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "indices must be integers or strings",
            ))
        }
    }

    /// Check if a named field exists in the result.
    fn __contains__(&self, name: &str) -> bool {
        self.inner.named.contains_key(name)
    }

    /// String representation of the result.
    fn __repr__(&self) -> String {
        let fixed_strs: Vec<String> = self
            .inner
            .fixed
            .iter()
            .map(|v| format!("{:?}", v))
            .collect();
        let named_strs: Vec<String> = self
            .inner
            .named
            .iter()
            .map(|(k, v)| format!("{}: {:?}", k, v))
            .collect();
        format!(
            "<Result ({}) {{{}}}>",
            fixed_strs.join(", "),
            named_strs.join(", ")
        )
    }

    /// Get the fixed (positional) results as a Python tuple.
    #[getter]
    fn fixed(&self, py: Python<'_>) -> PyResult<PyObject> {
        let items: Vec<PyObject> = self
            .inner
            .fixed
            .iter()
            .map(|v| v.to_pyobject(py))
            .collect();
        Ok(PyTuple::new(py, &items)?.into_any().unbind())
    }

    /// Get the named results as a Python dict.
    #[getter]
    fn named(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (key, val) in &self.inner.named {
            dict.set_item(key, val.to_pyobject(py))?;
        }
        Ok(dict.into_any().unbind())
    }

    /// Get the span information as a Python dict.
    #[getter]
    fn spans(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (key, (start, end)) in &self.inner.spans {
            let span_tuple = PyTuple::new(py, &[*start, *end])?;
            dict.set_item(key, span_tuple)?;
        }
        Ok(dict.into_any().unbind())
    }
}
