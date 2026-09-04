//! Differential testing: `tomlproc` against the `toml` crate.
//!
//! For each input, both parsers run and three things are compared: whether
//! they accept it, and -- when they both do -- the values they produce,
//! canonicalized so that key order and map type cannot matter.
//!
//! Run it with corpus directories to walk, and it will parse every `.toml`
//! file underneath them as well:
//!
//! ```text
//! cargo run --release -- ~/.cargo/registry/src
//! ```
//!
//! Any output beyond the summary line is a disagreement worth explaining.

use std::fmt::Write as _;
use std::path::Path;

/// Renders a value so that two equal documents render identically, whatever
/// map type or key order the parser used.
macro_rules! canonicalize {
    ($name:ident, $value:path) => {
        fn $name(value: &$value, out: &mut String) {
            use $value as V;
            match value {
                V::String(s) => write!(out, "s{s:?}").unwrap(),
                V::Integer(i) => write!(out, "i{i}").unwrap(),
                V::Float(f) => write!(out, "f{f:?}").unwrap(),
                V::Boolean(b) => write!(out, "b{b}").unwrap(),
                V::Datetime(d) => write!(out, "d{d}").unwrap(),
                V::Array(items) => {
                    out.push('[');
                    for item in items {
                        $name(item, out);
                        out.push(',');
                    }
                    out.push(']');
                }
                V::Table(table) => {
                    // Sorted, so the two crates' different map types cannot
                    // show up as a disagreement.
                    let mut entries: Vec<(String, String)> = table
                        .iter()
                        .map(|(key, value)| {
                            let mut rendered = String::new();
                            $name(value, &mut rendered);
                            (key.to_string(), rendered)
                        })
                        .collect();
                    entries.sort();
                    out.push('{');
                    for (key, rendered) in entries {
                        write!(out, "{key:?}={rendered},").unwrap();
                    }
                    out.push('}');
                }
            }
        }
    };
}

canonicalize!(canonical_mine, tomlproc::Value);
canonicalize!(canonical_reference, toml::Value);

#[derive(Default)]
struct Stats {
    agree: u64,
    different_values: u64,
    only_mine_accepts: u64,
    only_reference_accepts: u64,
    reported: u32,
}

impl Stats {
    fn compare(&mut self, input: &str, label: &str) {
        match (tomlproc::parse(input), input.parse::<toml::Table>()) {
            (Ok(mine), Ok(reference)) => {
                let (mut a, mut b) = (String::new(), String::new());
                canonical_mine(&tomlproc::Value::Table(mine), &mut a);
                canonical_reference(&toml::Value::Table(reference), &mut b);
                if a == b {
                    self.agree += 1;
                } else {
                    self.different_values += 1;
                    self.report(label, input, &format!("mine:      {a}\n  reference: {b}"));
                }
            }
            (Err(_), Err(_)) => self.agree += 1,
            (Ok(_), Err(error)) => {
                self.only_mine_accepts += 1;
                self.report(label, input, &format!("the reference rejects it: {error}"));
            }
            (Err(error), Ok(_)) => {
                self.only_reference_accepts += 1;
                self.report(label, input, &format!("tomlproc rejects it: {error}"));
            }
        }
    }

    fn report(&mut self, label: &str, input: &str, detail: &str) {
        self.reported += 1;
        if self.reported <= 20 {
            println!("--- {label}\n  input: {input:?}\n  {detail}");
        }
    }
}

/// xorshift64*, so a disagreement reproduces from its seed alone.
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

    fn pick(&mut self, choices: &[&'static str]) -> &'static str {
        choices[self.below(choices.len())]
    }
}

/// Fragments that land on the parser's decision points.
const PIECES: &[&str] = &[
    "a", "b.c", "\"q\"", "'l'", "=", " ", "\t", "\n", "\r\n", "#c", ".", ",", "[", "]", "[[", "]]",
    "{", "}", "\"", "'", "\"\"\"", "'''", "\\", "\\u00e9", "\\q", "1", "0x1", "0b1", "1_0", "01",
    "1.", ".5", "1e", "1e5", "1e501", "+", "-", "inf", "nan", "true", "false", "1979-05-27",
    "07:32:00", "1979-05-27T07:32:00Z", "1979-05-27 07:32:00", "é", "9223372036854775808",
    "x = ", "[t]", "[[t]]", "a=1", "a=[", "{a=1}", "\"\"", "''", "0", "00:00:00.9999999999",
    "-0", "+0",
];

/// Whole statements, which is where the table-definition rules live: which
/// table may be reopened, extended with a dotted key, or claimed by a header.
const STATEMENTS: &[&str] = &[
    "[a]", "[a.b]", "[a.b.c]", "[[a]]", "[[a.b]]", "[b]", "[\"a\"]", "[a.\"b\"]", "a = 1",
    "a.b = 1", "a.b.c = 1", "b = 2", "b.c = 3", "a = {}", "a = { b = 1 }", "a = { b.c = 1 }",
    "a = []", "a = [{}]", "a = [1]", "c = 1", "a.b = {}", "", "# comment", "[a.b.d]", "d = 1",
    "[[a.b.c]]", "a = { b = { c = 1 } }",
];

const ROUNDS: usize = 200_000;

fn main() {
    let mut stats = Stats::default();

    let mut files = 0;
    for directory in std::env::args().skip(1) {
        walk(Path::new(&directory), &mut files, &mut stats);
    }

    let mut rng = Rng(0xA11C_E5EE_D000_0001);
    for _ in 0..ROUNDS {
        let mut input = String::new();
        for _ in 0..rng.below(14) {
            input.push_str(rng.pick(PIECES));
        }
        stats.compare(&input, "fragments");
    }

    let mut rng = Rng(0xDEC0_DE12_3400_0001);
    for _ in 0..ROUNDS {
        let mut input = String::new();
        for _ in 0..rng.below(7) {
            input.push_str(rng.pick(STATEMENTS));
            input.push('\n');
        }
        stats.compare(&input, "statements");
    }

    println!(
        "{} files + {} generated inputs\nagree: {}  different values: {}  only tomlproc accepts: {}  only the reference accepts: {}",
        files,
        2 * ROUNDS,
        stats.agree,
        stats.different_values,
        stats.only_mine_accepts,
        stats.only_reference_accepts,
    );
    if stats.reported > 0 {
        std::process::exit(1);
    }
}

fn walk(path: &Path, files: &mut u64, stats: &mut Stats) {
    let Ok(entries) = std::fs::read_dir(path) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files, stats);
        } else if path.extension().is_some_and(|e| e == "toml")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            *files += 1;
            stats.compare(&text, &path.display().to_string());
        }
    }
}
