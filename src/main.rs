use std::env;
use std::io;
use std::process;

enum PatternKind {
    Literal(char),
    Digit,
}

struct SubPattern {
    kind: PatternKind,
}

fn parse_pattern(pattern: &str) -> Vec<SubPattern> {
    let mut subpatterns = vec![];
    let mut chars = pattern.chars();

    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(nc) if nc == '\\' => subpatterns.push(SubPattern {
                    kind: PatternKind::Literal('\\'),
                }),
                Some(nc) if nc == 'd' => subpatterns.push(SubPattern {
                    kind: PatternKind::Digit,
                }),
                Some(_) => todo!(),
                None => todo!(),
            },
            c if c.is_alphanumeric() => subpatterns.push(SubPattern {
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
        _ => todo!(),
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
    let subpatterns = parse_pattern(pattern);
    if subpatterns.is_empty() {
        return true;
    }

    // Start by trying to find somewhere in the input where we can start a match.
    let Some((mut remaining, _)) = find_match_start(&input_line[0..], subpatterns.first().unwrap())
    else {
        return false;
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
