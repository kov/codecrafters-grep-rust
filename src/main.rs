use std::env;
use std::io;
use std::process;
use std::sync::RwLock;

#[derive(Debug)]
enum PatternKind {
    Alternatives(Vec<SubPattern>),
    NotAlternatives(Vec<SubPattern>),
    Literal(char),
    AlphaNumeric,
    Digit,
    InputStart,
    InputEnd,
    Any,
    AlternateGroups(Vec<String>),
    BackRef(usize),
}

#[derive(Debug)]
enum Modifier {
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
}

#[derive(Debug)]
struct SubPattern {
    kind: PatternKind,
    modifier: Option<Modifier>,
}

static BACKREFS: RwLock<Vec<String>> = RwLock::new(vec![]);

fn parse_pattern(pattern: &str) -> Vec<SubPattern> {
    let mut subpatterns = vec![];
    let mut chars = pattern.chars().peekable();

    while let Some(c) = chars.next() {
        let mut sp = match c {
            '+' | '*' | '?' => continue, // Handled on the previous iteration. Skip.
            '.' => SubPattern {
                kind: PatternKind::Any,
                modifier: None,
            },
            '^' => SubPattern {
                kind: PatternKind::InputStart,
                modifier: None,
            },
            '$' => SubPattern {
                kind: PatternKind::InputEnd,
                modifier: None,
            },
            '(' => {
                let mut groups = vec![];
                let mut contents = String::new();

                while let Some(c) = chars.next() {
                    if c == ')' {
                        break;
                    } else if c == '|' {
                        groups.push(std::mem::take(&mut contents));
                        continue;
                    }
                    contents.push(c);
                }
                groups.push(contents);

                SubPattern {
                    kind: PatternKind::AlternateGroups(groups),
                    modifier: None,
                }
            }
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
                Some(nc) if nc.is_digit(10) => {
                    let mut tmp = [0u8; 4];
                    SubPattern {
                        kind: PatternKind::BackRef(
                            nc.encode_utf8(&mut tmp).parse::<usize>().unwrap(),
                        ),
                        modifier: None,
                    }
                }
                Some(_) => todo!(),
                None => todo!(),
            },
            c => SubPattern {
                kind: PatternKind::Literal(c),
                modifier: None,
            },
        };

        if let Some(nc) = chars.peek() {
            match nc {
                '+' => sp.modifier = Some(Modifier::OneOrMore),
                '*' => sp.modifier = Some(Modifier::ZeroOrMore),
                '?' => sp.modifier = Some(Modifier::ZeroOrOne),
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
        PatternKind::Any => {
            if !remaining.is_empty() {
                Some(1)
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
        PatternKind::AlternateGroups(groups) => {
            for g in groups {
                if let Some((start, end)) = match_pattern(remaining, g) {
                    if start == 0 {
                        BACKREFS.write().unwrap().push(g.clone());
                        return Some(end);
                    }
                }
            }

            None
        }
        PatternKind::BackRef(i) => {
            let g = BACKREFS.read().unwrap().get(*i).map(|g| g.clone());
            if let Some(g) = g {
                match_subpattern_kind(remaining, &PatternKind::AlternateGroups(vec![g.clone()]))
            } else {
                None
            }
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
        Some(Modifier::ZeroOrOne) => {
            if let Some(offset) = match_subpattern_kind(remaining, &sp.kind) {
                Some(offset)
            } else {
                Some(0)
            }
        }
        None => match_subpattern_kind(remaining, &sp.kind),
    }
}

fn find_match_start<'a, 'b>(input: &'a str, sp: &'b SubPattern) -> Option<(&'a str, usize)> {
    for n in 0..input.len() {
        if let Some(_) = match_subpattern(&input[n..], sp) {
            return Some((&input[n..], n));
        }
    }
    None
}

fn match_pattern(input_line: &str, pattern: &str) -> Option<(usize, usize)> {
    let mut subpatterns = parse_pattern(pattern);
    if subpatterns.is_empty() {
        return Some((0, 0));
    }

    // Start by trying to find somewhere in the input where we can start a match.
    // Unless we have a line start pattern ^, in which case we simply drop that pattern and
    // expect matches to start at the beginning of input.
    let (mut remaining, match_start) = if let Some(SubPattern {
        kind: PatternKind::InputStart,
        ..
    }) = subpatterns.first()
    {
        subpatterns.remove(0);
        (&input_line[0..], 0)
    } else {
        let Some((remaining, match_start)) =
            find_match_start(&input_line[0..], subpatterns.first().unwrap())
        else {
            return None; // Short-circuit if we couldn't find a match starting point.
        };
        (remaining, match_start)
    };

    // Try to match from there and fail if we cannot at some point.
    for sp in &subpatterns {
        let Some(offset) = match_subpattern(remaining, sp) else {
            return None;
        };

        remaining = &remaining[offset..];
    }

    // We ran out of pattern to match, so we had a match!
    Some((match_start, input_line.len() - remaining.len()))
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

    if let Some((start, end)) = match_pattern(&input_line, &pattern) {
        let bold = "\x1b[1m";
        let regular = "\x1b[22m";
        println!(
            "{}{}{}{}{}",
            &input_line[..start],
            bold,
            &input_line[start..end],
            regular,
            &input_line[end..]
        );
        process::exit(0)
    } else {
        process::exit(1)
    }
}
