"""
Integration tests for parse_rust Python bindings.

Ensures API compatibility with the original Python parse library.
"""


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
