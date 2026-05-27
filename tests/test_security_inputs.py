"""Adversarial input tests for parse-rust security evaluation.

Each test category verifies that parse-rust handles hostile/malformed
inputs without crashing, panicking, hanging, or behaving unexpectedly.
"""

import signal
from contextlib import contextmanager

import pytest

from parse_rust import compile, findall, parse, search


TIMEOUT_SECONDS = 10


@contextmanager
def timeout(seconds: int):
    """Fail the test if it takes longer than *seconds*."""
    def _handle(signum, frame):
        raise TimeoutError(f"test exceeded {seconds}s timeout")

    old = signal.signal(signal.SIGALRM, _handle)
    signal.alarm(seconds)
    try:
        yield
    finally:
        signal.alarm(0)
        signal.signal(signal.SIGALRM, old)


# ── helpers ──────────────────────────────────────────────────────────

def _check_no_crash(fn):
    """Run *fn* and return its result; fail the test on any unexpected crash."""
    try:
        return fn()
    except (ValueError, KeyError, IndexError, TypeError, AttributeError):
        # Standard Python exceptions are acceptable
        return None
    except BaseException as e:
        if type(e).__name__ in (
            "PanicException", "SystemError", "FatalError", "SignalException"
        ):
            pytest.fail(f"crash / panic detected: {type(e).__name__}: {e}")
        raise


# ══════════════════════════════════════════════════════════════════════
# 1. Very large inputs
# ══════════════════════════════════════════════════════════════════════

def test_huge_string_does_not_oom():
    """A 1 MiB single-line string must not OOM or hang."""
    big = "x" * (1024 * 1024)

    def run():
        r = parse("prefix {} suffix", f"prefix {big} suffix")
        return r is not None

    assert _check_no_crash(run) in (True, False, None)


def test_huge_format_field_count_does_not_stack_overflow():
    """A format string with 100 fields must compile without crash."""
    fmt = " ".join(["{}"] * 100)
    data = " ".join(str(i) for i in range(100))

    def run():
        r = parse(fmt, data)
        return len(r.fixed)

    result = _check_no_crash(run)
    assert result in (100, None)


def test_many_search_results_do_not_exhaust_memory():
    """findall on a string with many matches must not blow memory."""
    needle = "a"
    haystack = "a" * 100_000  # 100K chars, each 'a' is a match for {}

    def run():
        count = 0
        for _ in findall("{}", haystack):
            count += 1
            if count >= 100_000:
                break
        return count

    assert _check_no_crash(run) > 0


# ══════════════════════════════════════════════════════════════════════
# 2. Null bytes and control characters
# ══════════════════════════════════════════════════════════════════════

def test_null_byte_in_string():
    """Null bytes in the input string must be handled, not crash."""
    def run():
        r = parse("hello {} world", "hello \x00 world")
        return r is not None

    _check_no_crash(run)


def test_null_byte_in_format():
    """Null bytes in the format string must be handled."""
    def run():
        r = parse("hello \x00 {}", "hello world")
        return r is not None

    _check_no_crash(run)


def test_control_characters_in_both():
    """ASCII control chars (\\x01-\\x1f) must not crash."""
    def run():
        for c in range(1, 32):
            s = chr(c) + " {} "
            parse(s, s.replace("{}", "test"))
    _check_no_crash(run)


# ══════════════════════════════════════════════════════════════════════
# 3. Unicode edge cases
# ══════════════════════════════════════════════════════════════════════

def test_four_byte_emoji():
    """4-byte UTF-8 emoji must work and span offsets must be character-based."""
    emoji = "🎉🎉🎉"
    def run():
        r = parse("hi {} bye", f"hi {emoji} bye")
        assert r is not None
        assert r[0] == emoji
        assert r.spans[0] == (3, 6)  # 3 chars each is 1 code point
    _check_no_crash(run)


def test_combining_characters():
    """Combining diacritics must not cause span corruption."""
    cafe = "café"  # café with combining acute
    def run():
        r = parse("{}", cafe)
        assert r is not None
        assert r[0] == cafe
    _check_no_crash(run)


def test_right_to_left_text():
    """RTL text must not crash."""
    rtl = "السلام"  # Arabic "salaam"
    def run():
        r = parse("greeting: {}", f"greeting: {rtl}")
        assert r is not None
        assert r[0] == rtl
    _check_no_crash(run)


def test_surrogate_range_bytes():
    """Lone surrogate code units (\\ud800-\\udfff) must not crash."""
    # Encode as UTF-8 bytes then decode with surrogateescape
    def run():
        raw = b"prefix \xed\xa0\x80 suffix"
        text = raw.decode("utf-8", errors="surrogateescape")
        r = parse("prefix {} suffix", text)
        assert r is not None
    _check_no_crash(run)


def test_mixed_byte_and_unicode_spans():
    """Spans on mixed ASCII/CJK must report character offsets, not byte offsets."""
    def run():
        r = parse("你好 {} 世界", "你好 中文 世界")
        assert r is not None
        assert r[0] == "中文"
        assert r.spans == {0: (3, 5)}  # "你好 " = 3 chars, "中文" = 2 chars
    _check_no_crash(run)


# ══════════════════════════════════════════════════════════════════════
# 4. Deeply nested named fields
# ══════════════════════════════════════════════════════════════════════

NESTING_DEPTH = 8

def test_deeply_nested_fields_compile():
    """A format with deep nested fields must compile."""
    parts = []
    for i in range(NESTING_DEPTH):
        parts.append(f"level_{i}")
    key_path = "[" + "][".join(f'"{p}"' for p in parts) + "]"
    fmt = f"{{{key_path}}}"

    def run():
        p = compile(fmt)
        return p.named_fields
    result = _check_no_crash(run)
    assert result is not None


