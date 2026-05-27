from datetime import date, datetime, time, timedelta, timezone
from decimal import Decimal

import pytest

import pickle

from parse_rust import RepeatedNameError, Result, compile, extract_format, findall, parse, search, with_pattern


def test_top_level_parse_named_and_positional_fields():
    result = parse("User {name:w} is {:d} years old", "User Alice is 30 years old")

    assert result is not None
    assert result[0] == 30
    assert result["name"] == "Alice"
    assert result.fixed == (30,)
    assert result.named == {"name": "Alice"}


def test_top_level_parse_returns_none_on_no_match():
    assert parse("Hello {}", "Goodbye World") is None


def test_search_and_findall_work_end_to_end():
    found = search("is {:d}", "Alice is 30 years old")
    values = list(findall("{:d}", "I have 3 cats and 5 dogs"))

    assert found is not None
    assert found.fixed == (30,)
    assert [item[0] for item in values] == [3, 5]


def test_compiled_parser_methods_and_properties():
    parser = compile("User {name:w} has {:d} points")

    parsed = parser.parse("User Alice has 12 points")
    searched = parser.search("prefix User Bob has 8 points suffix")
    found = list(parser.findall("User Carol has 3 points and User Dave has 4 points"))

    assert parsed is not None
    assert searched is not None
    assert parsed.named == {"name": "Alice"}
    assert parsed.fixed == (12,)
    assert searched.named == {"name": "Bob"}
    assert [item.named for item in found] == [{"name": "Carol"}, {"name": "Dave"}]
    assert [item.fixed for item in found] == [(3,), (4,)]
    assert parser.format == "User {name:w} has {:d} points"
    assert "name" in parser.named_fields
    assert parser.fixed_fields == [0]
    assert "User" in parser.pattern


def test_findall_returns_iterator_object():
    results = findall("{:d}", "1 2 3")

    assert iter(results) is results
    assert [item[0] for item in results] == [1, 2, 3]


def test_result_access_and_membership_behaviors():
    result = parse("{} {name} {} {city}", "hello Alice from Beijing")

    assert result is not None
    assert result[0] == "hello"
    assert result[1] == "from"
    assert result[-1] == "from"
    assert result[0:2] == ("hello", "from")
    assert result["name"] == "Alice"
    assert "name" in result
    assert "city" in result
    assert "missing" not in result

    with pytest.raises(IndexError):
        _ = result[3]

    with pytest.raises(KeyError):
        _ = result["missing"]


def test_result_spans_and_unicode_offsets_use_character_positions():
    ascii_result = parse("hello {} {name} {} {spam}", "hello world and other beings")
    unicode_result = parse("你好 {}", "你好 世界")

    assert ascii_result is not None
    assert ascii_result.spans == {0: (6, 11), "name": (12, 15), 1: (16, 21), "spam": (22, 28)}
    assert unicode_result is not None
    assert unicode_result.spans == {0: (3, 5)}


def test_match_objects_support_deferred_evaluation():
    parse_match = parse("hello {}", "hello world", evaluate_result=False)
    search_match = search("age: {:d}\n", "name: Rufus\nage: 42\n", evaluate_result=False)
    parser = compile("value={:d}")
    compiled_match = parser.parse("value=7", evaluate_result=False)
    findall_matches = list(findall(">{}<", "<p>some <b>bold</b> text</p>", evaluate_result=False))

    assert parse_match.evaluate_result().fixed == ("world",)
    assert search_match.evaluate_result().fixed == (42,)
    assert compiled_match.evaluate_result().fixed == (7,)
    assert "".join(match.evaluate_result().fixed[0] for match in findall_matches) == "some bold text"


def test_case_sensitive_flag_changes_matching_behavior():
    assert parse("Hello {name}", "hello WORLD") is not None
    assert parse("Hello {name}", "hello WORLD", case_sensitive=True) is None
    assert search("Hello {name}", "xxhello WORLDyy", case_sensitive=True) is None
    assert compile("Hello {name}", case_sensitive=True).parse("hello WORLD") is None


