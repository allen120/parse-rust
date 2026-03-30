"""
Integration tests for parse_rust Python bindings.

Ensures API compatibility with the original Python parse library.
"""

import pytest


def test_basic_anonymous():
    """Test basic anonymous (positional) field parsing."""
    from parse_rust import parse

    r = parse("Hello {}!", "Hello World!")
    assert r is not None
    assert r[0] == "World"


def test_named_fields():
    """Test named field parsing."""
    from parse_rust import parse

    r = parse("User {name} from {city}", "User Alice from Beijing")
    assert r is not None
    assert r["name"] == "Alice"
    assert r["city"] == "Beijing"


def test_integer_type():
    """Test :d integer type specifier."""
    from parse_rust import parse

    r = parse("{name:w} is {:d} years old", "Alice is 30 years old")
    assert r is not None
    assert r["name"] == "Alice"
    assert r[0] == 30
    assert isinstance(r[0], int)


def test_float_type():
    """Test :f float type specifier."""
    from parse_rust import parse

    r = parse("Price: ${:f}", "Price: $19.99")
    assert r is not None
    assert abs(r[0] - 19.99) < 1e-10
    assert isinstance(r[0], float)


def test_negative_float_type():
    from parse_rust import parse

    r = parse("{:f}", "-19.99")
    assert r is not None
    assert abs(r[0] + 19.99) < 1e-10


def test_hex_type():
    """Test :x hexadecimal type specifier."""
    from parse_rust import parse

    r = parse("Color: #{:x}", "Color: #FF00AA")
    assert r is not None
    assert r[0] == 0xFF00AA


def test_percentage_type():
    """Test :% percentage type specifier."""
    from parse_rust import parse

    r = parse("Progress: {:%}", "Progress: 75%")
    assert r is not None
    assert abs(r[0] - 0.75) < 1e-10


def test_word_type():
    """Test :w word type specifier."""
    from parse_rust import parse

    r = parse("{:w}", "hello_world")
    assert r is not None
    assert r[0] == "hello_world"


def test_letters_type():
    """Test :l letters type specifier."""
    from parse_rust import parse

    r = parse("{:l}", "Hello")
    assert r is not None
    assert r[0] == "Hello"


def test_no_match():
    """Test that non-matching strings return None."""
    from parse_rust import parse

    r = parse("Hello {}", "Goodbye World")
    assert r is None


def test_case_insensitive():
    """Test case-insensitive matching (default)."""
    from parse_rust import parse

    r = parse("Hello {name}", "hello WORLD")
    assert r is not None
    assert r["name"] == "WORLD"


def test_case_sensitive():
    """Test case-sensitive matching."""
    from parse_rust import parse

    r = parse("Hello {name}", "hello World", case_sensitive=True)
    assert r is None


def test_search():
    """Test search function."""
    from parse_rust import search

    r = search("is {:d}", "Alice is 30 years old")
    assert r is not None
    assert r[0] == 30


def test_findall():
    """Test findall function."""
    from parse_rust import findall

    results = findall("{:d}", "I have 3 cats and 5 dogs")
    assert len(results) == 2
    assert results[0][0] == 3
    assert results[1][0] == 5


def test_compile():
    """Test compiled parser."""
    from parse_rust import compile

    p = compile("User {name:w}")
    r1 = p.parse("User Alice")
    r2 = p.parse("User Bob")
    assert r1 is not None
    assert r2 is not None
    assert r1["name"] == "Alice"
    assert r2["name"] == "Bob"


def test_compiled_search():
    """Test compiled parser search method."""
    from parse_rust import compile

    p = compile("is {:d}")
    r = p.search("Alice is 30 years old")
    assert r is not None
    assert r[0] == 30


def test_compiled_findall():
    """Test compiled parser findall method."""
    from parse_rust import compile

    p = compile("{:d}")
    results = p.findall("I have 3 cats and 5 dogs")
    assert len(results) == 2


def test_result_fixed_property():
    """Test Result.fixed property."""
    from parse_rust import parse

    r = parse("{} and {}", "hello and world")
    assert r is not None
    fixed = r.fixed
    assert len(fixed) == 2
    assert fixed[0] == "hello"
    assert fixed[1] == "world"


