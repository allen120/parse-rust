//! Result types for parse operations.
//!
//! Provides the `ParseResult` struct that holds parsed values,
//! supporting both positional (indexed) and named field access.

use chrono::Datelike;
use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods, PyTuple, PyTupleMethods};
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
    /// Span (start, end) positions for positional captures.
    pub fixed_spans: Vec<(usize, usize)>,
    /// Span (start, end) positions for named captures keyed by original field name.
    pub named_spans: HashMap<String, (usize, usize)>,
}

/// A parsed value that can be one of several types.
///
/// This enum represents the possible types that a format specifier
/// can produce after type conversion.
#[derive(Debug)]
pub enum ParseValue {
    /// A string value (default, or from :s, :l, :w, :W, :S, :D specifiers).
    Str(String),
    /// An integer value (from :d, :b, :o, :x, :n specifiers).
    Int(i64),
    /// A floating-point value (from :f, :e, :g specifiers).
    Float(f64),
    /// A Python object produced by a user-defined converter.
    PyObject(PyObject),
    /// A decimal value (from :F specifier), preserved as text for Python Decimal.
    Decimal(String),
    /// A datetime string value with its original format specifier.
    DateTime { raw: String, format: String },
    /// A percentage value (from :% specifier), stored as float (already divided by 100).
    Percent(f64),
    /// A nested mapping value used for bracket-style fields like `foo[bar]`.
    Map(HashMap<String, ParseValue>),
}

impl Clone for ParseValue {
    fn clone(&self) -> Self {
        match self {
            ParseValue::Str(value) => ParseValue::Str(value.clone()),
            ParseValue::Int(value) => ParseValue::Int(*value),
            ParseValue::Float(value) => ParseValue::Float(*value),
            ParseValue::PyObject(obj) => {
                ParseValue::PyObject(Python::with_gil(|py| obj.clone_ref(py)))
            }
            ParseValue::Decimal(value) => ParseValue::Decimal(value.clone()),
            ParseValue::DateTime { raw, format } => ParseValue::DateTime {
                raw: raw.clone(),
                format: format.clone(),
            },
            ParseValue::Percent(value) => ParseValue::Percent(*value),
            ParseValue::Map(items) => ParseValue::Map(items.clone()),
        }
    }
}

impl ParseValue {
    fn from_python(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if value.is_none() {
            return Ok(ParseValue::Str("None".to_string()));
        }
        if let Ok(dict) = value.downcast::<PyDict>() {
            let mut items = HashMap::new();
            for (key, item) in dict.iter() {
                items.insert(key.extract::<String>()?, ParseValue::from_python(&item)?);
            }
            return Ok(ParseValue::Map(items));
        }
        if let Ok(text) = value.extract::<String>() {
            return Ok(ParseValue::Str(text));
        }
        if let Ok(number) = value.extract::<i64>() {
            return Ok(ParseValue::Int(number));
        }
        if let Ok(number) = value.extract::<f64>() {
            return Ok(ParseValue::Float(number));
        }
        Ok(ParseValue::PyObject(value.clone().unbind()))
    }

    /// Convert this ParseValue to a Python object.
    pub fn to_pyobject(&self, py: Python<'_>) -> PyObject {
        match self {
            ParseValue::Str(s) => s.into_pyobject(py).unwrap().into_any().unbind(),
            ParseValue::Int(i) => i.into_pyobject(py).unwrap().into_any().unbind(),
            ParseValue::Float(f) => f.into_pyobject(py).unwrap().into_any().unbind(),
            ParseValue::PyObject(obj) => obj.clone_ref(py),
            ParseValue::Decimal(raw) => decimal_to_pyobject(py, raw)
                .unwrap_or_else(|_| raw.into_pyobject(py).unwrap().into_any().unbind()),
            ParseValue::DateTime { raw, format } => datetime_to_pyobject(py, raw, format)
                .unwrap_or_else(|_| raw.into_pyobject(py).unwrap().into_any().unbind()),
            ParseValue::Percent(f) => f.into_pyobject(py).unwrap().into_any().unbind(),
            ParseValue::Map(items) => {
                let dict = PyDict::new(py);
                for (key, value) in items {
                    dict.set_item(key, value.to_pyobject(py)).unwrap();
                }
                dict.into_any().unbind()
            }
        }
    }
}

