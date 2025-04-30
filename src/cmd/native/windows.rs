use std::ffi::{OsStr, OsString};
use std::iter;
use std::iter::Peekable;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::str::Chars;

trait CharsExt {
    fn advance_while<P: FnMut(char) -> bool>(&mut self, predicate: P) -> usize;
}

impl CharsExt for Peekable<Chars<'_>> {
    fn advance_while<P: FnMut(char) -> bool>(&mut self, mut predicate: P) -> usize {
        let mut counter = 0;
        while let Some(c) = self.peek() {
            if !predicate(*c) {
                break;
            }
            counter += 1;
            self.next();
        }
        counter
    }
}

// Parsing command line arguments from singe line.
// See also: http://msdn.microsoft.com/en-us/library/17w5ykft.aspx
pub fn parse(cmd_line: &str) -> crate::Result<Vec<String>> {
    const BACKSLASH: char = '\\';
    const QUOTE: char = '"';
    const TAB: char = '\t';
    const SPACE: char = ' ';
    const NEWLINE: char = '\n';
    const RETURN: char = '\r';

    let mut ret_val = Vec::<String>::new();

    let mut code_units = cmd_line.trim().chars().peekable();

    // Parse the arguments according to these rules:
    // * All code units are taken literally except space, tab, quote and backslash.
    // * When not `in_quotes`, space and tab separate arguments. Consecutive spaces and tabs are
    // treated as a single separator.
    // * A space or tab `in_quotes` is taken literally.
    // * A quote toggles `in_quotes` mode unless it's escaped. An escaped quote is taken literally.
    // * A quote can be escaped if preceded by an odd number of backslashes.
    // * If any number of backslashes is immediately followed by a quote then the number of
    // backslashes is halved (rounding down).
    // * Backslashes not followed by a quote are all taken literally.
    // * If `in_quotes` then a quote can also be escaped using another quote
    // (i.e. two consecutive quotes become one literal quote).
    let mut cur = Vec::new();
    let mut in_quotes = false;
    while let Some(c) = code_units.next() {
        match c {
            // If not `in_quotes`, a space or tab ends the argument.
            SPACE | NEWLINE | RETURN | TAB if !in_quotes => {
                ret_val.push(String::from_iter(&cur[..]));
                cur.truncate(0);

                // Skip whitespace.
                code_units.advance_while(|w| w == SPACE || w == NEWLINE || w == RETURN || w == TAB);
            }
            // Backslashes can escape quotes or backslashes but only if consecutive backslashes are followed by a quote.
            BACKSLASH => {
                let backslash_count = code_units.advance_while(|w| w == BACKSLASH) + 1;
                if code_units.peek() == Some(&QUOTE) {
                    cur.extend(iter::repeat_n(BACKSLASH, backslash_count / 2));
                    // The quote is escaped if there are an odd number of backslashes.
                    if backslash_count % 2 == 1 {
                        code_units.next();
                        cur.push(QUOTE);
                    }
                } else {
                    // If there is no quote on the end then there is no escaping.
                    cur.extend(iter::repeat_n(BACKSLASH, backslash_count));
                }
            }
            // If `in_quotes` and not backslash escaped (see above) then a quote either
            // unsets `in_quote` or is escaped by another quote.
            QUOTE if in_quotes => match code_units.peek() {
                // Two consecutive quotes when `in_quotes` produces one literal quote.
                Some(&QUOTE) => {
                    cur.push(QUOTE);
                    code_units.next();
                }
                // Otherwise set `in_quotes`.
                Some(_) => in_quotes = false,
                // The end of the command line.
                // Push `cur` even if empty, which we do by breaking while `in_quotes` is still set.
                None => break,
            },
            // If not `in_quotes` and not BACKSLASH escaped (see above) then a quote sets `in_quote`.
            QUOTE => in_quotes = true,
            // Everything else is always taken literally.
            _ => cur.push(c),
        }
    }
    // Push the final argument, if any.
    if !cur.is_empty() || in_quotes {
        ret_val.push(String::from_iter(&cur[..]));
    }
    Ok(ret_val)
}

pub fn quote(arg: impl AsRef<OsStr>) -> crate::Result<OsString> {
    let arg_ref = arg.as_ref();

    let mut result = Vec::<u16>::new();
    let need_quote = arg_ref.is_empty()
        || arg_ref
            .encode_wide()
            .any(|c| c == ' ' as u16 || c == '\t' as u16);

    if need_quote {
        result.push('"' as u16);
    }

    let mut backslashes: usize = 0;
    for x in arg_ref.encode_wide() {
        if x == '\\' as u16 {
            backslashes += 1;
        } else {
            if x == '"' as u16 {
                // Add n+1 backslashes to total 2n+1 before internal '"'.
                result.extend((0..=backslashes).map(|_| '\\' as u16));
            }
            backslashes = 0;
        }
        result.push(x);
    }

    if need_quote {
        // Add n backslashes to total 2n before ending '"'.
        result.extend((0..backslashes).map(|_| '\\' as u16));
        result.push('"' as u16);
    }

    Ok(OsStringExt::from_wide(&result))
}

pub fn join<'a, I: IntoIterator<Item = &'a OsString>>(words: I) -> crate::Result<OsString> {
    Ok(words
        .into_iter()
        .map(quote)
        .collect::<crate::Result<Vec<OsString>>>()?
        .join(OsStr::new(" ")))
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
    assert_eq!(
        parse("\"\" \"abc\" d e \"\"").unwrap(),
        ["", "abc", "d", "e", ""]
    );
}

#[test]
fn test_parse_4() {
    assert_eq!(
        parse("a\\\\b d\"e f\"g h").unwrap(),
        ["a\\\\b", "de fg", "h"]
    );
}

#[test]
fn test_parse_5() {
    assert_eq!(parse("a\\\\\\\"b c d").unwrap(), ["a\\\"b", "c", "d"]);
}

#[test]
fn test_parse_6() {
    assert_eq!(
        parse("a\\\\\\\\\"b c\" d e").unwrap(),
        ["a\\\\b c", "d", "e"]
    );
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
        parse("/Fp\"Debug\\HelloWorld.pch\" /Fo\"Debug\\\\\" /Gd").unwrap(),
        ["/FpDebug\\HelloWorld.pch", "/FoDebug\\", "/Gd"]
    );
}
