use std::io;
use std::path::PathBuf;

use structopt::StructOpt;

use imapfetch;

#[derive(Debug, StructOpt)]
struct Opt {
    path: PathBuf,
    #[structopt(short = "v", long = "verbose", help = "Print some more information")]
    verbose: bool,
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();
    let mbox = imapfetch::Mboxfile::from_file(&opt.path)?;

    let mut count;
    if opt.verbose {
        count = 0;
        for entry in mbox.iter() {
            println!("{:?}", entry);
            count += 1;
        }
    } else {
        count = mbox.iter().fold(0, |i, _| i + 1);
    }
    println!("Found {} E-mails in mbox file", count);
    Ok(())
}