impl PartialEq for ParseValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ParseValue::Str(a), ParseValue::Str(b)) => a == b,
            (ParseValue::Int(a), ParseValue::Int(b)) => a == b,
            (ParseValue::Float(a), ParseValue::Float(b)) => a == b,
            (ParseValue::Decimal(a), ParseValue::Decimal(b)) => a == b,
            (ParseValue::Percent(a), ParseValue::Percent(b)) => a == b,
            (
                ParseValue::DateTime { raw: a, format: af },
                ParseValue::DateTime { raw: b, format: bf },
            ) => a == b && af == bf,
            (ParseValue::Map(a), ParseValue::Map(b)) => a == b,
            (ParseValue::PyObject(_), ParseValue::PyObject(_)) => false,
            _ => false,
        }
    }
}

fn decimal_to_pyobject(py: Python<'_>, raw: &str) -> PyResult<PyObject> {
    Ok(PyModule::import(py, "decimal")?
        .getattr("Decimal")?
        .call1((raw,))?
        .into_any()
        .unbind())
}

fn parse_datetime_formats(
    datetime_cls: &Bound<'_, PyAny>,
    raw: &str,
    formats: &[&str],
) -> PyResult<PyObject> {
    for fmt in formats {
        if let Ok(value) = datetime_cls.call_method1("strptime", (raw, fmt)) {
            return Ok(value.into_any().unbind());
        }
    }

    Err(pyo3::exceptions::PyValueError::new_err(format!(
        "unsupported datetime format '{}'",
        raw
    )))
}

fn normalize_fractional_seconds(raw: &str) -> Option<String> {
    let (prefix, suffix) = raw.split_once('.')?;
    let (fraction, tz) = split_fraction_and_tz(suffix)?;
    let truncated = &fraction[..fraction.len().min(6)];
    let padded = format!("{truncated:0<6}");
    Some(format!("{prefix}.{padded}{tz}"))
}

fn parse_iso_datetime(datetime_cls: &Bound<'_, PyAny>, raw: &str) -> PyResult<PyObject> {
    let normalized = if let Some(stripped) = raw.strip_suffix('Z') {
        format!("{}+00:00", stripped)
    } else {
        raw.to_string()
    };
    let normalized = normalized.replace('T', " ");
    let normalized = normalized.replace(" +", "+");

    let direct_formats = [
        "%Y-%m-%dT%H:%M:%S.%f%z",
        "%Y-%m-%dT%H:%M:%S%z",
        "%Y-%m-%d %H:%M:%S.%f%z",
        "%Y-%m-%d %H:%M:%S%z",
        "%Y-%m-%dT%H:%M:%S.%f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S.%f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M%z",
        "%Y-%m-%d %H:%M%z",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
    ];
    if let Ok(value) = parse_datetime_formats(datetime_cls, &normalized, &direct_formats) {
        return Ok(value);
    }

    if let Some(rebuilt) = normalize_fractional_seconds(&normalized) {
        let formats = [
            "%Y-%m-%dT%H:%M:%S.%f%z",
            "%Y-%m-%d %H:%M:%S.%f%z",
            "%Y-%m-%dT%H:%M:%S.%f",
            "%Y-%m-%d %H:%M:%S.%f",
        ];
        if let Ok(value) = parse_datetime_formats(datetime_cls, &rebuilt, &formats) {
            return Ok(value);
        }
    }

    Err(pyo3::exceptions::PyValueError::new_err(format!(
        "unsupported datetime format '{}'",
        raw
    )))
}

