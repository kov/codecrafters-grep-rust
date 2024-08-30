use lazy_static::lazy_static;
use log::trace;
use std::collections::HashMap;
use std::env;
use std::io;
use std::process;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
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
    AlternateGroups(usize, Vec<Vec<SubPattern>>),
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

lazy_static! {
    static ref BACKREFS: RwLock<HashMap<usize, String>> = RwLock::new(HashMap::new());
    static ref NUM_OF_BACKREFS: AtomicUsize = AtomicUsize::new(0);
}

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

                let mut nesting_depth = 0;
                while let Some(c) = chars.next() {
                    if c == '(' {
                        nesting_depth += 1;
                    } else if c == ')' {
                        nesting_depth -= 1;
                        if nesting_depth < 0 {
                            break;
                        }
                    } else if c == '|' {
                        if nesting_depth == 0 {
                            groups.push(std::mem::take(&mut contents));
                            continue;
                        }
                    }
                    contents.push(c);
                }
                groups.push(contents);

                SubPattern {
                    kind: PatternKind::AlternateGroups(
                        NUM_OF_BACKREFS.fetch_add(1, Ordering::SeqCst),
                        groups.iter().map(|group| parse_pattern(group)).collect(),
                    ),
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
            c if c == '\'' => SubPattern {
                kind: PatternKind::Literal(c),
                modifier: None,
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
        PatternKind::AlternateGroups(bref, groups) => {
            for g in groups {
                if let Some(offset) = match_all_subpatterns(remaining, g) {
                    BACKREFS
                        .write()
                        .unwrap()
                        .entry(*bref)
                        .or_insert(remaining[..offset].to_string());
                    return Some(offset);
                }
            }

            None
        }
        PatternKind::BackRef(i) => {
            if let Some(g) = BACKREFS.read().unwrap().get(&(*i - 1)) {
                if let Some((start, end)) = match_pattern(remaining, g.as_str(), true) {
                    if start == 0 {
                        return Some(end);
                    }
                }
                None
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
                if still_remaining.is_empty() {
                    break;
                }
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
        trace!("{n} attempt at finding first match...");
        if let Some(offset) = match_subpattern(&input[n..], sp) {
            trace!("Found first match at {n} offset {offset}");
            return Some((&input[n + offset..], n));
        }
    }
    None
}

fn match_all_subpatterns(input: &str, subpatterns: &[SubPattern]) -> Option<usize> {
    let mut remaining = input;
    for sp in subpatterns {
        trace!("MATCHING {sp:?} against {remaining}");
        let Some(offset) = match_subpattern(remaining, sp) else {
            return None;
        };

        remaining = &remaining[offset..];
    }

    Some(input.len() - remaining.len())
}

fn match_pattern(
    input_line: &str,
    pattern: &str,
    force_from_start: bool,
) -> Option<(usize, usize)> {
    let mut subpatterns = parse_pattern(pattern);
    if subpatterns.is_empty() {
        return Some((0, 0));
    }

    // Start by trying to find somewhere in the input where we can start a match.
    // Unless we have a line start pattern ^, in which case we simply drop that pattern and
    // expect matches to start at the beginning of input.
    let (mut remaining, match_start) = if force_from_start {
        (&input_line[0..], 0)
    } else {
        if let Some(SubPattern {
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
            subpatterns.remove(0);
            (remaining, match_start)
        }
    };

    // Try to match from there and fail if we cannot at some point.
    let mut previous_sp: Option<&SubPattern> = None;
    let mut previous_remaining = remaining;
    for sp in &subpatterns {
        trace!("MATCHING {sp:?} against {remaining}");
        let offset = match match_subpattern(remaining, sp) {
            Some(offset) => offset,
            None => {
                trace!("Backtracking...");
                if let Some(psp) = previous_sp {
                    if matches!(
                        psp.modifier,
                        Some(Modifier::ZeroOrMore) | Some(Modifier::OneOrMore)
                    ) || matches!(
                        previous_sp,
                        Some(SubPattern {
                            kind: PatternKind::AlternateGroups { .. },
                            ..
                        })
                    ) {
                        trace!("Matched prev {previous_remaining} <- {remaining}");
                        // We had a greedy operator before us, but still have a pattern to match, so we need to backtrack.
                        remaining = if matches!(psp.modifier, Some(Modifier::ZeroOrMore)) {
                            previous_remaining
                        } else {
                            &previous_remaining[1..]
                        };

                        let Some((_, match_start)) = find_match_start(remaining, sp) else {
                            return None;
                        };

                        remaining = &remaining[match_start..];

                        match_subpattern(remaining, sp).unwrap()
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
        };

        previous_sp = Some(sp);
        previous_remaining = remaining;

        remaining = &remaining[offset..];
    }

    // We ran out of pattern to match, so we had a match!
    Some((match_start, input_line.len() - remaining.len()))
}

// Usage: echo <input_text> | your_program.sh -E <pattern>
fn main() {
    env_logger::init();

    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    if env::args().nth(1).unwrap() != "-E" {
        println!("Expected first argument to be '-E'");
        process::exit(1);
    }

    let pattern = env::args().nth(2).unwrap();
    let mut input_line = String::new();

    io::stdin().read_line(&mut input_line).unwrap();

    if let Some((start, end)) = match_pattern(&input_line, &pattern, false) {
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
