import re
from decimal import Decimal


def with_pattern(pattern, regex_group_count=None):
    def decorator(func):
        func.pattern = pattern
        func.regex_group_count = regex_group_count
        return func

    return decorator


class RepeatedNameError(ValueError):
    pass


class convert_first:
    def __init__(self, converter):
        self.converter = converter

    def __call__(self, string, match):
        return self.converter(string)


class int_convert:
    CHARS = "0123456789abcdefghijklmnopqrstuvwxyz"

    def __init__(self, base=None):
        self.base = base

    def __call__(self, string, match):
        if string[0] == "-":
            sign = -1
            number_start = 1
        elif string[0] == "+":
            sign = 1
            number_start = 1
        else:
            sign = 1
            number_start = 0

        base = self.base
        if base is None:
            base = 10
            if string[number_start] == "0" and len(string) - number_start > 2:
                if string[number_start + 1] in "bB":
                    base = 2
                elif string[number_start + 1] in "oO":
                    base = 8
                elif string[number_start + 1] in "xX":
                    base = 16

        chars = int_convert.CHARS[:base]
        string = re.sub("[^%s]" % chars, "", string.lower())
        return sign * int(string, base)


def percentage(string, match):
    return float(string[:-1]) / 100.0


REGEX_SAFETY = re.compile(r"([?\\.[\]()*+^$!|])")
PARSE_RE = re.compile(r"({{|}}|{[\w-]*(?:\.[\w-]+|\[[^]]+])*(?::[^}]+)?})")
ALLOWED_TYPES = set(list("nbox%fFegwWdDsSl"))


def extract_format(format_spec, extra_types):
    fill = align = None
    if format_spec and format_spec[0] in "<>=^":
        align = format_spec[0]
        format_spec = format_spec[1:]
    elif len(format_spec) > 1 and format_spec[1] in "<>=^":
        fill = format_spec[0]
        align = format_spec[1]
        format_spec = format_spec[2:]

    if format_spec.startswith(("+", "-", " ")):
        format_spec = format_spec[1:]

    zero = False
    if format_spec and format_spec[0] == "0":
        zero = True
        format_spec = format_spec[1:]

    width = ""
    while format_spec and format_spec[0].isdigit():
        width += format_spec[0]
        format_spec = format_spec[1:]

    grouping = None
    if format_spec.startswith(","):
        grouping = ","
        format_spec = format_spec[1:]
    elif format_spec.startswith("_"):
        grouping = "_"
        format_spec = format_spec[1:]

    precision = ""
    if format_spec.startswith("."):
        format_spec = format_spec[1:]
        while format_spec and format_spec[0].isdigit():
            precision += format_spec[0]
            format_spec = format_spec[1:]

    type_ = format_spec
    if type_ and type_ not in ALLOWED_TYPES and type_ not in extra_types:
        raise ValueError("format spec %r not recognised" % type_)

    return {
        "fill": fill,
        "align": align,
        "zero": zero,
        "width": width,
        "grouping": grouping,
        "precision": precision,
        "type": type_,
    }


class Result:
    def __init__(self, fixed, named, spans):
        self.fixed = fixed
        self.named = named
        self.spans = spans

    def __getitem__(self, item):
        if isinstance(item, (int, slice)):
            return self.fixed[item]
        return self.named[item]

    def __contains__(self, name):
        return name in self.named


class Match:
    def __init__(self, parser, match):
        self.parser = parser
        self.match = match

    def evaluate_result(self):
        return self.parser.evaluate_result(self.match)


class ResultIterator:
    def __init__(self, parser, string, pos, endpos, evaluate_result=True):
        self.parser = parser
        self.string = string
        self.pos = pos
        self.endpos = endpos
        self.evaluate_result = evaluate_result

    def __iter__(self):
        return self

    def __next__(self):
        m = self.parser._search_re.search(self.string, self.pos, self.endpos)
        if m is None:
            raise StopIteration()
        self.pos = m.end()
        if self.evaluate_result:
            return self.parser.evaluate_result(m)
        return Match(self.parser, m)


