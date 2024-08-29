use std::env;
use std::io;
use std::process;

enum PatternKind {
    Alternatives(Vec<SubPattern>),
    NotAlternatives(Vec<SubPattern>),
    Literal(char),
    AlphaNumeric,
    Digit,
    InputStart,
    InputEnd,
}

#[derive(Debug)]
enum Modifier {
    ZeroOrMore,
    OneOrMore,
}

struct SubPattern {
    kind: PatternKind,
    modifier: Option<Modifier>,
}

fn parse_pattern(pattern: &str) -> Vec<SubPattern> {
    let mut subpatterns = vec![];
    let mut chars = pattern.chars().peekable();

    while let Some(c) = chars.next() {
        let mut sp = match c {
            '+' | '*' => continue, // Handled above. Skip.
            '^' => SubPattern {
                kind: PatternKind::InputStart,
                modifier: None,
            },
            '$' => SubPattern {
                kind: PatternKind::InputEnd,
                modifier: None,
            },
            '[' => {
                let mut contents = String::new();
                let mut kind = if let Some(nc) = chars.next() {
                    if nc == '^' {
                        PatternKind::NotAlternatives(vec![])
                    } else {
                        contents.push(nc);
                        PatternKind::Alternatives(vec![])
                    }
                } else {
                    unreachable!()
                };

                while let Some(c) = chars.next() {
                    if c == ']' {
                        break;
                    }
                    contents.push(c);
                }

                match kind {
                    PatternKind::Alternatives(ref mut v)
                    | PatternKind::NotAlternatives(ref mut v) => {
                        v.extend(parse_pattern(contents.as_str()).into_iter());
                    }
                    _ => unreachable!(),
                }

                SubPattern {
                    kind,
                    modifier: None,
                }
            }
            '\\' => match chars.next() {
                Some(nc) if nc == '\\' => SubPattern {
                    kind: PatternKind::Literal('\\'),
                    modifier: None,
                },
                Some(nc) if nc == 'd' => SubPattern {
                    kind: PatternKind::Digit,
                    modifier: None,
                },
                Some(nc) if nc == 'w' => SubPattern {
                    kind: PatternKind::AlphaNumeric,
                    modifier: None,
                },
                Some(_) => todo!(),
                None => todo!(),
            },
            c if c.is_alphanumeric() || c.is_ascii_whitespace() => SubPattern {
                kind: PatternKind::Literal(c),
                modifier: None,
            },
            c => panic!("Unhandled character: {c}"),
        };

        if let Some(nc) = chars.peek() {
            match nc {
                '+' => sp.modifier = Some(Modifier::OneOrMore),
                '*' => sp.modifier = Some(Modifier::ZeroOrMore),
                _ => (),
            }
        };

        subpatterns.push(sp);
    }

    subpatterns
}

fn match_subpattern_kind(remaining: &str, kind: &PatternKind) -> Option<usize> {
    match kind {
        PatternKind::InputStart => unreachable!(),
        PatternKind::InputEnd => {
            if remaining.is_empty() {
                Some(0)
            } else {
                None
            }
        }
        PatternKind::Literal(l) => {
            if remaining.starts_with(*l) {
                Some(1)
            } else {
                None
            }
        }
        PatternKind::Digit => match remaining.chars().nth(0) {
            Some(c) if c.is_digit(10) => Some(1),
            Some(_) | None => None,
        },
        PatternKind::AlphaNumeric => match remaining.chars().nth(0) {
            Some(c) if c.is_alphanumeric() || c == '_' => Some(1),
            Some(_) | None => None,
        },
        PatternKind::Alternatives(v) => {
            for alternative in v {
                if let Some(offset) = match_subpattern(remaining, alternative) {
                    return Some(offset);
                }
            }
            None
        }
        PatternKind::NotAlternatives(v) => {
            for alternative in v {
                if let Some(_) = match_subpattern(remaining, alternative) {
                    return None;
                }
            }

            Some(1)
        }
    }
}

fn match_subpattern(remaining: &str, sp: &SubPattern) -> Option<usize> {
    match sp.modifier {
        Some(Modifier::ZeroOrMore) | Some(Modifier::OneOrMore) => {
            let mut still_remaining = remaining;
            while let Some(offset) = match_subpattern_kind(still_remaining, &sp.kind) {
                still_remaining = &still_remaining[offset..];
            }

            let offset = remaining.len() - still_remaining.len();

            // We didn't match a single instance, but we must.
            if matches!(sp.modifier, Some(Modifier::OneOrMore)) && offset == 0 {
                return None;
            }

            // We may have matched or not, doesn't matter.
            Some(offset)
        }
        None => match_subpattern_kind(remaining, &sp.kind),
    }
}

fn find_match_start<'a, 'b>(input: &'a str, sp: &'b SubPattern) -> Option<(&'a str, usize)> {
    for n in 0..input.len() {
        if let Some(count) = match_subpattern(&input[n..], sp) {
            return Some((&input[n..], count));
        }
    }
    None
}

fn match_pattern(input_line: &str, pattern: &str) -> bool {
    let mut subpatterns = parse_pattern(pattern);
    if subpatterns.is_empty() {
        return true;
    }

    // Start by trying to find somewhere in the input where we can start a match.
    // Unless we have a line start pattern ^, in which case we simply drop that pattern and
    // expect matches to start at the beginning of input.
    let mut remaining = if let Some(SubPattern {
        kind: PatternKind::InputStart,
        ..
    }) = subpatterns.first()
    {
        subpatterns.remove(0);
        &input_line[0..]
    } else {
        let Some((remaining, _)) = find_match_start(&input_line[0..], subpatterns.first().unwrap())
        else {
            return false; // Short-circuit if we couldn't find a match starting point.
        };
        remaining
    };

    // Try to match from there and fail if we cannot at some point.
    for sp in &subpatterns {
        let Some(offset) = match_subpattern(remaining, sp) else {
            return false;
        };

        remaining = &remaining[offset..];
    }

    // We ran out of pattern to match, so we had a match!
    true
}

// Usage: echo <input_text> | your_program.sh -E <pattern>
fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    if env::args().nth(1).unwrap() != "-E" {
        println!("Expected first argument to be '-E'");
        process::exit(1);
    }

    let pattern = env::args().nth(2).unwrap();
    let mut input_line = String::new();

    io::stdin().read_line(&mut input_line).unwrap();

    if match_pattern(&input_line, &pattern) {
        process::exit(0)
    } else {
        process::exit(1)
    }
}