def test_result_named_property():
    """Test Result.named property."""
    from parse_rust import parse

    r = parse("{name} from {city}", "Alice from Beijing")
    assert r is not None
    named = r.named
    assert named["name"] == "Alice"
    assert named["city"] == "Beijing"


def test_result_contains():
    """Test 'in' operator on Result."""
    from parse_rust import parse

    r = parse("{name} from {city}", "Alice from Beijing")
    assert r is not None
    assert "name" in r
    assert "city" in r
    assert "age" not in r


def test_multiple_types():
    """Test parsing with multiple different types."""
    from parse_rust import parse

    r = parse(
        "{name:w} scored {score:f} with {:d} attempts",
        "Alice scored 9.5 with 3 attempts",
    )
    assert r is not None
    assert r["name"] == "Alice"
    assert abs(r["score"] - 9.5) < 1e-10
    assert r[0] == 3


def test_version():
    """Test that version is accessible."""
    import parse_rust

    assert hasattr(parse_rust, "__version__")
    assert parse_rust.__version__ == "0.1.0"


def test_parser_properties():
    """Test Parser object properties."""
    from parse_rust import compile

    p = compile("User {name:w} is {:d}")
    assert "name" in p.named_fields
    assert len(p.fixed_fields) == 1
    assert "User" in p.format


def test_repeated_named_field_matches():
    from parse_rust import parse

    r = parse("{name:w} {name:w}", "Alice Alice")
    assert r is not None
    assert r["name"] == "Alice"


def test_repeated_named_field_type_mismatch_raises():
    import pytest
    from parse_rust import parse, RepeatedNameError

    with pytest.raises(RepeatedNameError):
        parse("{name:w} {name:d}", "Alice 30")


def test_datetime_iso_returns_datetime():
    from datetime import datetime, timezone
    from parse_rust import parse

    r = parse("At {:ti}", "At 1972-01-20T10:21:36Z")
    assert r is not None
    assert r[0] == datetime(1972, 1, 20, 10, 21, 36, tzinfo=timezone.utc)


def test_datetime_email_returns_datetime():
    from datetime import datetime, timezone, timedelta
    from parse_rust import parse

    r = parse("At {:te}", "At Mon, 20 Jan 1972 10:21:36 +1000")
    assert r is not None
    assert r[0] == datetime(1972, 1, 20, 10, 21, 36, tzinfo=timezone(timedelta(hours=10)))


def test_time_only_returns_time():
    from datetime import time, timezone, timedelta
    from parse_rust import parse

    r = parse("At {:tt}", "At 10:21:36 PM -0530")
    assert r is not None
    assert r[0] == time(22, 21, 36, tzinfo=timezone(timedelta(hours=-5, minutes=-30)))


def test_custom_datetime_format_returns_date():
    from datetime import date
    from parse_rust import parse

    r = parse("On {:%Y-%m-%d}", "On 2023-11-25")
    assert r is not None
    assert r[0] == date(2023, 11, 25)


def test_left_alignment_trims_padding():
    from parse_rust import parse

    r = parse("{:<} world", "hello       world")
    assert r is not None
    assert r.fixed == ("hello",)


def test_right_alignment_trims_padding():
    from parse_rust import parse

    r = parse("hello {:>}", "hello       world")
    assert r is not None
    assert r.fixed == ("world",)


def test_center_alignment_trims_padding():
    from parse_rust import parse

    r = parse("hello {:^} world", "hello  there     world")
    assert r is not None
    assert r.fixed == ("there",)


def test_precision_and_width_for_strings():
    from parse_rust import parse

    assert parse("{:.2}{:.2}", "look").fixed == ("lo", "ok")
    assert parse("{:2}{:2}", "look").fixed == ("lo", "ok")
    assert parse("{:4}{:.4}", "look at that").fixed == ("look at ", "that")


def test_width_for_numeric_types():
    from parse_rust import parse

    assert parse("a {:5d} b", "a    12 b")[0] == 12
    assert parse("a {:8.5f} b", "a  .31415 b")[0] == 0.31415
    assert parse("{:02d}{:02d}", "0440").fixed == (4, 40)


