//! The TOML 1.0.0 parser.
//!
//! A single pass over the source bytes builds the document directly; there is
//! no separate token stream. Positions are tracked as a byte offset plus the
//! start of the current line, so an error can report a line and column without
//! any bookkeeping on the happy path.

use std::collections::HashSet;

use crate::datetime;
use crate::error::Error;
use crate::map::Table;
use crate::span::{Span, Spans};
use crate::value::Value;

/// How deeply arrays and inline tables may nest.
///
/// Parsing is recursive, so without a cap a document such as `a = [[[[…`
/// would exhaust the stack. No real document comes close to this.
const MAX_DEPTH: usize = 128;

/// One step of a path into the document: a table key, or an index into an
/// array of tables.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Seg {
    Key(String),
    Index(usize),
}

type Path = Vec<Seg>;

/// What sits at a path, from the point of view of the rules that decide whether
/// a definition is legal.
enum Slot {
    Missing,
    Table,
    Array(usize),
    Other,
}

pub(crate) struct Parser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    line_start: usize,
    /// How many arrays and inline tables enclose the cursor.
    depth: usize,
    root: Table,
    /// The table that bare key/value pairs are currently written into.
    current: Path,
    /// Tables brought into being as an intermediate step of a `[a.b.c]`
    /// header. These may still be defined explicitly later on.
    implicit: HashSet<Path>,
    /// Tables brought into being by a dotted key. A header may pass through
    /// one, but may not target it.
    dotted: HashSet<Path>,
    /// Tables that came from inline table syntax, and everything nested inside
    /// them. These are sealed: nothing may be added to them.
    frozen: HashSet<Path>,
    /// Arrays declared with `[[header]]`, the only ones a later `[[header]]`
    /// may append to.
    arrays: HashSet<Path>,
    /// Where each value was written, recorded only when asked for.
    spans: Option<Spans>,
}