class Parser:
    def __init__(self, format, extra_types=None, case_sensitive=False):
        self._group_to_name_map = {}
        self._name_to_group_map = {}
        self._name_types = {}
        self._format = format
        self._extra_types = extra_types or {}
        self._re_flags = re.DOTALL if case_sensitive else re.IGNORECASE | re.DOTALL
        self._fixed_fields = []
        self._named_fields = []
        self._group_index = 0
        self._type_conversions = {}
        self._expression = self._generate_expression()
        self.__search_re = None
        self.__match_re = None

    @property
    def _search_re(self):
        if self.__search_re is None:
            self.__search_re = re.compile(self._expression, self._re_flags)
        return self.__search_re

    @property
    def _match_re(self):
        if self.__match_re is None:
            self.__match_re = re.compile(r"\A%s\Z" % self._expression, self._re_flags)
        return self.__match_re

    @property
    def named_fields(self):
        return self._named_fields[:]

    @property
    def fixed_fields(self):
        return self._fixed_fields[:]

    @property
    def format(self):
        return self._format

    def parse(self, string, evaluate_result=True):
        m = self._match_re.match(string)
        if m is None:
            return None
        return self.evaluate_result(m) if evaluate_result else Match(self, m)

    def search(self, string, pos=0, endpos=None, evaluate_result=True):
        if endpos is None:
            endpos = len(string)
        m = self._search_re.search(string, pos, endpos)
        if m is None:
            return None
        return self.evaluate_result(m) if evaluate_result else Match(self, m)

    def findall(self, string, pos=0, endpos=None, evaluate_result=True):
        if endpos is None:
            endpos = len(string)
        return ResultIterator(self, string, pos, endpos, evaluate_result=evaluate_result)

    def evaluate_result(self, m):
        fixed_fields = list(m.groups())
        for n in self._fixed_fields:
            if n in self._type_conversions:
                fixed_fields[n] = self._type_conversions[n](fixed_fields[n], m)
        fixed_fields = tuple(fixed_fields[n] for n in self._fixed_fields)

        groupdict = m.groupdict()
        named_fields = {}
        name_map = {}
        for k in self._named_fields:
            korig = self._group_to_name_map[k]
            name_map[korig] = k
            if k in self._type_conversions:
                value = self._type_conversions[k](groupdict[k], m)
            else:
                value = groupdict[k]
            named_fields[korig] = value

        spans = {n: m.span(name_map[n]) for n in named_fields}
        spans.update((i, m.span(n + 1)) for i, n in enumerate(self._fixed_fields))
        return Result(fixed_fields, named_fields, spans)

    def _regex_replace(self, match):
        return "\\" + match.group(1)

    def _generate_expression(self):
        e = []
        for part in PARSE_RE.split(self._format):
            if not part:
                continue
            if part == "{{":
                e.append(r"\{")
            elif part == "}}":
                e.append(r"\}")
            elif part[0] == "{" and part[-1] == "}":
                e.append(self._handle_field(part))
            else:
                e.append(REGEX_SAFETY.sub(self._regex_replace, part))
        return "".join(e)

    def _to_group_name(self, field):
        group = field.replace(".", "_").replace("[", "_").replace("]", "_").replace("-", "_")
        n = 1
        while group in self._group_to_name_map:
            n += 1
            if "." in field:
                group = field.replace(".", "_" * n)
            elif "_" in field:
                group = field.replace("_", "_" * n)
            elif "-" in field:
                group = field.replace("-", "_" * n)
            else:
                raise KeyError("duplicated group name %r" % (field,))
        self._group_to_name_map[group] = field
        self._name_to_group_map[field] = group
        return group

    def _handle_field(self, field):
        field = field[1:-1]
        if ":" in field:
            name, format_spec = field.split(":", 1)
        else:
            name, format_spec = field, ""

        if name and name[0].isalpha():
            if name in self._name_to_group_map:
                if self._name_types[name] != format_spec:
                    raise RepeatedNameError(
                        'field type %r for field "%s" does not match previous seen type %r'
                        % (format_spec, name, self._name_types[name])
                    )
                return r"(?P=%s)" % self._name_to_group_map[name]
            group = self._to_group_name(name)
            self._name_types[name] = format_spec
            self._named_fields.append(group)
            wrap = r"(?P<%s>%%s)" % group
        else:
            self._fixed_fields.append(self._group_index)
            wrap = r"(%s)"
            group = self._group_index

        if not format_spec:
            self._group_index += 1
            return wrap % r".+?"

        format_info = extract_format(format_spec, self._extra_types)
        type_ = format_info["type"]
        is_numeric = type_ and type_ in "n%fegdobx"
        conv = self._type_conversions

        if type_ in self._extra_types:
            type_converter = self._extra_types[type_]
            s = getattr(type_converter, "pattern", r".+?")
            regex_group_count = getattr(type_converter, "regex_group_count", 0)
            if regex_group_count is None:
                regex_group_count = 0
            self._group_index += regex_group_count
            conv[group] = convert_first(type_converter)
        elif type_ == "n":
            s = r"\d{1,3}([,._]\d{3})*"
            self._group_index += 1
            conv[group] = int_convert(10)
        elif type_ == "b":
            s = r"(0[bB])?[01]+"
            self._group_index += 1
            conv[group] = int_convert(2)
        elif type_ == "o":
            s = r"(0[oO])?[0-7]+"
            self._group_index += 1
            conv[group] = int_convert(8)
        elif type_ == "x":
            s = r"(0[xX])?[0-9a-fA-F]+"
            self._group_index += 1
            conv[group] = int_convert(16)
        elif type_ == "%":
            s = r"\d+(\.\d+)?%"
            self._group_index += 1
            conv[group] = percentage
        elif type_ == "f":
            s = r"\d*\.\d+"
            conv[group] = convert_first(float)
        elif type_ == "F":
            s = r"\d*\.\d+"
            conv[group] = convert_first(Decimal)
        elif type_ == "e":
            s = r"\d*\.\d+[eE][-+]?\d+|nan|NAN|[-+]?inf|[-+]?INF"
            conv[group] = convert_first(float)
        elif type_ == "g":
            s = r"\d+(\.\d+)?([eE][-+]?\d+)?|nan|NAN|[-+]?inf|[-+]?INF"
            self._group_index += 2
            conv[group] = convert_first(float)
        elif type_ == "d":
            width = r"{1,%s}" % int(format_info["width"]) if format_info.get("width") else "+"
            grouping = format_info.get("grouping") or ""
            s = r"[0-9{g}]{w}|0[xX][0-9a-fA-F{g}]{w}|0[bB][01{g}]{w}|0[oO][0-7{g}]{w}".format(
                g=grouping, w=width
            )
            conv[group] = int_convert()
        elif type_ == "l":
            s = r"[A-Za-z]+"
        elif type_:
            s = r"\%s+" % type_
        elif format_info.get("precision"):
            if format_info.get("width"):
                s = r".{%s,%s}?" % (format_info["width"], format_info["precision"])
            else:
                s = r".{1,%s}?" % format_info["precision"]
        elif format_info.get("width"):
            s = r".{%s,}?" % format_info["width"]
        else:
            s = r".+?"

        align = format_info["align"]
        fill = format_info["fill"]
        if is_numeric:
            if align == "=":
                if not fill:
                    fill = "0"
                s = r"%s*" % re.escape(fill) + s
            s = r"[-+ ]?" + s
        if not fill:
            fill = " "
        s = wrap % s
        self._group_index += 1
        if format_info["width"] and not align:
            align = ">"
        fill = re.escape(fill)
        if align == "<":
            s = "%s%s*" % (s, fill)
        elif align == ">":
            s = "%s*%s" % (fill, s)
        elif align == "^":
            s = "%s*%s%s*" % (fill, s, fill)
        return s


def parse(format, string, extra_types=None, evaluate_result=True, case_sensitive=False):
    return Parser(format, extra_types=extra_types, case_sensitive=case_sensitive).parse(
        string, evaluate_result=evaluate_result
    )


def search(format, string, pos=0, endpos=None, extra_types=None, evaluate_result=True, case_sensitive=False):
    return Parser(format, extra_types=extra_types, case_sensitive=case_sensitive).search(
        string, pos, endpos, evaluate_result=evaluate_result
    )


def findall(format, string, pos=0, endpos=None, extra_types=None, evaluate_result=True, case_sensitive=False):
    return Parser(format, extra_types=extra_types, case_sensitive=case_sensitive).findall(
        string, pos, endpos, evaluate_result=evaluate_result
    )


def compile(format, extra_types=None, case_sensitive=False):
    return Parser(format, extra_types=extra_types, case_sensitive=case_sensitive)
