//! A deterministic smoke fuzzer.
//!
//! The parser must answer every input with a value or an error, never a panic
//! -- no out-of-bounds index, no slice landing inside a multi-byte character,
//! no arithmetic overflow. Whatever it does accept must also survive a
//! serialize/re-parse round-trip.

use tomlproc::{parse, to_string};

/// xorshift64*, so a failure reproduces from its seed alone.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn pick<T: Copy>(&mut self, choices: &[T]) -> T {
        choices[self.below(choices.len())]
    }
}

/// Fragments chosen to sit right on the parser's decision points.
const PIECES: &[&str] = &[
    "a",
    "b.c",
    "\"q\"",
    "'l'",
    "=",
    " ",
    "\t",
    "\n",
    "\r\n",
    "\r",
    "#c",
    ".",
    ",",
    "[",
    "]",
    "[[",
    "]]",
    "{",
    "}",
    "\"",
    "'",
    "\"\"\"",
    "'''",
    "\\",
    "\\u00e9",
    "\\U0001F600",
    "\\q",
    "1",
    "0x1",
    "0b_",
    "1_0",
    "01",
    "1.",
    ".5",
    "1e",
    "+",
    "-",
    "inf",
    "nan",
    "true",
    "false",
    "1979-05-27",
    "07:32:00",
    "1979-05-27T07:32:00Z",
    "1979-05-27 07:32:00",
    "é",
    "\u{1}",
    "\u{feff}",
    "\u{7f}",
    "9223372036854775808",
    "x = ",
    "[t]",
    "[[t]]",
];

#[test]
fn never_panics_and_round_trips() {
    let mut rng = Rng(0x5EED_1979_0527_0732);
    for _ in 0..40_000 {
        let mut input = String::new();
        for _ in 0..rng.below(24) {
            input.push_str(rng.pick(PIECES));
        }
        // Any outcome is fine as long as it is an outcome.
        let Ok(table) = parse(&input) else { continue };
        let written = to_string(&table);
        let reparsed = parse(&written)
            .unwrap_or_else(|e| panic!("input {input:?} wrote {written:?} which failed: {e}"));
        // Compare the serialized forms: a document holding a NaN is never
        // equal to itself.
        assert_eq!(written, to_string(&reparsed), "input {input:?}");
    }
}

#[test]
fn never_panics_on_arbitrary_bytes() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF0);
    for _ in 0..40_000 {
        let bytes: Vec<u8> = (0..rng.below(48)).map(|_| rng.below(256) as u8).collect();
        // Only valid UTF-8 can reach the parser, which takes a `&str`.
        if let Ok(input) = str::from_utf8(&bytes) {
            let _ = parse(input);
        }
    }
}

#[test]
fn truncations_of_a_real_document_never_panic() {
    let document = include_str!("../README.md");
    for end in 0..document.len() {
        if document.is_char_boundary(end) {
            let _ = parse(&document[..end]);
        }
    }
}
