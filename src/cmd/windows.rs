// Parsing command line arguments from singe line.
// See also: http://msdn.microsoft.com/en-us/library/17w5ykft.aspx
pub fn parse(cmd: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut arg: String = String::new();
    let mut slash: usize = 0;
    let mut quote = false;
    let mut data = false;
    for c in cmd.chars() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                arg = add_slashes(arg, if quote && ((slash % 2) == 0) { slash / 2 } else { slash });
                slash = 0;
                if quote {
                    arg.push(c);
                    data = true;
                } else if data {
                    args.push(arg);
                    arg = String::new();
                    data = false;
                }
            }
            '\\' => {
                slash = slash + 1;
                data = true;
            }
            '"' => {
                arg = add_slashes(arg, slash / 2);
                if (slash & 1) == 0 {
                    quote = !quote;
                } else {
                    arg.push(c);
                }
                slash = 0;
                data = true;
            }
            _ => {
                arg = add_slashes(arg, if quote && ((slash % 2) == 0) { slash / 2 } else { slash });
                slash = 0;
                arg.push(c);
                data = true;
            }
        }
    }
    arg = add_slashes(arg, if quote && ((slash % 2) == 0) { slash / 2 } else { slash });
    if data {
        args.push(arg);
    }
    args
}

fn add_slashes(mut line: String, count: usize) -> String {
    for _ in 0..count {
        line.push('\\');
    }
    line
}

#[test]
fn test_parse_1() {
    assert_eq!(parse("\"abc\" d e"), ["abc", "d", "e"]);
}

#[test]
fn test_parse_2() {
    assert_eq!(parse(" \"abc\" d e "), ["abc", "d", "e"]);
}

#[test]
fn test_parse_3() {
    assert_eq!(parse("\"\" \"abc\" d e \"\""), ["", "abc", "d", "e", ""]);
}

#[test]
fn test_parse_4() {
    assert_eq!(parse("a\\\\b d\"e f\"g h"), ["a\\\\b", "de fg", "h"]);
}

#[test]
fn test_parse_5() {
    assert_eq!(parse("a\\\\\\\"b c d"), ["a\\\"b", "c", "d"]);
}

#[test]
fn test_parse_6() {
    assert_eq!(parse("a\\\\\\\\\"b c\" d e"), ["a\\\\b c", "d", "e"]);
}

#[test]
fn test_parse_7() {
    assert_eq!(parse("C:\\Windows\\System32 d e"), ["C:\\Windows\\System32", "d", "e"]);
}

#[test]
fn test_parse_8() {
    assert_eq!(
        parse("/TEST\"C:\\Windows\\System32\" d e"),
        ["/TESTC:\\Windows\\System32", "d", "e"]
    );
}

#[test]
fn test_parse_9() {
    assert_eq!(
        parse("/Fp\"Debug\\HelloWorld.pch\" /Fo\"Debug\\\\\" /Gd"),
        ["/FpDebug\\HelloWorld.pch", "/FoDebug\\", "/Gd"]
    );
}