def test_top_level_and_compiled_range_behavior_stay_consistent():
    top_level_search = search("{}", "你好世界", 1, 3)
    compiled_search = compile("{}").search("你好世界", 1, 3)
    top_level_findall = list(findall("{:d}", "a1 b2 c3", 1, 6))
    compiled_findall = list(compile("{:d}").findall("a1 b2 c3", 1, 6))

    assert top_level_search is not None
    assert compiled_search is not None
    assert top_level_search.fixed == compiled_search.fixed == ("好",)
    assert top_level_search.spans == compiled_search.spans == {0: (1, 2)}
    assert [item.fixed for item in top_level_findall] == [item.fixed for item in compiled_findall]
    assert [item.spans for item in top_level_findall] == [item.spans for item in compiled_findall]


def test_numeric_and_decimal_types_are_converted():
    integer = parse("value={:d}", "value=12")
    hexadecimal = parse("Color: #{:x}", "Color: #FF00AA")
    percentage = parse("Progress: {:%}", "Progress: 75%")
    decimal_value = parse("total {:F}", "total 5.5")

    assert integer.fixed == (12,)
    assert hexadecimal.fixed == (0xFF00AA,)
    assert percentage.fixed == (0.75,)
    assert decimal_value.fixed == (Decimal("5.5"),)
    assert isinstance(decimal_value[0], Decimal)


def test_alignment_width_and_precision_behaviors_are_exposed_through_api():
    left = parse("{:<} world", "hello       world")
    right = parse("hello {:>}", "hello       world")
    center = parse("hello {:^} world", "hello  there     world")
    split_text = parse("{:.2}{:.2}", "look")
    padded_int = parse("a {:5d} b", "a    12 b")
    padded_float = parse("a {:8.5f} b", "a  .31415 b")

    assert left.fixed == ("hello",)
    assert right.fixed == ("world",)
    assert center.fixed == ("there",)
    assert split_text.fixed == ("lo", "ok")
    assert padded_int.fixed == (12,)
    assert padded_float.fixed == (0.31415,)


def test_nested_named_fields_expand_to_nested_dicts():
    result = parse("{hello[world]}_{hello[foo][baz]}_{simple}", "a_b_c")

    assert result is not None
    assert result.named == {"hello": {"world": "a", "foo": {"baz": "b"}}, "simple": "c"}


def test_datetime_and_custom_percent_formats_return_python_temporals():
    iso_datetime = parse("At {:ti}", "At 1972-01-20T10:21:36Z")
    email_datetime = parse("At {:te}", "At Mon, 20 Jan 1972 10:21:36 +1000")
    time_only = parse("At {:tt}", "At 10:21:36 PM -0530")
    custom_date = parse("On {:%Y-%m-%d}", "On 2023-11-25")
    custom_time = parse("At {:%H:%M:%S}", "At 13:23:27")

    assert iso_datetime[0] == datetime(1972, 1, 20, 10, 21, 36, tzinfo=timezone.utc)
    assert email_datetime[0] == datetime(1972, 1, 20, 10, 21, 36, tzinfo=timezone(timedelta(hours=10)))
    assert time_only[0] == time(22, 21, 36, tzinfo=timezone(timedelta(hours=-5, minutes=-30)))
    assert custom_date[0] == date(2023, 11, 25)
    assert custom_time[0] == time(13, 23, 27)


def test_repeated_names_support_success_and_raise_on_type_mismatch():
    matched = parse("{name:w} {name:w}", "Alice Alice")

    assert matched is not None
    assert matched.named == {"name": "Alice"}

    with pytest.raises(RepeatedNameError):
        parse("{name:w} {name:d}", "Alice 30")


