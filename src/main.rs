use std::io;
use std::io::prelude::*;
use std::path::PathBuf;

use structopt::StructOpt;
use imap;
use native_tls;
use rpassword;
use env_logger;

use imapfetch;

mod error {
    use std::io;

    #[derive(Debug)]
    pub enum Error {
        NoDelimiter,
        Imap(imap::error::Error),
        Io(io::Error),
    }

    impl From<imap::error::Error> for Error {
        fn from(e: imap::error::Error) -> Self {
            Error::Imap(e)
        }
    }

    impl From<io::Error> for Error {
        fn from(e: io::Error) -> Self {
            Error::Io(e)
        }
    }

}

use error::Error;

type ImapSession = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(long="user", help="IMAP username")]
    user: String,
    #[structopt(long="password", help="IMAP password")]
    password: Option<String>,
    #[structopt(long="tls", help="Use tls (port 993)")]
    tls: bool,
    #[structopt(long="path", help="Specify output directory (default: current working directory)")]
    path: Option<PathBuf>,
    #[structopt(long="compress", help="Compress mbox file")]
    compress: bool,
    #[structopt(short="v", long="verbose", help="Print some more information")]
    verbose: bool,
    #[structopt(short="d", long="debug", help="Print debug information")]
    debug: bool,
    #[structopt(help="IMAP host")]
    host: String,
}

/// Create filenames which use dot to separate hierarchies
fn get_names(session: &mut ImapSession) -> Result<Vec<(String, String)>, Error> {
    // Fetch the delimiter
    let list = session.list(None, None)?;
    let hdelim = list.get(0).map(|name| name.delimiter()).ok_or(Error::NoDelimiter)?.ok_or(Error::NoDelimiter)?;
    println!("hdelim: {}", hdelim);

    // Fetch list of all mailboxes
    let list = session.list(None, Some("*"))?;
    let mut res = Vec::new();
    for item in &*list {
        let mut filename = item.name().replace(hdelim, ".");
        filename.push_str(".mbox");
        res.push((item.name().to_string(), filename));
    }
    Ok(res)

}

fn main() -> Result<(), Error>{
    env_logger::init();
    let opt = Opt::from_args();

    let tls = native_tls::TlsConnector::builder().build().unwrap();

    let client = imap::connect((&*opt.host, 993), &opt.host, &tls).unwrap();

    let password = match &opt.password {
        Some(pw) => pw.clone(),
        None => rpassword::read_password_from_tty(Some("Password:")).expect("Not a tty"),
    };
    let mut imap_session = client.login(&opt.user, password).unwrap();

    // Directory names
    let names = get_names(&mut imap_session)?;
    println!("Found {:?}", names);

    for name in names {
        // TODO: Check if file exists, collect all message ids
        let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(name.1)?;
        // open mailbox in readonly mode
        let mailbox = imap_session.examine(name.0).unwrap();

        println!("Found {} mail", mailbox.exists);

        // Get message-id / uid for all e-mails, take 100 at a time
        const CHUNK_SIZE: u32 = 100;
        for i in 1..mailbox.exists/CHUNK_SIZE {
            let start = (i-1)*CHUNK_SIZE+1;
            let end = std::cmp::min(i*CHUNK_SIZE, mailbox.exists);
            let seq = format!("{}:{}", start, end);
            //let seq = format!("{}", i);
            println!("Fetching {}", seq);
            let messages = imap_session.fetch(seq, "(ENVELOPE UID)").unwrap();
            for message in &*messages {
                let line = if let Some(e) = message.envelope() {
                    let from = e.from.as_ref().map(|v| v.get(0).map(|a| a.name));
                    format!("[{:?}] {:?} - {:?}", e.date, from, e.message_id)
                } else {
                    std::string::String::from_utf8(b"unkown".to_vec()).unwrap()
                };
                //if let Some(uid) = &message.uid {
                //    print!("{:06} ", uid);
                //}
                //println!("{}", line);
            }
            if i > 2 {
                break;
            }
        }

        // Get body of messages, 1 at a time?
        for i in 1..mailbox.exists {
            let seq = format!("{}", i);
            let messages = imap_session.fetch(seq, "(RFC822)").unwrap();
            for message in &*messages {
                if let Some(body) = message.body() {
                    file.write(b"From \r\n")?;
                    file.write(body)?;
                    if &body[body.len()-2..body.len()] == &b"\r\n"[..] {
                        file.write(b"\r\n")?;
                    } else {
                        file.write(b"\r\n\r\n")?;
                    }
                }
            }
            if i > 2 {
                break;
            }
        }

        // TODO: Write new files to disk
    }


    // let mbox = imapfetch::Mboxfile::from_file(&opt.path)?;

    // let mut count;
    // if opt.verbose {
    //     count = 0;
    //     for entry in mbox.iter() {
    //         println!("{:?}", entry);
    //         count += 1;
    //     }
    // } else {
    //     count = mbox.iter().fold(0, |i, _| i+1);
    // }
    // println!("Found {} E-mails in mbox file", count);
    imap_session.logout();
    Ok(())
}