fn split_fraction_and_tz(suffix: &str) -> Option<(&str, &str)> {
    if let Some(stripped) = suffix.strip_suffix('Z') {
        return Some((stripped, "Z"));
    }
    for (idx, ch) in suffix.char_indices() {
        if ch == '+' || ch == '-' {
            return Some((&suffix[..idx], &suffix[idx..]));
        }
    }
    Some((suffix, ""))
}

fn parse_global_datetime(datetime_cls: &Bound<'_, PyAny>, raw: &str) -> PyResult<PyObject> {
    let formats = [
        "%d-%B-%Y %I:%M:%S %p %z",
        "%d-%b-%Y %I:%M:%S %p %z",
        "%d-%m-%Y %I:%M:%S %p %z",
        "%d-%B-%Y %H:%M:%S %z",
        "%d-%b-%Y %H:%M:%S %z",
        "%d-%m-%Y %H:%M:%S %z",
        "%d-%B-%Y %I:%M %p",
        "%d-%b-%Y %I:%M %p",
        "%d-%m-%Y %I:%M %p",
        "%d-%B-%Y %H:%M",
        "%d-%b-%Y %H:%M",
        "%d-%m-%Y %H:%M",
        "%d-%B-%Y",
        "%d-%b-%Y",
        "%d-%m-%Y",
        "%d/%B/%Y %I:%M:%S %p %z",
        "%d/%b/%Y %I:%M:%S %p %z",
        "%d/%m/%Y %I:%M:%S %p %z",
        "%d/%B/%Y %H:%M:%S %z",
        "%d/%b/%Y %H:%M:%S %z",
        "%d/%m/%Y %H:%M:%S %z",
        "%d/%B/%Y %I:%M %p",
        "%d/%b/%Y %I:%M %p",
        "%d/%m/%Y %I:%M %p",
        "%d/%B/%Y %H:%M",
        "%d/%b/%Y %H:%M",
        "%d/%m/%Y %H:%M",
        "%d/%B/%Y",
        "%d/%b/%Y",
        "%d/%m/%Y",
    ];
    parse_datetime_formats(datetime_cls, raw, &formats)
}

fn parse_syslog_datetime(datetime_cls: &Bound<'_, PyAny>, raw: &str) -> PyResult<PyObject> {
    let value = datetime_cls.call_method1("strptime", (raw, "%b %d %H:%M:%S"))?;
    let year = chrono::Local::now().year();
    let kwargs = PyDict::new(value.py());
    kwargs.set_item("year", year)?;
    Ok(value
        .call_method("replace", (), Some(&kwargs))?
        .into_any()
        .unbind())
}

