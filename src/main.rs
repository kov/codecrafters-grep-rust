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

struct SubPattern {
    kind: PatternKind,
}

fn parse_pattern(pattern: &str) -> Vec<SubPattern> {
    let mut subpatterns = vec![];
    let mut chars = pattern.chars();

    while let Some(c) = chars.next() {
        match c {
            '^' => subpatterns.push(SubPattern {
                kind: PatternKind::InputStart,
            }),
            '$' => subpatterns.push(SubPattern {
                kind: PatternKind::InputEnd,
            }),
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

                subpatterns.push(SubPattern { kind });
            }
            '\\' => match chars.next() {
                Some(nc) if nc == '\\' => subpatterns.push(SubPattern {
                    kind: PatternKind::Literal('\\'),
                }),
                Some(nc) if nc == 'd' => subpatterns.push(SubPattern {
                    kind: PatternKind::Digit,
                }),
                Some(nc) if nc == 'w' => subpatterns.push(SubPattern {
                    kind: PatternKind::AlphaNumeric,
                }),
                Some(_) => todo!(),
                None => todo!(),
            },
            c if c.is_alphanumeric() || c.is_ascii_whitespace() => subpatterns.push(SubPattern {
                kind: PatternKind::Literal(c),
            }),
            _ => todo!(),
        }
    }

    subpatterns
}

fn match_subpattern(remaining: &str, sp: &SubPattern) -> Option<usize> {
    match sp {
        SubPattern {
            kind: PatternKind::InputStart,
            ..
        } => unreachable!(),
        SubPattern {
            kind: PatternKind::InputEnd,
            ..
        } => {
            if remaining.is_empty() {
                Some(0)
            } else {
                None
            }
        }
        SubPattern {
            kind: PatternKind::Literal(l),
            ..
        } => {
            if remaining.starts_with(*l) {
                Some(1)
            } else {
                None
            }
        }
        SubPattern {
            kind: PatternKind::Digit,
            ..
        } => match remaining.chars().nth(0) {
            Some(c) if c.is_digit(10) => Some(1),
            Some(_) | None => None,
        },
        SubPattern {
            kind: PatternKind::AlphaNumeric,
            ..
        } => match remaining.chars().nth(0) {
            Some(c) if c.is_alphanumeric() || c == '_' => Some(1),
            Some(_) | None => None,
        },
        SubPattern {
            kind: PatternKind::Alternatives(v),
            ..
        } => {
            for alternative in v {
                if let Some(offset) = match_subpattern(remaining, alternative) {
                    return Some(offset);
                }
            }
            None
        }
        SubPattern {
            kind: PatternKind::NotAlternatives(v),
            ..
        } => {
            for alternative in v {
                if let Some(_) = match_subpattern(remaining, alternative) {
                    return None;
                }
            }

            Some(1)
        }
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
    let mut remaining = if let Some(
        SubPattern {
            kind: PatternKind::InputStart,
        },
        ..,
    ) = subpatterns.first()
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
