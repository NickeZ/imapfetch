use std::io;
use std::io::prelude::*;
use std::path::PathBuf;

use env_logger;
use imap;
use log::debug;
use native_tls;
use rpassword;
use structopt::StructOpt;

use imapfetch;

mod error {
    use std::io;

    #[derive(Debug)]
    pub enum Error {
        NoDelimiter,
        NotATTY,
        Imap(imap::error::Error),
        Io(io::Error),
        NativeTLS(native_tls::Error),
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

    impl From<native_tls::Error> for Error {
        fn from(e: native_tls::Error) -> Self {
            Error::NativeTLS(e)
        }
    }

}

use error::Error;

type ImapSession = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;

// TODO: Fix cli args...

//#[derive(Debug, StructOpt)]
//struct Opt {
//    #[structopt(subcommand)]
//    command: Command,
//    #[structopt(short="v", long="verbose", help="Print some more information")]
//    verbose: bool,
//    #[structopt(short="d", long="debug", help="Print debug information")]
//    debug: bool,
//}

#[derive(Debug, StructOpt)]
enum Opt {
    #[structopt(name = "list")]
    List(OptList),
    #[structopt(name = "backup")]
    Backup(OptBackup),
}

#[derive(Debug, StructOpt)]
struct OptList {
    #[structopt(flatten)]
    conn: Connection,
}

#[derive(Debug, StructOpt)]
struct OptBackup {
    #[structopt(
        long = "path",
        help = "Specify output directory (default: current working directory)"
    )]
    path: Option<PathBuf>,
    #[structopt(long = "compress", help = "Compress mbox file")]
    compress: bool,
    #[structopt(flatten)]
    conn: Connection,
}

#[derive(Debug, StructOpt)]
struct Connection {
    #[structopt(help = "IMAP host")]
    host: String,
    #[structopt(long = "user", help = "IMAP username")]
    user: String,
    #[structopt(long = "password", help = "IMAP password")]
    password: Option<String>,
    #[structopt(long = "tls", help = "Use tls (port 993)")]
    tls: bool,
}

/// Create filenames which use dot to separate hierarchies
fn get_names(session: &mut ImapSession) -> Result<Vec<(String, String)>, Error> {
    // Fetch the delimiter
    let list = session.list(None, None)?;
    let hdelim = list
        .get(0)
        .map(|name| name.delimiter())
        .ok_or(Error::NoDelimiter)?
        .ok_or(Error::NoDelimiter)?;
    debug!("hdelim: {}", hdelim);

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

fn create_session(opt: &Connection) -> Result<ImapSession, Error> {
    let qstr = format!("Enter password for {}:", opt.user);
    let mut password = match &opt.password {
        Some(pw) => pw.clone(),
        None => rpassword::read_password_from_tty(Some(&qstr))?,
    };

    let tls = native_tls::TlsConnector::builder().build()?;
    let mut client = imap::connect((&*opt.host, 993), &opt.host, &tls)?;
    let mut count = 3;
    loop {
        match client.login(&opt.user, password) {
            Ok(s) => return Ok(s),
            Err((e, c)) => {
                count -= 1;
                if count < 1 {
                    return Err(e.into());
                }
                client = c;
                password = rpassword::read_password_from_tty(Some(&qstr))?;
            }
        }
    }

    // with and without TLS are different types??
    // if opt.tls {
    //     let tls = native_tls::TlsConnector::builder().build()?;
    //     let client = imap::connect((&*opt.host, 993), &opt.host, &tls)?;
    //     client.login(&opt.user, password)
    // }
    // else {
    //     let client = imap::connect_insecure((&*opt.host, 143))?;
    //     client.login(&opt.user, password)
    // }
}

fn backup_mailboxes(opt: &OptBackup) -> Result<(), Error> {
    let mut imap_session = create_session(&opt.conn)?;

    // Directory names
    let names = get_names(&mut imap_session)?;
    debug!("Found {:?}", names);

    for name in names {
        print!("Fetching for {} ... ", name.0);
        // TODO: Check if file exists, collect all message ids
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(name.1)?;
        // open mailbox in readonly mode
        let mailbox = imap_session.examine(name.0).unwrap();

        debug!("Found {} mail", mailbox.exists);

        // Get message-id / uid for all e-mails, take 100 at a time
        const CHUNK_SIZE: u32 = 100;
        for i in 1..mailbox.exists / CHUNK_SIZE {
            let start = (i - 1) * CHUNK_SIZE + 1;
            let end = std::cmp::min(i * CHUNK_SIZE, mailbox.exists);
            let seq = format!("{}:{}", start, end);
            //let seq = format!("{}", i);
            debug!("Fetching {}", seq);
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
                    if &body[body.len() - 2..body.len()] == &b"\r\n"[..] {
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

        println!("Done!");

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
    imap_session.logout()?;
    Ok(())
}

fn list_mailboxes(opt: &OptList) -> Result<(), Error> {
    let mut imap_session = create_session(&opt.conn)?;

    // Directory names
    let names = get_names(&mut imap_session)?;

    if names.len() > 0 {
        println!("Found mailboxes:");
    }
    for (mailbox, filename) in names {
        println!("  {}: {}", mailbox, filename);
    }
    imap_session.logout()?;
    Ok(())
}

fn main() -> Result<(), Error> {
    env_logger::init();
    match Opt::from_args() {
        Opt::List(list) => list_mailboxes(&list),
        Opt::Backup(backup) => backup_mailboxes(&backup),
    }
}
