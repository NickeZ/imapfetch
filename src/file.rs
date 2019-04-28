use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;

use crate::parser::Parser;
use memmap::Mmap;

pub struct Mboxfile {
    mmap: memmap::Mmap,
}

impl Mboxfile {
    pub fn from_file(path: &Path) -> io::Result<Self> {
        let path = File::open(path)?;
        let mmap = unsafe { Mmap::map(&path)? };
        Ok(Mboxfile { mmap })
    }

    pub fn as_slice(&self) -> &[u8] {
        &*self.mmap
    }

    pub fn iter(&self) -> MboxReader {
        MboxReader::new(self)
    }
}

pub struct Entry<'a> {
    data: &'a [u8],
    idx: usize,
}

impl<'a> Entry<'a> {
    pub fn new(data: &'a [u8], idx: usize) -> Entry<'a> {
        Entry { data, idx }
    }
}

impl<'a> fmt::Debug for Entry<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Entry {} {:?}",
            self.idx,
            std::str::from_utf8(self.data).map(|s| s.get(..10))
        )
    }
}

pub struct MboxReader<'a> {
    buf: &'a [u8],
    parser: Parser<'a>,
    count: usize,
}

impl<'a> MboxReader<'a> {
    pub fn new(file: &Mboxfile) -> MboxReader {
        MboxReader {
            buf: file.as_slice(),
            parser: Parser::new(file.as_slice()),
            count: 0,
        }
    }
}

impl<'a> Iterator for MboxReader<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let res = match self.parser.next() {
            Some(item) => Some(Entry::new(item, self.count)),
            None => None,
        };
        self.count += 1;
        res
    }
}

#[cfg(test)]
mod tests {
    use crate::Mboxfile;

    #[test]
    fn mmap_works() {
        let mbox = Mboxfile::from_file("test/example1.mbox").expect("File not found");

        for entry in mbox.iter() {
            println!("{:?}", entry);
        }
    }

}
