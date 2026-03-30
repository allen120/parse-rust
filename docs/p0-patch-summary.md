# Compatibility Patch Summary

This document summarizes the compatibility patches applied so far:

- `71d6307` - `Improve P0 compatibility gaps`
- `62f7514` - `Implement P1 compatibility semantics`
- Pending P2 follow-up in the current worktree

## Scope

Goal: close the most visible gaps discovered during side-by-side testing with `parse-original`.

Covered by P0:

- Negative numeric parsing for float-like formats
- Repeated named field compatibility
- Datetime/time/custom `%...` format conversion to native Python objects
- Python-side error mapping and regression tests

Covered by P1:

- Alignment semantics (`<`, `>`, `^`)
- Width / precision handling for strings and numeric fields
- `spans` parity for positional and named keys
- Nested named-field expansion such as `foo[bar]`

## P0 Changes By File

### `src/compiler.rs`

- Added `format_string` to `FormatSpec` at [compiler.rs](/mnt/d/openproject/parse-rust/src/compiler.rs:28) so downstream conversion can see the original format token such as `tt` or `%Y-%m-%d`.
- Added `group_name` and `repeated_of` to `FieldInfo` at [compiler.rs](/mnt/d/openproject/parse-rust/src/compiler.rs:60) to support repeated named-field handling without relying on unsupported regex backreferences.
- Added `name_types` tracking in the compiler state at [compiler.rs](/mnt/d/openproject/parse-rust/src/compiler.rs:117) to detect type mismatches like `{name:w} {name:d}`.
- Updated field compilation logic at [compiler.rs](/mnt/d/openproject/parse-rust/src/compiler.rs:251) to:
  - preserve the raw format text,
  - add optional numeric sign handling consistently,
  - compile repeated named fields into a second capture group plus later equality validation,
  - return a `RepeatedNameError`-style compile error when the repeated field type differs.
- Extended `parse_format_spec` at [compiler.rs](/mnt/d/openproject/parse-rust/src/compiler.rs:356) so unknown format strings containing `%` are treated as custom datetime formats instead of hard failure.

Reason: P0 tests showed failures for negative floats, repeated named fields, and custom datetime syntax.

### `src/types.rs`

- Added `CustomDateTime(String)` to `FormatType` at [types.rs](/mnt/d/openproject/parse-rust/src/types.rs:14).
- Changed `regex_pattern()` at [types.rs](/mnt/d/openproject/parse-rust/src/types.rs:109) to return `String` and added regex generation for custom `%...` datetime formats.
- Relaxed numeric regex definitions at [types.rs](/mnt/d/openproject/parse-rust/src/types.rs:118) so sign handling can be applied centrally by the compiler.
- Changed `convert_value()` at [types.rs](/mnt/d/openproject/parse-rust/src/types.rs:208) to accept `FormatSpec` instead of only `FormatType`, preserving both the raw datetime text and its original format string.
- Added `datetime_format_to_regex()` at [types.rs](/mnt/d/openproject/parse-rust/src/types.rs:253) to translate common `strptime` tokens into regex.

Reason: the original implementation could not distinguish `ti` from `%Y-%m-%d` at conversion time, and could not support native datetime conversion later.

### `src/parser.rs`

- Updated parse/search/findall evaluation flow at [parser.rs](/mnt/d/openproject/parse-rust/src/parser.rs:219) so `evaluate_captures()` can reject invalid repeated-name matches by returning `None`.
- Switched conversion to use full field specs at [parser.rs](/mnt/d/openproject/parse-rust/src/parser.rs:240).
- Added repeated-field equality checks at [parser.rs](/mnt/d/openproject/parse-rust/src/parser.rs:256).
- Added `parse_values_equal()` at [parser.rs](/mnt/d/openproject/parse-rust/src/parser.rs:307) for typed repeated-field comparison.

Reason: Rust `regex` does not support Python-style named backreferences, so semantic equality has to be enforced after capture extraction.

### `src/result.rs`