impl<'a> Parser<'a> {
    /// Builds a parser. `spans` asks it to record where each value was
    /// written, which costs a little time and memory, so it is off unless the
    /// caller wants it.
    pub(crate) fn new(src: &'a str, spans: bool) -> Parser<'a> {
        Parser {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            line_start: 0,
            depth: 0,
            root: Table::new(),
            current: Path::new(),
            implicit: HashSet::new(),
            dotted: HashSet::new(),
            frozen: HashSet::new(),
            arrays: HashSet::new(),
            spans: spans.then(Spans::default),
        }
    }

    pub(crate) fn parse(mut self) -> Result<(Table, Option<Spans>), Error> {
        // A leading byte order mark is not part of the document.
        if self.src.starts_with('\u{feff}') {
            self.pos += '\u{feff}'.len_utf8();
        }
        loop {
            self.skip_trivia()?;
            if self.pos >= self.bytes.len() {
                return Ok((self.root, self.spans));
            }
            if self.peek() == Some(b'[') {
                self.parse_header()?;
            } else {
                self.parse_keyval()?;
            }
            self.finish_line()?;
        }
    }

    // ----- positions and errors -------------------------------------------

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    /// Records that the byte before `self.pos` ended a line.
    fn note_newline(&mut self) {
        self.line += 1;
        self.line_start = self.pos;
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T, Error> {
        Err(self.error_at(self.pos, message))
    }

    fn error_at(&self, pos: usize, message: impl Into<String>) -> Error {
        let (line, column) = self.position(pos);
        Error::parse(message, line, column, pos.min(self.src.len()))
    }

    /// The 1-based line and character column of a byte offset.
    fn position(&self, pos: usize) -> (usize, usize) {
        let pos = pos.min(self.src.len());
        // Columns are counted in characters. Errors are rare enough that
        // rescanning the line (or, for a position behind the cursor, the whole
        // input) is cheaper than tracking columns as we go.
        let (line, line_start) = if pos >= self.line_start {
            (self.line, self.line_start)
        } else {
            let mut line = 1;
            let mut line_start = 0;
            for (i, c) in self.src[..pos].char_indices() {
                if c == '\n' {
                    line += 1;
                    line_start = i + 1;
                }
            }
            (line, line_start)
        };
        (line, self.src[line_start..pos].chars().count() + 1)
    }

    // ----- whitespace, comments, line structure ---------------------------

    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
    }

    /// Consumes a newline, if there is one, reporting a stray carriage return.
    fn consume_newline(&mut self) -> Result<bool, Error> {
        match self.peek() {
            Some(b'\n') => {
                self.pos += 1;
                self.note_newline();
                Ok(true)
            }
            Some(b'\r') if self.peek_at(1) == Some(b'\n') => {
                self.pos += 2;
                self.note_newline();
                Ok(true)
            }
            Some(b'\r') => self.error("a carriage return must be followed by a line feed"),
            _ => Ok(false),
        }
    }

    fn skip_comment(&mut self) -> Result<(), Error> {
        self.pos += 1; // the `#`
        while let Some(c) = self.peek() {
            match c {
                b'\n' => break,
                b'\r' if self.peek_at(1) == Some(b'\n') => break,
                c if is_control(c) => return self.error("invalid control character in comment"),
                _ => self.pos += 1,
            }
        }
        Ok(())
    }

    /// Skips whitespace, newlines and comments.
    fn skip_trivia(&mut self) -> Result<(), Error> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t') => self.pos += 1,
                Some(b'#') => self.skip_comment()?,
                Some(b'\n' | b'\r') => {
                    self.consume_newline()?;
                }
                _ => return Ok(()),
            }
        }
    }

    /// After a header or a key/value pair, only a comment may follow before the
    /// end of the line.
    fn finish_line(&mut self) -> Result<(), Error> {
        self.skip_spaces();
        if self.peek() == Some(b'#') {
            self.skip_comment()?;
        }
        if self.pos >= self.bytes.len() || self.consume_newline()? {
            Ok(())
        } else {
            self.error("expected a newline after the value")
        }
    }

    // ----- keys -----------------------------------------------------------

    /// Parses a possibly dotted key, such as `a.b."c d"`.
    fn parse_key(&mut self) -> Result<Vec<String>, Error> {
        let mut keys = vec![self.parse_simple_key()?];
        loop {
            self.skip_spaces();
            if self.peek() != Some(b'.') {
                return Ok(keys);
            }
            self.pos += 1;
            self.skip_spaces();
            keys.push(self.parse_simple_key()?);
        }
    }

    fn parse_simple_key(&mut self) -> Result<String, Error> {
        match self.peek() {
            Some(b'"') => self.parse_quoted_string(false),
            Some(b'\'') => self.parse_quoted_string(true),
            Some(c) if is_bare_key_char(c) => {
                let start = self.pos;
                while matches!(self.peek(), Some(c) if is_bare_key_char(c)) {
                    self.pos += 1;
                }
                Ok(self.src[start..self.pos].to_owned())
            }
            _ => self.error("expected a key"),
        }
    }

    // ----- statements -----------------------------------------------------

    /// Parses `[table]` or `[[array of tables]]`.
    fn parse_header(&mut self) -> Result<(), Error> {
        let start = self.pos;
        self.pos += 1;
        let is_array = self.peek() == Some(b'[');
        if is_array {
            self.pos += 1;
        }
        self.skip_spaces();
        let keys = self.parse_key()?;
        self.skip_spaces();
        for _ in 0..1 + usize::from(is_array) {
            if self.peek() != Some(b']') {
                return self.error(if is_array {
                    "expected `]]`"
                } else {
                    "expected `]`"
                });
            }
            self.pos += 1;
        }
        self.define_table(keys, is_array, start)
    }

    /// Parses `key = value`.
    fn parse_keyval(&mut self) -> Result<(), Error> {
        let start = self.pos;
        let keys = self.parse_key()?;
        self.skip_spaces();
        if self.peek() != Some(b'=') {
            return self.error("expected `=` after the key");
        }
        self.pos += 1;
        self.skip_spaces();
        let value_start = self.pos;
        let value = self.parse_value()?;
        self.insert_keyval(keys, value, start, value_start..self.pos)
    }

    // ----- values ---------------------------------------------------------

    fn parse_value(&mut self) -> Result<Value, Error> {
        match self.peek() {
            Some(b'"') | Some(b'\'') => self.parse_string_value(),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_inline_table(),
            Some(b't') | Some(b'f') => self.parse_bool(),
            Some(c) if c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'i' | b'n') => {
                self.parse_number()
            }
            Some(_) => self.error("expected a value"),
            None => self.error("expected a value, found the end of the document"),
        }
    }

    fn parse_bool(&mut self) -> Result<Value, Error> {
        for (word, value) in [("true", true), ("false", false)] {
            if self.src[self.pos..].starts_with(word) {
                self.pos += word.len();
                self.expect_value_end()?;
                return Ok(Value::Boolean(value));
            }
        }
        self.error("expected a value")
    }

    fn parse_number(&mut self) -> Result<Value, Error> {
        let start = self.pos;
        match datetime::scan(&self.bytes[start..]) {
            Ok(Some((dt, used))) => {
                self.pos += used;
                self.expect_value_end()?;
                return Ok(Value::Datetime(dt));
            }
            Ok(None) => {}
            Err(message) => return Err(self.error_at(start, message)),
        }
        let mut end = start;
        while end < self.bytes.len() && !is_value_end(self.bytes[end]) {
            end += 1;
        }
        self.pos = end;
        parse_number_token(&self.src[start..end]).map_err(|message| self.error_at(start, message))
    }

    /// Checks that a bare value (number, boolean, date-time) is followed by
    /// something that can legitimately end it.
    fn expect_value_end(&self) -> Result<(), Error> {
        match self.peek() {
            None => Ok(()),
            Some(c) if is_value_end(c) => Ok(()),
            Some(_) => Err(self.error_at(self.pos, "unexpected character after the value")),
        }
    }

    /// Enters a nested array or inline table, refusing to recurse past
    /// [`MAX_DEPTH`]. Every caller pairs this with `self.depth -= 1`.
    fn enter(&mut self) -> Result<(), Error> {
        if self.depth == MAX_DEPTH {
            return self.error(format!(
                "arrays and inline tables cannot nest more than {MAX_DEPTH} deep"
            ));
        }
        self.depth += 1;
        Ok(())
    }

    fn parse_array(&mut self) -> Result<Value, Error> {
        self.enter()?;
        let array = self.parse_array_items();
        self.depth -= 1;
        array
    }

    fn parse_array_items(&mut self) -> Result<Value, Error> {
        let open = self.pos;
        self.pos += 1;
        let mut array = Vec::new();
        loop {
            // Newlines and comments may appear anywhere inside an array.
            self.skip_trivia()?;
            match self.peek() {
                None => return Err(self.error_at(open, "unterminated array")),
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(array));
                }
                _ => {}
            }
            array.push(self.parse_value()?);
            self.skip_trivia()?;
            match self.peek() {
                // A trailing comma is allowed, so the next turn of the loop
                // may well find the closing bracket.
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(array));
                }
                None => return Err(self.error_at(open, "unterminated array")),
                Some(_) => return self.error("expected `,` or `]` in the array"),
            }
        }
    }

    fn parse_inline_table(&mut self) -> Result<Value, Error> {
        self.enter()?;
        let table = self.parse_inline_table_entries();
        self.depth -= 1;
        table
    }

    fn parse_inline_table_entries(&mut self) -> Result<Value, Error> {
        let open = self.pos;
        self.pos += 1;
        let mut table = Table::new();
        // Tables created by a dotted key *within this inline table*, which the
        // pairs that follow may keep extending.
        let mut dotted = HashSet::new();
        loop {
            // Newlines, comments and a trailing comma have been allowed inside
            // an inline table since TOML 1.1.
            self.skip_trivia()?;
            if self.peek() == Some(b'}') {
                self.pos += 1;
                return Ok(Value::Table(table));
            }
            if self.pos >= self.bytes.len() {
                return Err(self.error_at(open, "unterminated inline table"));
            }
            let key_pos = self.pos;
            let keys = self.parse_key()?;
            self.skip_spaces();
            if self.peek() != Some(b'=') {
                return self.error("expected `=` after the key");
            }
            self.pos += 1;
            self.skip_spaces();
            let value = self.parse_value()?;
            insert_dotted(&mut table, &keys, value, &mut dotted)
                .map_err(|message| self.error_at(key_pos, message))?;
            self.skip_trivia()?;
            match self.peek() {
                // The next turn of the loop closes the table if that comma was
                // a trailing one.
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Table(table));
                }
                None => return Err(self.error_at(open, "unterminated inline table")),
                _ => return self.error("expected `,` or `}` in the inline table"),
            }
        }
    }

    // ----- strings --------------------------------------------------------

    fn parse_string_value(&mut self) -> Result<Value, Error> {
        let literal = self.peek() == Some(b'\'');
        let quote = if literal { b'\'' } else { b'"' };
        if self.peek_at(1) == Some(quote) && self.peek_at(2) == Some(quote) {
            self.parse_multiline_string(literal).map(Value::String)
        } else {
            self.parse_quoted_string(literal).map(Value::String)
        }
    }

    /// Parses a single-line basic (`"…"`) or literal (`'…'`) string.
    fn parse_quoted_string(&mut self, literal: bool) -> Result<String, Error> {
        let quote = if literal { b'\'' } else { b'"' };
        let open = self.pos;
        self.pos += 1;
        let mut out = String::new();
        let mut run = self.pos;
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => return Err(self.error_at(open, "unterminated string")),
            };
            match c {
                c if c == quote => {
                    out.push_str(&self.src[run..self.pos]);
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' if !literal => {
                    out.push_str(&self.src[run..self.pos]);
                    self.pos += 1;
                    self.parse_escape(&mut out)?;
                    run = self.pos;
                }
                b'\n' | b'\r' => {
                    return self.error("a single-line string cannot contain a newline");
                }
                c if is_control(c) => return self.error("invalid control character in string"),
                _ => self.pos += 1,
            }
        }
    }

    /// Parses a multi-line basic (`"""…"""`) or literal (`'''…'''`) string.
    fn parse_multiline_string(&mut self, literal: bool) -> Result<String, Error> {
        let quote = if literal { b'\'' } else { b'"' };
        let open = self.pos;
        self.pos += 3;
        // A newline immediately after the opening delimiter is trimmed.
        self.consume_newline()?;
        let mut out = String::new();
        let mut run = self.pos;
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => return Err(self.error_at(open, "unterminated string")),
            };
            match c {
                c if c == quote => {
                    let mut quotes = 0;
                    while self.peek_at(quotes) == Some(quote) {
                        quotes += 1;
                    }
                    if quotes < 3 {
                        // One or two quotes are ordinary content.
                        self.pos += quotes;
                        continue;
                    }
                    if quotes > 5 {
                        return self.error("too many quotes at the end of a multi-line string");
                    }
                    out.push_str(&self.src[run..self.pos]);
                    // Of a run of 4 or 5, all but the last three belong to the
                    // string.
                    for _ in 0..quotes - 3 {
                        out.push(char::from(quote));
                    }
                    self.pos += quotes;
                    return Ok(out);
                }
                b'\\' if !literal => {
                    out.push_str(&self.src[run..self.pos]);
                    if self.at_line_ending_backslash() {
                        // A backslash at the end of a line swallows that
                        // newline and all the whitespace that follows.
                        self.pos += 1;
                        loop {
                            self.skip_spaces();
                            if !self.consume_newline()? {
                                break;
                            }
                        }
                    } else {
                        self.pos += 1;
                        self.parse_escape(&mut out)?;
                    }
                    run = self.pos;
                }
                b'\n' => {
                    self.pos += 1;
                    self.note_newline();
                }
                b'\r' if self.peek_at(1) == Some(b'\n') => {
                    // Newlines are normalized to a line feed.
                    out.push_str(&self.src[run..self.pos]);
                    out.push('\n');
                    self.pos += 2;
                    self.note_newline();
                    run = self.pos;
                }
                c if is_control(c) => return self.error("invalid control character in string"),
                _ => self.pos += 1,
            }
        }
    }

    /// Whether the backslash at the cursor is followed only by whitespace up to
    /// the end of the line.
    fn at_line_ending_backslash(&self) -> bool {
        let mut i = self.pos + 1;
        while matches!(self.bytes.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        match self.bytes.get(i) {
            Some(b'\n') => true,
            Some(b'\r') => self.bytes.get(i + 1) == Some(&b'\n'),
            _ => false,
        }
    }

    /// Parses the body of an escape sequence; the backslash is already
    /// consumed.
    fn parse_escape(&mut self, out: &mut String) -> Result<(), Error> {
        let start = self.pos;
        let c = match self.peek() {
            Some(c) => c,
            None => return self.error("unterminated escape sequence"),
        };
        self.pos += 1;
        let c = match c {
            b'b' => '\u{8}',
            b't' => '\t',
            b'n' => '\n',
            b'f' => '\u{c}',
            b'r' => '\r',
            b'e' => '\u{1b}',
            b'"' => '"',
            b'\\' => '\\',
            // `\xHH` reaches only the first 256 codepoints, which is exactly
            // what two hex digits can say.
            b'x' => return self.parse_escape_codepoint(2, start, out),
            b'u' => return self.parse_escape_codepoint(4, start, out),
            b'U' => return self.parse_escape_codepoint(8, start, out),
            _ => return Err(self.error_at(start, "invalid escape sequence")),
        };
        out.push(c);
        Ok(())
    }

    fn parse_escape_codepoint(
        &mut self,
        digits: usize,
        start: usize,
        out: &mut String,
    ) -> Result<(), Error> {
        let end = self.pos + digits;
        if end > self.bytes.len() || !self.bytes[self.pos..end].iter().all(u8::is_ascii_hexdigit) {
            return Err(self.error_at(
                start,
                format!("expected {digits} hexadecimal digits in the escape sequence"),
            ));
        }
        let code = u32::from_str_radix(&self.src[self.pos..end], 16).expect("hex digits");
        self.pos = end;
        match char::from_u32(code) {
            Some(c) => {
                out.push(c);
                Ok(())
            }
            None => Err(self.error_at(start, format!("`{code:X}` is not a Unicode scalar value"))),
        }
    }

    // ----- building the document ------------------------------------------

    fn slot(&self, path: &Path) -> Slot {
        match lookup(&self.root, path) {
            None => Slot::Missing,
            Some(Value::Table(_)) => Slot::Table,
            Some(Value::Array(array)) => Slot::Array(array.len()),
            Some(_) => Slot::Other,
        }
    }

    /// Applies a `[table]` or `[[array]]` header, and makes it current.
    fn define_table(
        &mut self,
        keys: Vec<String>,
        is_array: bool,
        start: usize,
    ) -> Result<(), Error> {
        let count = keys.len();
        let mut path = Path::new();
        for (i, key) in keys.into_iter().enumerate() {
            path.push(Seg::Key(key));
            if self.frozen.contains(&path) {
                return Err(self.error_at(
                    start,
                    format!("cannot add to `{}`: it is an inline table", render(&path)),
                ));
            }
            let last = i + 1 == count;
            if !last {
                match self.slot(&path) {
                    Slot::Missing => {
                        insert_at(&mut self.root, &path, Value::Table(Table::new()));
                        self.implicit.insert(path.clone());
                    }
                    Slot::Table => {}
                    // A header may extend the most recent element of an array
                    // of tables, and nothing else.
                    Slot::Array(len) if self.arrays.contains(&path) => {
                        path.push(Seg::Index(len - 1))
                    }
                    _ => {
                        return Err(
                            self.error_at(start, format!("`{}` is not a table", render(&path)))
                        );
                    }
                }
            } else if is_array {
                match self.slot(&path) {
                    Slot::Missing => {
                        insert_at(
                            &mut self.root,
                            &path,
                            Value::Array(vec![Value::Table(Table::new())]),
                        );
                        self.arrays.insert(path.clone());
                        path.push(Seg::Index(0));
                    }
                    Slot::Array(len) if self.arrays.contains(&path) => {
                        if let Some(Value::Array(array)) = lookup_mut(&mut self.root, &path) {
                            array.push(Value::Table(Table::new()));
                        }
                        path.push(Seg::Index(len));
                    }
                    _ => {
                        return Err(self.error_at(
                            start,
                            format!(
                                "`{}` is already defined and is not an array of tables",
                                render(&path)
                            ),
                        ));
                    }
                }
            } else {
                match self.slot(&path) {
                    Slot::Missing => insert_at(&mut self.root, &path, Value::Table(Table::new())),
                    // Only a table conjured up by an earlier header path may be
                    // claimed by a header of its own.
                    Slot::Table if self.implicit.remove(&path) => {}
                    _ => {
                        return Err(
                            self.error_at(start, format!("`{}` is already defined", render(&path)))
                        );
                    }
                }
            }
        }
        self.record_span(&path, start..self.pos, start..self.pos);
        self.current = path;
        Ok(())
    }

    /// Stores a key/value pair in the current table.
    fn insert_keyval(
        &mut self,
        keys: Vec<String>,
        value: Value,
        start: usize,
        value_range: core::ops::Range<usize>,
    ) -> Result<(), Error> {
        let mut path = self.current.clone();
        let count = keys.len();
        for (i, key) in keys.into_iter().enumerate() {
            path.push(Seg::Key(key));
            if i + 1 == count {
                break;
            }
            if self.frozen.contains(&path) {
                return Err(self.error_at(
                    start,
                    format!("cannot add to `{}`: it is an inline table", render(&path)),
                ));
            }
            match self.slot(&path) {
                Slot::Missing => {
                    insert_at(&mut self.root, &path, Value::Table(Table::new()));
                    self.dotted.insert(path.clone());
                }
                Slot::Table if self.dotted.contains(&path) => {}
                // A table that a header only brought into being on its way
                // somewhere deeper -- the `a.b` of `[a.b.c]` -- has been
                // created but not *defined*, so a dotted key may still write
                // into it. Doing so is what defines it, which is why no header
                // may claim it afterwards.
                Slot::Table if self.implicit.remove(&path) => {
                    self.dotted.insert(path.clone());
                }
                // Anything else is a table already defined some other way, and
                // a dotted key may not redefine it.
                _ => {
                    return Err(self.error_at(
                        start,
                        format!("cannot add to `{}` with a dotted key", render(&path)),
                    ));
                }
            }
        }
        if lookup(&self.root, &path).is_some() {
            return Err(self.error_at(start, format!("duplicate key `{}`", render(&path))));
        }
        if value.is_table() {
            self.freeze(&path, &value);
        }
        insert_at(&mut self.root, &path, value);
        self.record_span(&path, start..value_range.end, value_range);
        Ok(())
    }

    /// Notes where a value was written, if spans were asked for.
    fn record_span(
        &mut self,
        path: &Path,
        range: core::ops::Range<usize>,
        value: core::ops::Range<usize>,
    ) {
        if self.spans.is_none() {
            return;
        }
        let (line, column) = self.position(range.start);
        let key = render_raw(path);
        self.spans.as_mut().expect("checked above").record(
            key,
            Span {
                range,
                value,
                line,
                column,
            },
        );
    }

    /// Marks an inline table, and every table nested in it, as sealed.
    fn freeze(&mut self, path: &Path, value: &Value) {
        if let Value::Table(table) = value {
            self.frozen.insert(path.clone());
            for (key, child) in table {
                let mut child_path = path.clone();
                child_path.push(Seg::Key(key.to_owned()));
                self.freeze(&child_path, child);
            }
        }
    }
}