fn datetime_to_pyobject(py: Python<'_>, raw: &str, format: &str) -> PyResult<PyObject> {
    let dt_mod = PyModule::import(py, "datetime")?;
    let datetime_cls = dt_mod.getattr("datetime")?;

    match format {
        "ti" => parse_iso_datetime(&datetime_cls, raw),
        "te" => {
            let normalized = normalize_short_tz_offset(raw);
            parse_datetime_formats(
                &datetime_cls,
                &normalized,
                &["%a, %d %b %Y %H:%M:%S %z", "%d %b %Y %H:%M:%S %z"],
            )
        }
        "ta" => {
            let normalized = normalize_short_tz_offset(raw);
            let formats = [
                "%B-%d-%Y %I:%M:%S %p %z",
                "%m-%d-%Y %I:%M:%S %p %z",
                "%b-%d-%Y %I:%M:%S %p %z",
                "%B/%d/%Y %I:%M:%S %p %z",
                "%m/%d/%Y %I:%M:%S %p %z",
                "%b/%d/%Y %I:%M:%S %p %z",
                "%B/%d/%Y %H:%M:%S %p %z",
                "%m/%d/%Y %H:%M:%S %p %z",
                "%b/%d/%Y %H:%M:%S %p %z",
                "%B-%d-%Y %H:%M:%S %z",
                "%m-%d-%Y %H:%M:%S %z",
                "%b-%d-%Y %H:%M:%S %z",
                "%B/%d/%Y %H:%M:%S %z",
                "%m/%d/%Y %H:%M:%S %z",
                "%b/%d/%Y %H:%M:%S %z",
                "%B-%d-%Y %I:%M %p %z",
                "%m-%d-%Y %I:%M %p %z",
                "%b-%d-%Y %I:%M %p %z",
                "%B/%d/%Y %I:%M %p %z",
                "%m/%d/%Y %I:%M %p %z",
                "%b/%d/%Y %I:%M %p %z",
                "%B-%d-%Y %H:%M %z",
                "%m-%d-%Y %H:%M %z",
                "%b-%d-%Y %H:%M %z",
                "%B/%d/%Y %H:%M %z",
                "%m/%d/%Y %H:%M %z",
                "%b/%d/%Y %H:%M %z",
                "%B-%d-%Y %I:%M:%S %p",
                "%m-%d-%Y %I:%M:%S %p",
                "%b-%d-%Y %I:%M:%S %p",
                "%B/%d/%Y %I:%M:%S %p",
                "%m/%d/%Y %I:%M:%S %p",
                "%b/%d/Y %I:%M:%S %p",
                "%B-%d-%Y %H:%M:%S",
                "%m-%d-%Y %H:%M:%S",
                "%b-%d-%Y %H:%M:%S",
                "%B/%d/%Y %H:%M:%S",
                "%m/%d/%Y %H:%M:%S",
                "%b/%d/%Y %H:%M:%S",
                "%B-%d-%Y %I:%M %p",
                "%m-%d-%Y %I:%M %p",
                "%b-%d-%Y %I:%M %p",
                "%B/%d/%Y %I:%M %p",
                "%m/%d/%Y %I:%M %p",
                "%b/%d/%Y %I:%M %p",
                "%B/%d/%Y %H:%M",
                "%m/%d/%Y %H:%M",
                "%b/%d/%Y %H:%M",
                "%B-%d-%Y",
                "%m-%d-%Y",
                "%b-%d-%Y",
                "%B/%d/%Y",
                "%m/%d/%Y",
                "%b/%d/%Y",
            ];
            parse_datetime_formats(&datetime_cls, &normalized, &formats)
        }
        "tg" => {
            let normalized = normalize_short_tz_offset(raw);
            if let Ok(value) = parse_datetime_formats(
                &datetime_cls,
                &normalized,
                &[
                    "%d/%B/%Y %I:%M:%S %p %z",
                    "%d/%b/%Y %I:%M:%S %p %z",
                    "%d/%m/%Y %I:%M:%S %p %z",
                    "%d/%B/%Y %H:%M:%S %z",
                    "%d/%b/%Y %H:%M:%S %z",
                    "%d/%m/%Y %H:%M:%S %z",
                    "%d/%B/%Y %H:%M:%S",
                    "%d/%b/%Y %H:%M:%S",
                    "%d/%m/%Y %H:%M:%S",
                    "%d/%B/%Y %I:%M %p",
                    "%d/%b/%Y %I:%M %p",
                    "%d/%m/%Y %I:%M %p",
                    "%d/%B/%Y %H:%M",
                    "%d/%b/%Y %H:%M",
                    "%d/%m/%Y %H:%M",
                    "%m/%d/%Y %H:%M",
                    "%d/%B/%Y",
                    "%d/%b/%Y",
                    "%d/%m/%Y",
                ],
            ) {
                Ok(value)
            } else {
                parse_global_datetime(&datetime_cls, &normalized)
            }
        }
        "th" => {
            let normalized = normalize_short_tz_offset(raw);
            parse_datetime_formats(&datetime_cls, &normalized, &["%d/%b/%Y:%H:%M:%S %z"])
        }
        "tc" => parse_datetime_formats(&datetime_cls, raw, &["%a %b %d %H:%M:%S %Y"]),
        "ts" => parse_syslog_datetime(&datetime_cls, raw),
        "tt" => {
            let normalized = normalize_short_tz_offset(raw);
            let formats = [
                "%I:%M:%S.%f %p %z",
                "%I:%M:%S %p %z",
                "%I:%M:%S.%f %p",
                "%I:%M:%S %p",
                "%H:%M:%S.%f %p %z",
                "%H:%M:%S %p %z",
                "%H:%M:%S.%f %p",
                "%H:%M:%S %p",
                "%H:%M:%S.%f %z",
                "%H:%M:%S %z",
                "%H:%M:%S.%f",
                "%H:%M:%S",
                "%I:%M %p %z",
                "%I:%M %p",
                "%H:%M %p %z",
                "%H:%M %p",
                "%H:%M %z",
                "%H:%M",
            ];
            for fmt in formats {
                if let Ok(value) = datetime_cls.call_method1("strptime", (normalized.as_str(), fmt)) {
                    let method = if fmt.contains("%z") { "timetz" } else { "time" };
                    return Ok(value.call_method0(method)?.into_any().unbind());
                }
            }
            Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unsupported time format '{}'",
                raw
            )))
        }
        custom if custom.contains('%') => {
            let mut parsed = datetime_cls.call_method1("strptime", (raw, custom))?;
            let normalized = normalize_short_tz_offset(raw);
            if normalized != raw {
                if let Ok(value) = datetime_cls.call_method1("strptime", (normalized.as_str(), custom)) {
                    parsed = value;
                }
            }
            let is_date = [
                "%a", "%A", "%w", "%d", "%b", "%B", "%m", "%y", "%Y", "%j", "%U", "%W",
            ]
            .iter()
            .any(|token| custom.contains(token));
            let is_time = ["%H", "%I", "%p", "%M", "%S", "%f", "%z"]
                .iter()
                .any(|token| custom.contains(token));
            let has_year = custom.contains("%Y") || custom.contains("%y");

            if !has_year {
                let year = chrono::Local::now().year();
                let kwargs = PyDict::new(py);
                kwargs.set_item("year", year)?;
                parsed = parsed.call_method("replace", (), Some(&kwargs))?;
            }

            if is_date && is_time {
                Ok(parsed.into_any().unbind())
            } else if is_date {
                Ok(parsed.call_method0("date")?.into_any().unbind())
            } else if is_time {
                Ok(parsed.call_method0("time")?.into_any().unbind())
            } else {
                Ok(parsed.into_any().unbind())
            }
        }
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unsupported datetime format '{}'",
            format
        ))),
    }
}