def test_custom_types_work_for_top_level_and_compiled_paths():
    @with_pattern(r"[ab]")
    def ab(text):
        return {"a": 1, "b": 2}[text]

    top_level = parse("test {result:ab}", "test a", extra_types={"ab": ab})
    parser = compile("test {result:ab}", extra_types={"ab": ab})

    assert top_level.named == {"result": 1}
    assert parser.parse("test b").named == {"result": 2}
    assert parser.parse("test c") is None


def test_regex_group_count_is_supported_for_custom_types():
    @with_pattern(r"(meter|kilometer)", regex_group_count=1)
    def parse_unit(text):
        return text.strip()

    @with_pattern(r"\d+")
    def parse_number(text):
        return int(text)

    parser = compile("test {:Unit}-{:Number}", extra_types={"Unit": parse_unit, "Number": parse_number})

    assert parser.parse("test meter-10").fixed == ("meter", 10)
    assert parser.parse("test kilometer-20").fixed == ("kilometer", 20)
    assert parser.parse("test liter-30") is None


def test_invalid_regex_group_count_configuration_raises_at_parse_time():
    @with_pattern(r"(meter|kilometer)", regex_group_count=1)
    def parse_unit(text):
        return text.strip()

    @with_pattern(r"\d+")
    def parse_number(text):
        return int(text)

    for bad_group_count, error_type in ((None, ValueError), (0, ValueError), (2, IndexError)):
        parse_unit.regex_group_count = bad_group_count
        parser = compile("test {:Unit}-{:Number}", extra_types={"Unit": parse_unit, "Number": parse_number})
        with pytest.raises(error_type):
            parser.parse("test meter-10")


def test_custom_type_override_and_search_findall_conversion_work():
    doubler = lambda text: int(text) * 2

    overridden = parse("{:d}", "12", extra_types={"d": doubler})

    @with_pattern(r"\d+")
    def triple(text):
        return int(text) * 3

    @with_pattern(r"\d+")
    def increment(text):
        return int(text) + 1

    searched = search("value={:Num}", "prefix value=14 suffix", extra_types={"Num": triple})
    found = list(findall("{:Num}", "3 5 8", extra_types={"Num": increment}))

    assert overridden.fixed == (24,)
    assert searched.fixed == (42,)
    assert [item[0] for item in found] == [4, 6, 9]


def test_extract_format_parses_type_and_width_precision():
    assert extract_format("d") == {"type": "d"}
    assert extract_format("w") == {"type": "w"}
    assert extract_format("") == {}

    d = extract_format("05d")
    assert d["type"] == "d"
    assert d["width"] == "5"
    assert d["zero"] is True

    d = extract_format(".2f")
    assert d["type"] == "f"
    assert d["precision"] == "2"

    d = extract_format("^10s")
    assert d["type"] == "s"
    assert d["align"] == "^"
    assert d["width"] == "10"


def test_result_python_constructor_and_access():
    r = Result(("hello", 42), {"name": "Alice"}, {0: (0, 5), "name": (6, 11)})

    assert r.fixed == ("hello", 42)
    assert r.named == {"name": "Alice"}
    assert r.spans == {0: (0, 5), "name": (6, 11)}
    assert r[0] == "hello"
    assert r[1] == 42
    assert r["name"] == "Alice"
    assert "name" in r
    assert "missing" not in r

    with pytest.raises(IndexError):
        _ = r[2]
    with pytest.raises(KeyError):
        _ = r["missing"]


def test_parser_preserves_results_after_pickle_roundtrip():
    p = compile("User {name:w} is {:d} years old")
    r1 = p.parse("User Alice is 30 years old")

    restored = pickle.loads(pickle.dumps(p))
    r2 = restored.parse("User Bob is 42 years old")

    assert restored.format == p.format
    assert r1.named == {"name": "Alice"}
    assert r1.fixed == (30,)
    assert r2.named == {"name": "Bob"}
    assert r2.fixed == (42,)

    with pytest.raises(RepeatedNameError):
        compile("{name:w} {name:d}")