// ----- helpers over the document tree -------------------------------------

fn lookup<'t>(root: &'t Table, path: &[Seg]) -> Option<&'t Value> {
    let (first, rest) = path.split_first()?;
    let mut current = match first {
        Seg::Key(key) => root.get(key)?,
        Seg::Index(_) => return None,
    };
    for seg in rest {
        current = match (current, seg) {
            (Value::Table(table), Seg::Key(key)) => table.get(key)?,
            (Value::Array(array), Seg::Index(i)) => array.get(*i)?,
            _ => return None,
        };
    }
    Some(current)
}

fn lookup_mut<'t>(root: &'t mut Table, path: &[Seg]) -> Option<&'t mut Value> {
    let (first, rest) = path.split_first()?;
    let mut current = match first {
        Seg::Key(key) => root.get_mut(key)?,
        Seg::Index(_) => return None,
    };
    for seg in rest {
        current = match (current, seg) {
            (Value::Table(table), Seg::Key(key)) => table.get_mut(key)?,
            (Value::Array(array), Seg::Index(i)) => array.get_mut(*i)?,
            _ => return None,
        };
    }
    Some(current)
}

/// Inserts a value at `path`, whose parent must already exist.
fn insert_at(root: &mut Table, path: &[Seg], value: Value) {
    let (parent, last) = path.split_at(path.len() - 1);
    let key = match &last[0] {
        Seg::Key(key) => key,
        Seg::Index(_) => unreachable!("array elements are appended, not inserted by path"),
    };
    let table = if parent.is_empty() {
        Some(root)
    } else {
        match lookup_mut(root, parent) {
            Some(Value::Table(table)) => Some(table),
            _ => None,
        }
    };
    table
        .expect("the parent of an inserted path always exists")
        .insert(key.clone(), value);
}