fn normalize_short_tz_offset(raw: &str) -> String {
    let Some(sign_pos) = raw.rfind(['+', '-']) else {
        return raw.to_string();
    };
    let suffix = &raw[sign_pos + 1..];
    let sign = &raw[sign_pos..sign_pos + 1];
    let prefix = &raw[..sign_pos];
    if let Some((hours, minutes)) = suffix.split_once(':') {
        if hours.len() == 1 && minutes.len() == 2 {
            return format!("{}{}0{}:{}", prefix, sign, hours, minutes);
        }
    } else if suffix.len() == 3 && suffix.chars().all(|c| c.is_ascii_digit()) {
        return format!("{}{}0{}", prefix, sign, suffix);
    }
    raw.to_string()
}

impl ParseResult {
    /// Create a new empty ParseResult.
    pub fn new() -> Self {
        Self {
            fixed: Vec::new(),
            named: HashMap::new(),
            fixed_spans: Vec::new(),
            named_spans: HashMap::new(),
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

    pub fn get_fixed_span(&self, index: usize) -> Option<(usize, usize)> {
        self.fixed_spans.get(index).copied()
    }

    pub fn get_named_span(&self, key: &str) -> Option<(usize, usize)> {
        self.named_spans.get(key).copied()
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
    #[new]
    #[pyo3(signature = (fixed, named, spans=None))]
    fn new(
        fixed: &Bound<'_, PyAny>,
        named: &Bound<'_, PyAny>,
        spans: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let fixed_tuple = fixed.downcast::<PyTuple>()?;
        let named_dict = named.downcast::<PyDict>()?;

        let mut fixed_values = Vec::new();
        for item in fixed_tuple.iter() {
            fixed_values.push(ParseValue::from_python(&item)?);
        }

        let mut named_values = HashMap::new();
        for (key, value) in named_dict.iter() {
            named_values.insert(key.extract::<String>()?, ParseValue::from_python(&value)?);
        }

        let mut result = ParseResult {
            fixed: fixed_values,
            named: named_values,
            fixed_spans: Vec::new(),
            named_spans: HashMap::new(),
        };

        if let Some(spans) = spans {
            if !spans.is_none() {
                let spans_dict = spans.downcast::<PyDict>()?;
                for (key, value) in spans_dict.iter() {
                    let span = value.extract::<(usize, usize)>()?;
                    if let Ok(index) = key.extract::<usize>() {
                        if result.fixed_spans.len() <= index {
                            result.fixed_spans.resize(index + 1, (0, 0));
                        }
                        result.fixed_spans[index] = span;
                    } else {
                        result.named_spans.insert(key.extract::<String>()?, span);
                    }
                }
            }
        }

        Ok(Self { inner: result })
    }

    /// Access parsed values by index (int) or name (str).
    ///
    /// Examples:
    ///     result[0]       # first positional field
    ///     result['name']  # named field 'name'
    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        if let Ok(name) = key.extract::<String>() {
            match self.inner.get_named(&name) {
                Some(val) => Ok(val.to_pyobject(py)),
                None => Err(pyo3::exceptions::PyKeyError::new_err(format!(
                    "field '{}' not found",
                    name
                ))),
            }
        } else {
            let fixed = self.fixed(py)?;
            fixed
                .bind(py)
                .get_item(key)
                .map(|item| item.into_any().unbind())
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
        let items: Vec<PyObject> = self.inner.fixed.iter().map(|v| v.to_pyobject(py)).collect();
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
        for (index, (start, end)) in self.inner.fixed_spans.iter().enumerate() {
            let span_tuple = PyTuple::new(py, &[*start, *end])?;
            dict.set_item(index, span_tuple)?;
        }
        for (key, (start, end)) in &self.inner.named_spans {
            let span_tuple = PyTuple::new(py, &[*start, *end])?;
            dict.set_item(key, span_tuple)?;
        }
        Ok(dict.into_any().unbind())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::Python;

    #[test]
    fn test_normalize_short_tz_offset_hour_colon() {
        assert_eq!(normalize_short_tz_offset("10:21:36 PM +1:00"), "10:21:36 PM +01:00");
    }

    #[test]
    fn test_normalize_short_tz_offset_three_digits() {
        assert_eq!(normalize_short_tz_offset("10:21:36 PM -530"), "10:21:36 PM -0530");
    }

    #[test]
    fn test_normalize_fractional_seconds_padding_and_truncation() {
        assert_eq!(
            normalize_fractional_seconds("2023-10-14T15:09:08.1Z").as_deref(),
            Some("2023-10-14T15:09:08.100000Z")
        );
        assert_eq!(
            normalize_fractional_seconds("2023-10-14T15:09:08.1234567Z").as_deref(),
            Some("2023-10-14T15:09:08.123456Z")
        );
    }

    #[test]
    fn test_parse_iso_datetime_supports_spaced_timezone() {
        Python::with_gil(|py| {
            let datetime_cls = PyModule::import(py, "datetime").unwrap().getattr("datetime").unwrap();
            let value = parse_iso_datetime(&datetime_cls, "1997-07-16T19:20 +01:00").unwrap();
            let repr = value.bind(py).repr().unwrap().to_string();
            assert!(repr.contains("1997, 7, 16, 19, 20"));
            assert!(repr.contains("datetime.datetime"));
        });
    }
}
