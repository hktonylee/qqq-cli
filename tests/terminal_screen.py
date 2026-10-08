"""Shared visible terminal frame decoder for PTY tests."""
import codecs
import unicodedata


class TerminalScreen:
    """Track visible cells across Ratatui's partial-frame terminal updates."""

    def __init__(self, width, height):
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.pending = ""
        self.cursor_visible = True
        self.resize(width, height)

    def resize(self, width, height):
        self.width, self.height = width, height
        self.cells = [[" "] * width for _ in range(height)]
        self.x = self.y = 0

    def feed(self, data):
        text = self.pending + self.decoder.decode(data)
        self.pending = ""
        index = 0
        while index < len(text):
            char = text[index]
            if char == "\x1b":
                if index + 1 >= len(text):
                    break
                if text[index + 1] != "[":
                    index += 2
                    continue
                end = index + 2
                while end < len(text) and not ("@" <= text[end] <= "~"):
                    end += 1
                if end == len(text):
                    break
                self.csi(text[index + 2:end], text[end])
                index = end + 1
                continue
            if char == "\r":
                self.x = 0
            elif char == "\n":
                self.y = min(self.height - 1, self.y + 1)
            elif char >= " ":
                assert char.isprintable(), f"Unsupported screen character {char!r}"
                if unicodedata.combining(char):
                    previous = min(self.x - 1, self.width - 1)
                    while previous > 0 and self.cells[self.y][previous] == "":
                        previous -= 1
                    if previous >= 0:
                        self.cells[self.y][previous] += char
                else:
                    width = 2 if unicodedata.east_asian_width(char) in ("W", "F") else 1
                    if self.x + width > self.width:
                        self.x = 0
                        self.y = min(self.height - 1, self.y + 1)
                    self.cells[self.y][self.x] = char
                    if width == 2 and self.x + 1 < self.width:
                        self.cells[self.y][self.x + 1] = ""
                    self.x += width
            index += 1
        self.pending = text[index:]

    def csi(self, parameters, command):
        if parameters == "?25" and command in ("h", "l"):
            self.cursor_visible = command == "h"
            return
        values = [int(part) if part.isdigit() else 0 for part in parameters.split(";")]
        if command in ("H", "f"):
            self.y = max(0, min(self.height - 1, (values[0] or 1) - 1))
            self.x = max(0, min(self.width - 1, (values[1] if len(values) > 1 else 1) - 1))
        elif command == "J" and values[0] == 2:
            self.cells = [[" "] * self.width for _ in range(self.height)]
        elif command == "K":
            self.cells[self.y][self.x:] = [" "] * (self.width - self.x)

    def text(self):
        return "\n".join("".join(row) for row in self.cells)

