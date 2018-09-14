use std::io::{Error, ErrorKind};

#[derive(PartialEq)]
enum Quote {
    None,
    Double,
    Single,
}

// Parsing command line arguments from singe line.
pub fn parse(cmd: &str) -> Result<Vec<String>, Error> {
    let mut args: Vec<String> = Vec::new();
    let mut arg: String = String::new();
    let mut slash: bool = false;
    let mut quote = Quote::None;
    let mut data = false;
    for c in cmd.chars() {
        match c {
            '\'' if quote == Quote::Single => {
                quote = Quote::None;
            }
            _ if quote == Quote::Single => {
                arg.push(c);
            }
            ' ' | '\t' | '"' | '\'' | '\\' if slash => {
                arg.push(c);
                slash = false;
            }
            _ if slash => {
                arg.push('\\');
                arg.push(c);
                slash = false;
            }
            '"' if quote == Quote::Double => {
                quote = Quote::None;
            }
            ' ' | '\t' if quote == Quote::None => {
                if data {
                    args.push(arg);
                    arg = String::new();
                    data = false;
                }
            }
            '\\' => {
                slash = true;
                data = true;
            }
            '"' => {
                quote = Quote::Double;
                data = true;
            }
            '\'' => {
                quote = Quote::Single;
                data = true;
            }
            _ => {
                arg.push(c);
                data = true;
            }
        }
    }
    if data {
        args.push(arg);
    }
    if slash {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Unexpected line end: escape sequence is not finished",
        ));
    }
    match quote {
        Quote::Single => Err(Error::new(
            ErrorKind::InvalidInput,
            "Unexpected line end: single quote is not closed",
        )),
        Quote::Double => Err(Error::new(
            ErrorKind::InvalidInput,
            "Unexpected line end: double quote is not closed",
        )),
        Quote::None => Ok(args),
    }
}

#[test]
fn test_parse_1() {
    assert_eq!(parse("\"abc\" d e").unwrap(), ["abc", "d", "e"]);
}

#[test]
fn test_parse_2() {
    assert_eq!(parse(" \"abc\" d e ").unwrap(), ["abc", "d", "e"]);
}

#[test]
fn test_parse_3() {
    assert_eq!(parse("\"\" \"abc\" d e \"\"").unwrap(), ["", "abc", "d", "e", ""]);
}

#[test]
fn test_parse_4() {
    assert_eq!(parse("a\\\\b d\"e f\"g h").unwrap(), ["a\\b", "de fg", "h"]);
}

#[test]
fn test_parse_5() {
    assert_eq!(parse("a\\\\\\\"b c d").unwrap(), ["a\\\"b", "c", "d"]);
}

#[test]
fn test_parse_6() {
    assert_eq!(parse("a\\\\\\\\\"b c\" d e").unwrap(), ["a\\\\b c", "d", "e"]);
}

#[test]
fn test_parse_7() {
    assert_eq!(
        parse("C:\\Windows\\System32 d e").unwrap(),
        ["C:\\Windows\\System32", "d", "e"]
    );
}

#[test]
fn test_parse_8() {
    assert_eq!(
        parse("/TEST\"C:\\Windows\\System32\" d e").unwrap(),
        ["/TESTC:\\Windows\\System32", "d", "e"]
    );
}

#[test]
fn test_parse_9() {
    assert_eq!(
        parse("begin ' some text \" foo\\ bar\\' end").unwrap(),
        ["begin", " some text \" foo\\ bar\\", "end"]
    );
}

#[test]
fn test_parse_10() {
    assert_eq!(parse("begin some\\ text end").unwrap(), ["begin", "some text", "end"]);
}