/// Renders a path as plain dotted segments, the way an error from the serde
/// integration spells one, so the two can be matched up.
fn render_raw(path: &[Seg]) -> String {
    let mut out = String::new();
    for seg in path {
        if !out.is_empty() {
            out.push('.');
        }
        match seg {
            Seg::Key(key) => out.push_str(key),
            Seg::Index(i) => out.push_str(&i.to_string()),
        }
    }
    out
}

/// Renders a path the way it would be written in a document, for error
/// messages.
fn render(path: &[Seg]) -> String {
    let mut out = String::new();
    for seg in path {
        match seg {
            Seg::Key(key) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(&crate::ser::key_to_string(key));
            }
            Seg::Index(i) => out.push_str(&format!("[{i}]")),
        }
    }
    out
}

/// Inserts a dotted key into a standalone table, used for inline tables.
///
/// `dotted` collects the tables this created, which later keys in the same
/// inline table may extend.
fn insert_dotted(
    table: &mut Table,
    keys: &[String],
    value: Value,
    dotted: &mut HashSet<Vec<String>>,
) -> Result<(), String> {
    let (last, parents) = keys.split_last().expect("a key has at least one part");
    let mut current = table;
    let mut prefix = Vec::new();
    for key in parents {
        prefix.push(key.clone());
        if !current.contains_key(key) {
            current.insert(key.clone(), Value::Table(Table::new()));
            dotted.insert(prefix.clone());
        } else if !dotted.contains(&prefix) {
            return Err(format!(
                "cannot add to `{}` with a dotted key",
                prefix.join(".")
            ));
        }
        current = match current.get_mut(key) {
            Some(Value::Table(table)) => table,
            _ => return Err(format!("`{}` is not a table", prefix.join("."))),
        };
    }
    if current.contains_key(last) {
        return Err(format!("duplicate key `{}`", keys.join(".")));
    }
    current.insert(last.clone(), value);
    Ok(())
}