- Replaced `ParseValue::DateTime(String)` with structured datetime metadata at [result.rs](/mnt/d/openproject/parse-rust/src/result.rs:31).
- Added `datetime_to_pyobject()` at [result.rs](/mnt/d/openproject/parse-rust/src/result.rs:61) to convert:
  - `ti` into `datetime`,
  - `te` into `datetime`,
  - `tt` into `time`,
  - custom `%...` formats into `date`, `time`, or `datetime` depending on the token set.
- Added `%z`-aware `timetz()` handling at [result.rs](/mnt/d/openproject/parse-rust/src/result.rs:105) so time-only values preserve timezone information.

Reason: previous behavior returned raw strings for datetime-like fields, which broke compatibility checks against `parse-original`.

### `src/lib.rs`

- Added compile error mapping at [lib.rs](/mnt/d/openproject/parse-rust/src/lib.rs:65) so repeated-name type mismatches surface as `RepeatedNameError` instead of generic `ValueError`.
- Updated Python `parse()` and `compile()` entry points at [lib.rs](/mnt/d/openproject/parse-rust/src/lib.rs:203) to use the new error mapping path.
- Exposed `RepeatedNameError` on the Python module at [lib.rs](/mnt/d/openproject/parse-rust/src/lib.rs:277).

Reason: compatibility tests expect a specific exception name for repeated-field type mismatches.

### `tests/test_integration.py`

- Added negative float regression coverage at [test_integration.py](/mnt/d/openproject/parse-rust/tests/test_integration.py:48).
- Added repeated named-field behavior tests at [test_integration.py](/mnt/d/openproject/parse-rust/tests/test_integration.py:234).
- Added datetime/time/custom-format regression tests at [test_integration.py](/mnt/d/openproject/parse-rust/tests/test_integration.py:250).

Reason: these were the concrete failures observed in the side-by-side validation run and should remain guarded.

## P0 Result

Validated after this patch:

- `cargo test --quiet` passed
- `python -m pytest -q tests` passed with `29 passed`
- The focused compatibility comparison set moved from several mismatches to `24 / 24` matched cases against `parse-original`

## P1 Changes By File

### `src/compiler.rs`

- Added `build_field_pattern()` at [compiler.rs](/mnt/d/openproject/parse-rust/src/compiler.rs:365) to centralize field-regex generation for:
  - alignment-aware padding placement,
  - width / precision handling for plain string fields,
  - width-aware decimal patterns,
  - keeping padding outside the capture group so parsed values are trimmed like `parse-original`.
- Updated the normal and repeated-field compilation paths at [compiler.rs](/mnt/d/openproject/parse-rust/src/compiler.rs:298) and [compiler.rs](/mnt/d/openproject/parse-rust/src/compiler.rs:331) to use the shared builder.

Reason: the previous implementation captured padding together with the value and ignored most width / precision semantics.

### `src/parser.rs`

- Changed capture extraction at [parser.rs](/mnt/d/openproject/parse-rust/src/parser.rs:222) to keep both the matched value and its span.
- Added `fixed_spans` and `named_spans` population logic at [parser.rs](/mnt/d/openproject/parse-rust/src/parser.rs:250).
- Added named-field expansion helpers at [parser.rs](/mnt/d/openproject/parse-rust/src/parser.rs:313) so flat keys like `hello[foo][baz]` become nested dictionaries in the final Python-visible result.

Reason: P1 parity requires result structure and span reporting to match `parse-original`, not just the raw parsed values.

### `src/result.rs`

- Reworked `ParseResult` span storage at [result.rs](/mnt/d/openproject/parse-rust/src/result.rs:16) into separate positional and named collections.
- Added `ParseValue::Map` at [result.rs](/mnt/d/openproject/parse-rust/src/result.rs:31) so nested field expansion can be represented natively before Python conversion.
- Updated Python `spans` export at [result.rs](/mnt/d/openproject/parse-rust/src/result.rs:269) to emit integer keys for positional fields and string keys for named fields, matching `parse-original`.

