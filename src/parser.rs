use std::io::prelude::*;

/// RFC 4155 excerpts:
///
/// The structure of the separator lines vary across implementations, but
/// usually contain the exact character sequence of "From", followed by a
/// single Space character (0x20), an email address of some kind, another
/// Space character, a timestamp sequence of some kind, and an end-of-
/// line marker.
///
/// Many implementations are also known to escape message body lines that
/// begin with the character sequence of "From ", so as to prevent
/// confusion with overly-liberal parsers that do not search for full
/// separator lines.  In the common case, a leading Greater-Than symbol
/// (0x3E) is used for this purpose (with "From " becoming ">From ").
/// However, other implementations are known not to escape such lines
/// unless they are immediately preceded by a blank line or if they also
/// appear to contain an email address and a timestamp.  Other
/// implementations are also known to perform secondary escapes against
/// these lines if they are already escaped or quoted, while others
/// ignore these mechanisms altogether.

pub struct Parser<'a> {
    buf: &'a [u8],
}

// TODO: Add constructors for sloppy mbox parsers
impl<'a> Parser<'a> {
    pub fn new(buf: &'a [u8]) -> Parser {
        Parser { buf }
    }
}

// For now, require that the FROM_ line is preceeded by an empty line
const FROM: &[u8] = b"\r\n\r\nFrom ";

pub fn read_until(buf: &[u8], needle: &[u8]) -> Result<usize, ()> {
    if needle.len() > buf.len() {
        return Err(());
    }
    for i in 0..(buf.len() - needle.len()) {
        if &buf[i..i + needle.len()] == needle {
            return Ok(i + 1);
        }
    }
    Err(())
}

impl<'a> Iterator for Parser<'a> {
    // Offset to next item
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        // The first byte of the message is after the newline
        let start = read_until(&self.buf, b"\n").ok()?;
        match read_until(&self.buf, FROM) {
            Ok(end) => {
                let res = &self.buf[start..end + 1];
                self.buf = &self.buf[end + FROM.len()..];
                Some(res)
            }
            Err(()) => {
                let res = &self.buf[start..];
                self.buf = &self.buf[start..start];
                Some(res)
            }
        }
    }
}