// ----- numbers -------------------------------------------------------------

fn parse_number_token(token: &str) -> Result<Value, String> {
    let bytes = token.as_bytes();
    let (signed, negative) = match bytes.first() {
        Some(b'+') => (true, false),
        Some(b'-') => (true, true),
        _ => (false, false),
    };
    let rest = &token[usize::from(signed)..];
    match rest {
        "inf" => {
            return Ok(Value::Float(if negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }));
        }
        "nan" => return Ok(Value::Float(if negative { -f64::NAN } else { f64::NAN })),
        _ => {}
    }
    if let Some(radix) = rest.as_bytes().get(1).and_then(|c| match c {
        b'x' => Some(16),
        b'o' => Some(8),
        b'b' => Some(2),
        _ => None,
    }) && rest.starts_with('0')
    {
        if signed {
            return Err("a sign is not allowed on a hexadecimal, octal or binary integer".into());
        }
        let digits = strip_underscores(&rest[2..], radix)?;
        return i64::from_str_radix(&digits, radix)
            .map(Value::Integer)
            .map_err(|_| format!("`{token}` is out of the range of a 64-bit integer"));
    }

    let bytes = rest.as_bytes();
    let mut out = String::with_capacity(token.len());
    if negative {
        out.push('-');
    }
    let mut i = scan_digits(bytes, 0, &mut out)?;
    if i > 1 && bytes[0] == b'0' {
        return Err(format!("leading zeros are not allowed in `{token}`"));
    }
    let mut is_float = false;
    if bytes.get(i) == Some(&b'.') {
        is_float = true;
        out.push('.');
        i = scan_digits(bytes, i + 1, &mut out)?;
    }
    if let Some(c @ (b'e' | b'E')) = bytes.get(i) {
        is_float = true;
        out.push(char::from(*c));
        i += 1;
        if let Some(sign @ (b'+' | b'-')) = bytes.get(i) {
            out.push(char::from(*sign));
            i += 1;
        }
        i = scan_digits(bytes, i, &mut out)?;
    }
    if i != bytes.len() {
        return Err(format!("`{token}` is not a valid number"));
    }
    if is_float {
        let float: f64 = out
            .parse()
            .map_err(|_| format!("`{token}` is not a valid float"))?;
        // A literal that overflows to infinity is a mistake, not a way to
        // write `inf`; the spelled-out `inf` is handled above.
        if float.is_infinite() {
            return Err(format!("`{token}` is out of the range of a 64-bit float"));
        }
        Ok(Value::Float(float))
    } else {
        out.parse()
            .map(Value::Integer)
            .map_err(|_| format!("`{token}` is out of the range of a 64-bit integer"))
    }
}