Reason: the old single `HashMap<String, ...>` could not preserve positional span keys or nested result values correctly.

### `tests/test_integration.py`

- Added alignment regression tests at [test_integration.py](/mnt/d/openproject/parse-rust/tests/test_integration.py:286).
- Added width / precision regression tests at [test_integration.py](/mnt/d/openproject/parse-rust/tests/test_integration.py:310).
- Added `spans` parity test at [test_integration.py](/mnt/d/openproject/parse-rust/tests/test_integration.py:325).
- Added nested named-field expansion test at [test_integration.py](/mnt/d/openproject/parse-rust/tests/test_integration.py:334).

Reason: these are the concrete P1 behaviors validated against `parse-original` and should stay covered.

## P1 Result

Validated after the P1 patch:

- `cargo test --quiet` passed
- `python -m pytest -q tests` passed with `36 passed`
- A focused P1 comparison set covering alignment, width, precision, `spans`, and nested fields matched `parse-original` in `11 / 11` cases

## P2 Changes By File

### `src/lib.rs`

- Added `PyMatch` at [lib.rs](/mnt/d/openproject/parse-rust/src/lib.rs:66) to expose `evaluate_result()` for deferred result evaluation.
- Added `compat_module()` at [lib.rs](/mnt/d/openproject/parse-rust/src/lib.rs:78) to load the embedded Python compatibility layer from `python_compat.py`.
- Extended `PyParser.parse/search/findall` at [lib.rs](/mnt/d/openproject/parse-rust/src/lib.rs:123) to support `evaluate_result=...` and return `Match` wrappers when requested.
- Extended top-level `parse/search/findall/compile` at [lib.rs](/mnt/d/openproject/parse-rust/src/lib.rs:257) to accept `extra_types` and delegate those cases to the compatibility layer.
- Exposed `with_pattern` on the Python module at [lib.rs](/mnt/d/openproject/parse-rust/src/lib.rs:416).

Reason: these API features are required for parity with `parse-original`, but are orthogonal to the Rust fast path and are cheaper to preserve through a focused Python fallback.

### `python_compat.py`

- Added a minimal compatibility parser at [python_compat.py](/mnt/d/openproject/parse-rust/python_compat.py:1) covering:
  - `with_pattern`,
  - `extra_types`,
  - `evaluate_result`,
  - regex group-count handling for user-defined converters,
  - Python-side `Parser`, `Result`, `Match`, and `ResultIterator`.

Reason: `parse-original` semantics for custom converters and deferred evaluation are API-heavy. Reusing a compact Python implementation avoids pushing complexity into the Rust matcher where it would add risk for the hot path.

### `tests/test_integration.py`

- Added `evaluate_result=False` coverage for top-level `parse/search/findall` and compiled parser methods at [test_integration.py](/mnt/d/openproject/parse-rust/tests/test_integration.py:347).
- Added `with_pattern` and `regex_group_count` compatibility tests at [test_integration.py](/mnt/d/openproject/parse-rust/tests/test_integration.py:381).
- Added `extra_types` override coverage for both `parse()` and `compile()` at [test_integration.py](/mnt/d/openproject/parse-rust/tests/test_integration.py:429).

Reason: these were the remaining uncovered differences from the side-by-side compatibility review.

## P2 Result

Validated after the P2 patch:

- `cargo test --quiet` passed
- `maturin build --release` passed
- `PYTHONPATH=/mnt/d/openproject/.pydeps python3 -m pytest -q tests` passed with `44 passed`
- Focused side-by-side verification against `parse-original` matched for:
  - `parse(..., evaluate_result=False)`
  - `search(..., evaluate_result=False)`
  - `findall(..., evaluate_result=False)`
  - `with_pattern`
  - `extra_types` overriding built-ins
  - `regex_group_count` behavior for nested custom converters

## Remaining Work

No open items remain from the original P0/P1/P2 compatibility checklist. Any next work should be either broader upstream test import or performance-focused cleanup of the Python fallback path.