def test_spans_include_fixed_and_named_keys():
    from parse_rust import parse

    string = "hello world and other beings"
    r = parse("hello {} {name} {} {spam}", string)
    assert r is not None
    assert r.spans == {0: (6, 11), "name": (12, 15), 1: (16, 21), "spam": (22, 28)}


def test_nested_named_fields_expand_to_dicts():
    from parse_rust import parse

    r = parse("{hello[world]}_{hello[foo][baz]}_{simple}", "a_b_c")
    assert r is not None
    assert r.named["hello"]["world"] == "a"
    assert r.named["hello"]["foo"]["baz"] == "b"
    assert r.named["simple"] == "c"


def test_parse_evaluate_result_false_returns_match():
    from parse_rust import parse

    match = parse("hello {}", "hello world", evaluate_result=False)
    assert match is not None
    assert match.evaluate_result().fixed == ("world",)


def test_search_evaluate_result_false_returns_match():
    from parse_rust import search

    match = search(
        "age: {:d}\n",
        "name: Rufus\nage: 42\ncolor: red\n",
        evaluate_result=False,
    )
    assert match is not None
    assert match.evaluate_result().fixed == (42,)


def test_findall_evaluate_result_false_returns_match_iterable():
    from parse_rust import findall

    matches = findall(">{}<", "<p>some <b>bold</b> text</p>", evaluate_result=False)
    assert "".join(m.evaluate_result().fixed[0] for m in matches) == "some bold text"


def test_compiled_parser_evaluate_result_false_returns_match():
    from parse_rust import compile

    parser = compile("hello {}")
    match = parser.parse("hello world", evaluate_result=False)
    assert match is not None
    assert match.evaluate_result().fixed == ("world",)


def test_with_pattern_custom_type():
    from parse_rust import compile, with_pattern

    @with_pattern(r"[ab]")
    def ab(text):
        return {"a": 1, "b": 2}[text]

    parser = compile("test {result:ab}", extra_types={"ab": ab})
    assert parser.parse("test a")["result"] == 1
    assert parser.parse("test b")["result"] == 2
    assert parser.parse("test c") is None


def test_with_pattern_regex_group_count_for_unnamed_followed_by_field():
    from parse_rust import compile, with_pattern

    @with_pattern(r"(meter|kilometer)", regex_group_count=1)
    def parse_unit(text):
        return text.strip()

    @with_pattern(r"\d+")
    def parse_number(text):
        return int(text)

    parser = compile(
        "test {:Unit}-{:Number}",
        extra_types={"Unit": parse_unit, "Number": parse_number},
    )
    assert parser.parse("test meter-10").fixed == ("meter", 10)
    assert parser.parse("test kilometer-20").fixed == ("kilometer", 20)
    assert parser.parse("test liter-30") is None


def test_with_pattern_bad_regex_group_count_raises():
    from parse_rust import compile, with_pattern

    @with_pattern(r"(meter|kilometer)", regex_group_count=1)
    def parse_unit(text):
        return text.strip()

    @with_pattern(r"\d+")
    def parse_number(text):
        return int(text)

    for bad_group_count, error_type in ((None, ValueError), (0, ValueError), (2, IndexError)):
        parse_unit.regex_group_count = bad_group_count
        parser = compile(
            "test {:Unit}-{:Number}",
            extra_types={"Unit": parse_unit, "Number": parse_number},
        )
        with pytest.raises(error_type):
            parser.parse("test meter-10")


def test_extra_types_override_builtin_and_compile_path():
    from parse_rust import compile, parse

    doubler = lambda text: int(text) * 2

    assert parse("{:d}", "12", extra_types={"d": doubler}).fixed == (24,)
    parser = compile("{:d}", extra_types={"d": doubler})
    assert parser.parse("12").fixed == (24,)


if __name__ == "__main__":
    import sys

    # Run all test functions
    test_funcs = [v for k, v in globals().items() if k.startswith("test_")]
    passed = 0
    failed = 0
    for func in test_funcs:
        try:
            func()
            print(f"  PASS: {func.__name__}")
            passed += 1
        except Exception as e:
            print(f"  FAIL: {func.__name__}: {e}")
            failed += 1

    print(f"\n{passed} passed, {failed} failed out of {passed + failed} tests")
    sys.exit(1 if failed else 0)