/// Copies a run of digits, which may be separated by underscores, into `out`,
/// returning where it ended.
fn scan_digits(bytes: &[u8], start: usize, out: &mut String) -> Result<usize, String> {
    let mut i = start;
    while let Some(c) = bytes.get(i) {
        match c {
            c if c.is_ascii_digit() => {
                out.push(char::from(*c));
                i += 1;
            }
            b'_' => {
                let surrounded = i > start && bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
                if !surrounded {
                    return Err("an underscore must sit between two digits".into());
                }
                i += 1;
            }
            _ => break,
        }
    }
    if i == start {
        return Err("expected a digit".into());
    }
    Ok(i)
}

fn strip_underscores(digits: &str, radix: u32) -> Result<String, String> {
    let mut out = String::with_capacity(digits.len());
    let bytes = digits.as_bytes();
    for (i, c) in bytes.iter().enumerate() {
        if *c == b'_' {
            let previous = i > 0 && char::from(bytes[i - 1]).is_digit(radix);
            let next = bytes
                .get(i + 1)
                .is_some_and(|c| char::from(*c).is_digit(radix));
            if !previous || !next {
                return Err("an underscore must sit between two digits".into());
            }
        } else if !char::from(*c).is_digit(radix) {
            return Err(format!(
                "`{}` is not a valid digit in base {radix}",
                char::from(*c)
            ));
        } else {
            out.push(char::from(*c));
        }
    }
    if out.is_empty() {
        return Err("expected a digit".into());
    }
    Ok(out)
}

// ----- character classes ---------------------------------------------------

fn is_bare_key_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

/// Control characters that TOML forbids in strings and comments. Tab is
/// allowed; the delete character is not.
fn is_control(c: u8) -> bool {
    (c < 0x20 && c != b'\t') || c == 0x7f
}

fn is_value_end(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b',' | b']' | b'}' | b'#')
}
