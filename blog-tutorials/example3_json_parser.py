#!/usr/bin/env python3
"""
Tutorial 3: Debugging a Recursive JSON Parser

This program implements a simple recursive descent JSON parser.
It has a bug in handling escaped characters in strings.
"""

class ParseError(Exception):
    def __init__(self, message, position):
        self.message = message
        self.position = position
        super().__init__(f"{message} at position {position}")


class JSONParser:
    def __init__(self, text):
        self.text = text
        self.pos = 0

    def parse(self):
        """Parse the JSON text and return the Python object."""
        self.skip_whitespace()
        result = self.parse_value()
        self.skip_whitespace()
        if self.pos < len(self.text):
            raise ParseError(f"Unexpected character: {self.text[self.pos]}", self.pos)
        return result

    def parse_value(self):
        """Parse any JSON value."""
        self.skip_whitespace()
        if self.pos >= len(self.text):
            raise ParseError("Unexpected end of input", self.pos)

        char = self.text[self.pos]
        if char == '"':
            return self.parse_string()
        elif char == '{':
            return self.parse_object()
        elif char == '[':
            return self.parse_array()
        elif char == 't':
            return self.parse_literal("true", True)
        elif char == 'f':
            return self.parse_literal("false", False)
        elif char == 'n':
            return self.parse_literal("null", None)
        elif char == '-' or char.isdigit():
            return self.parse_number()
        else:
            raise ParseError(f"Unexpected character: {char}", self.pos)

    def parse_string(self):
        """Parse a JSON string.

        BUG: Doesn't properly handle escaped quotes inside strings!
        """
        if self.text[self.pos] != '"':
            raise ParseError("Expected '\"'", self.pos)
        self.pos += 1  # Skip opening quote

        result = []
        while self.pos < len(self.text):
            char = self.text[self.pos]

            if char == '"':
                self.pos += 1  # Skip closing quote
                return ''.join(result)

            # Bug: We detect backslash but handle it incorrectly
            if char == '\\':
                self.pos += 1
                if self.pos >= len(self.text):
                    raise ParseError("Unexpected end of escape sequence", self.pos)

                escape_char = self.text[self.pos]
                # Bug: Missing case for escaped quote!
                if escape_char == 'n':
                    result.append('\n')
                elif escape_char == 't':
                    result.append('\t')
                elif escape_char == 'r':
                    result.append('\r')
                elif escape_char == '\\':
                    result.append('\\')
                # BUG: Missing: elif escape_char == '"': result.append('"')
                else:
                    # Bug: We skip unknown escapes but should handle \"
                    result.append(escape_char)  # This adds the raw character
            else:
                result.append(char)

            self.pos += 1

        raise ParseError("Unterminated string", self.pos)

    def parse_object(self):
        """Parse a JSON object."""
        if self.text[self.pos] != '{':
            raise ParseError("Expected '{'", self.pos)
        self.pos += 1

        result = {}
        self.skip_whitespace()

        if self.pos < len(self.text) and self.text[self.pos] == '}':
            self.pos += 1
            return result

        while True:
            self.skip_whitespace()
            key = self.parse_string()
            self.skip_whitespace()

            if self.pos >= len(self.text) or self.text[self.pos] != ':':
                raise ParseError("Expected ':'", self.pos)
            self.pos += 1

            value = self.parse_value()
            result[key] = value

            self.skip_whitespace()
            if self.pos >= len(self.text):
                raise ParseError("Unexpected end of input", self.pos)

            if self.text[self.pos] == '}':
                self.pos += 1
                return result
            elif self.text[self.pos] == ',':
                self.pos += 1
            else:
                raise ParseError("Expected ',' or '}'", self.pos)

    def parse_array(self):
        """Parse a JSON array."""
        if self.text[self.pos] != '[':
            raise ParseError("Expected '['", self.pos)
        self.pos += 1

        result = []
        self.skip_whitespace()

        if self.pos < len(self.text) and self.text[self.pos] == ']':
            self.pos += 1
            return result

        while True:
            value = self.parse_value()
            result.append(value)

            self.skip_whitespace()
            if self.pos >= len(self.text):
                raise ParseError("Unexpected end of input", self.pos)

            if self.text[self.pos] == ']':
                self.pos += 1
                return result
            elif self.text[self.pos] == ',':
                self.pos += 1
            else:
                raise ParseError("Expected ',' or ']'", self.pos)

    def parse_number(self):
        """Parse a JSON number."""
        start = self.pos

        if self.text[self.pos] == '-':
            self.pos += 1

        if self.pos >= len(self.text) or not self.text[self.pos].isdigit():
            raise ParseError("Expected digit", self.pos)

        while self.pos < len(self.text) and self.text[self.pos].isdigit():
            self.pos += 1

        if self.pos < len(self.text) and self.text[self.pos] == '.':
            self.pos += 1
            while self.pos < len(self.text) and self.text[self.pos].isdigit():
                self.pos += 1

        num_str = self.text[start:self.pos]
        return float(num_str) if '.' in num_str else int(num_str)

    def parse_literal(self, literal, value):
        """Parse a literal like true, false, null."""
        if self.text[self.pos:self.pos + len(literal)] == literal:
            self.pos += len(literal)
            return value
        raise ParseError(f"Expected '{literal}'", self.pos)

    def skip_whitespace(self):
        """Skip whitespace characters."""
        while self.pos < len(self.text) and self.text[self.pos] in ' \t\n\r':
            self.pos += 1


def test_json_parser():
    """Test the JSON parser with various inputs."""
    print("Testing JSON Parser")
    print("=" * 50)

    test_cases = [
        ('{"name": "Alice", "age": 30}', True),
        ('[1, 2, 3]', True),
        ('"Hello, World!"', True),
        ('{"nested": {"key": "value"}}', True),
        # This test case triggers the bug!
        ('{"quote": "He said \\"hello\\""}', True),
    ]

    for json_text, should_parse in test_cases:
        print(f"\nInput: {json_text}")
        try:
            parser = JSONParser(json_text)
            result = parser.parse()
            print(f"Parsed: {result}")

            # Check for the bug
            if 'quote' in str(result):
                expected = 'He said "hello"'
                if isinstance(result, dict) and result.get('quote') != expected:
                    print(f"BUG: Expected: {expected}")
                    print(f"BUG: Got: {result.get('quote')}")

        except ParseError as e:
            if should_parse:
                print(f"ERROR: {e}")
            else:
                print(f"Expected error: {e}")


if __name__ == "__main__":
    test_json_parser()