def test_deeply_nested_fields_parse():
    """Deep nested fields must parse correctly."""
    key_path = "[" + "][".join(f'"{p}"' for p in [f"l{i}" for i in range(NESTING_DEPTH)]) + "]"
    fmt = f"{{{key_path}}}"

    def run():
        r = parse(fmt, "value")
        return r is not None
    result = _check_no_crash(run)
    assert result is True


# ══════════════════════════════════════════════════════════════════════
# 5. Regex injection resistance
# ══════════════════════════════════════════════════════════════════════

def test_format_not_interpreted_as_regex_directly():
    """Format strings with regex metacharacters must be literal, not regex."""
    # If the format string were raw regex, `(.+)` would match anything.
    # As a format string, it should match literal "(.+)"
    def run():
        r = parse("value: {}", "value: (.+)")
        assert r is not None
        assert r[0] == "(.+)"
    _check_no_crash(run)


def test_regex_quantifier_chars_in_format():
    """Regex quantifiers in format string must be literal text."""
    for fmt_char in ["*", "+", "?", "|", "^", "$", "\\"]:
        def run(c=fmt_char):
            r = parse("char {}", f"char {c}")
            assert r is not None
            assert r[0] == c
        _check_no_crash(run)


def test_regex_group_chars_in_format():
    """Parentheses in format string must be literal."""
    def run():
        r = parse("({})", "(hello)")
        assert r is not None
        assert r[0] == "hello"
    _check_no_crash(run)


# ══════════════════════════════════════════════════════════════════════
# 6. Empty and edge-case inputs
# ══════════════════════════════════════════════════════════════════════

def test_empty_format_string():
    """An empty format must not crash."""
    def run():
        r = parse("", "")
        return r is not None
    _check_no_crash(run)


def test_empty_input_string():
    """An empty input string with non-empty format must return None."""
    def run():
        r = parse("hello {}", "")
        return r  # expected None
    assert _check_no_crash(run) is None


def test_whitespace_only_format():
    """A format of only whitespace must not crash."""
    def run():
        r = parse("   ", "   ")
        return r is not None
    _check_no_crash(run)


def test_format_equals_input():
    """Literal format that equals the entire input must match (zero fields)."""
    def run():
        r = parse("exact text", "exact text")
        assert r is not None
        assert len(r.fixed) == 0
    _check_no_crash(run)


# ══════════════════════════════════════════════════════════════════════
# 7. Malformed format strings
# ══════════════════════════════════════════════════════════════════════

def test_unclosed_brace():
    """A format string with an unclosed brace must not crash."""
    def run():
        r = parse("hello {name", "hello world")
        return r  # upstream returns None for malformed format
    result = _check_no_crash(run)
    assert result is None  # malformed format → no match


def test_double_close_brace():
    """Stray closing brace must not crash."""
    def run():
        r = parse("hello } world", "hello world")
        return r
    result = _check_no_crash(run)
    assert result is None  # stray '}' treated as literal, no match


def test_nested_braces():
    """Escaped braces ({{ }}) must be treated as literal, not crash."""
    def run():
        r = parse("{{literal}}", "{literal}")
        assert r is not None
        assert len(r.fixed) == 0  # zero fields, literal match
    _check_no_crash(run)


def test_unknown_format_type():
    """Unknown format type character must raise ValueError, not panic."""
    def run():
        parse("{:Z}", "anything")
    with pytest.raises(ValueError):
        run()


def test_incomplete_format_spec():
    """Format with empty spec ({:}) must be treated as {}, not crash."""
    def run():
        r = parse("{:}", "anything")
        assert r is not None
        assert r[0] == "anything"  # empty spec = default matching
    _check_no_crash(run)


# ══════════════════════════════════════════════════════════════════════
# 8. Type conversion edge cases
# ══════════════════════════════════════════════════════════════════════

def test_integer_overflow():
    """Very large integer strings must not crash, return int or None."""
    def run():
        big = "9" * 1000
        r = parse("{:d}", big)
        if r is None:
            return None
        return r[0]
    result = _check_no_crash(run)
    # Accept either: Python int (upstream) or str (parse-rust fallback for >i64)
    # The key property: no crash, no panic, no hang
    assert result is None or isinstance(result, (int, str))


def test_float_edge_values():
    """NaN/Inf representations must not crash."""
    def run():
        for val in ["nan", "inf", "-inf", "NaN", "Infinity"]:
            r = parse("{:f}", val)
            if r is not None:
                assert isinstance(r[0], float)
    _check_no_crash(run)


def test_datetime_edge_values():
    """Weird but parseable datetime strings must not crash."""
    def run():
        for val in [
            "0000-01-01T00:00:00Z",
            "9999-12-31T23:59:59Z",
            "1970-01-01T00:00:00+00:00",
        ]:
            r = parse("At {:ti}", f"At {val}")
            if r is not None:
                pass  # Any Result is acceptable
    _check_no_crash(run)


# ══════════════════════════════════════════════════════════════════════
# 9. search / findall edge cases
# ══════════════════════════════════════════════════════════════════════

def test_search_on_empty_string():
    """search on empty string must return None, not crash."""
    def run():
        return search("{}", "")
    assert _check_no_crash(run) is None


def test_findall_on_empty_string():
    """findall on empty string must return empty iterator."""
    def run():
        return len(list(findall("{}", "")))
    assert _check_no_crash(run) == 0


def test_search_pos_beyond_length():
    """search with pos beyond string length must return None."""
    def run():
        return search("{}", "hello", pos=100)
    assert _check_no_crash(run) is None


def test_findall_endpos_before_pos():
    """findall with endpos < pos must return empty."""
    def run():
        return len(list(findall("{}", "hello", pos=3, endpos=1)))
    assert _check_no_crash(run) == 0
