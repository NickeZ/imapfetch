use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::collections::HashSet;

use env_logger;
use imap;
use log::debug;
use native_tls;
use rpassword;
use structopt::StructOpt;
use indicatif;
//use email_format::Email;
//use email_format::rfc5322::Parsable;
use regex;

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
        RFC5322Parse(email_format::rfc5322::error::ParseError),
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

    impl From<email_format::rfc5322::error::ParseError> for Error {
        fn from(e: email_format::rfc5322::error::ParseError) -> Self {
            Error::RFC5322Parse(e)
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
    #[structopt(long = "mailboxes")]
    mboxes: Vec<String>
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

fn get_hdelim(session: &mut ImapSession) -> Result<String, Error> {
    // Fetch the delimiter
    let list = session.list(None, None)?;
    Ok(list
        .get(0)
        .map(|name| name.delimiter())
        .ok_or(Error::NoDelimiter)?
        .ok_or(Error::NoDelimiter)?.to_string())
}

/// Create filenames which use dot to separate hierarchies
fn get_names(session: &mut ImapSession) -> Result<Vec<String>, Error> {
    // Fetch list of all mailboxes
    let list = session.list(None, Some("*"))?;
    let mut res = Vec::new();
    for item in &*list {
        res.push(item.name().to_string());
    }
    Ok(res)
}

fn get_filenames(mailboxes: Vec<String>, hdelim: String) -> Result<Vec<(String, PathBuf)>, Error> {
    let mut res = Vec::new();
    for item in mailboxes {
        let mut filename = item.as_str().replace(hdelim.as_str(), ".");
        filename.push_str(".mbox");
        res.push((item, PathBuf::from(filename)));
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
    //(*client).debug = true;
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


    let re = regex::bytes::Regex::new(r"(?i)message-id: ([^\r]*)").unwrap();

    let hdelim = get_hdelim(&mut imap_session)?;
    debug!("hdelim: {}", hdelim);
    // Directory names
    let mboxes = if opt.mboxes.len() > 0 {
        opt.mboxes.clone()
    } else {
        get_names(&mut imap_session)?
    };
    let names = get_filenames(mboxes, hdelim)?;
    debug!("Found {:?}", names);

    for name in names {
        let mut seen_mids = HashSet::new();
        let path = &name.1;
        // TODO: Check if file exists, collect all message ids
        if path.is_file() && path.metadata()?.len() > 0{
            //println!("File will be mapped");
            let mbox = imapfetch::Mboxfile::from_file(path)?;

            let mut count;
            count = 0;
            for entry in mbox.iter() {
                if let Some(cap) = re.captures(entry.data()) {
                    if let Some(mat) = cap.get(1) {
                        //println!("{:?}", entry);
                        //let (email, rem) = email_format::Email::parse(entry.data())?;
                        //let mid = format!("{}", email.get_message_id().unwrap());
    
                        seen_mids.insert(mat.as_bytes().to_vec());
                    }
                }
                count += 1;
            }
            println!("Found {} E-mails in mbox file", count);
            //for m in &seen_mids {
            //    println!("{:?}", std::str::from_utf8(m));
            //}
            //println!();
        }

        // open mailbox in readonly mode
        let mailbox = imap_session.examine(&name.0)?;

        debug!("Found {} mail", mailbox.exists);
        if mailbox.exists == 0 {
            continue;
        }

        // Create file when we know that mailbox can be opened
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(name.1)?;

        let sty = indicatif::ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}");
        let pb = indicatif::ProgressBar::new(2*(mailbox.exists as u64));
        //let pb = indicatif::ProgressBar::hidden();
        pb.set_style(sty);
        pb.set_message(&name.0);

        let mut uids = HashSet::new();

        // Get message-id / uid for all e-mails, take 100 at a time
        const CHUNK_SIZE: u32 = 100;
        for i in 1..2+(mailbox.exists / CHUNK_SIZE) {
            let start = (i - 1) * CHUNK_SIZE + 1;
            let end = std::cmp::min(i * CHUNK_SIZE, mailbox.exists);
            let seq = format!("{}:{}", start, end);
            //let seq = format!("{}", i);
            debug!("Fetching {}", seq);
            let messages = imap_session.fetch(seq, "(ENVELOPE UID)").unwrap();
            for message in &*messages {
                let line = if let Some(e) = message.envelope() {
                    if let Some(mid) = &e.message_id {
                        if !seen_mids.contains(mid.as_bytes()) {
                            if let Some(uid) = &message.uid {
                                uids.insert(uid.clone());
                            }
                        }
                    }
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
            pb.inc(std::cmp::min(CHUNK_SIZE, mailbox.exists - (i-1)*CHUNK_SIZE) as u64);

            //if i > 1 {
            //    break;
            //}
        }

        pb.inc(mailbox.exists as u64-uids.len() as u64);

        println!("Number of e-mails to fetch: {}", uids.len());

        // Get body of messages, 1 at a time?
        //for i in 1..mailbox.exists+1 {
        for uid in uids {
            let seq = format!("{}", uid);
            let messages = imap_session.uid_fetch(seq, "(RFC822)").unwrap();
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
            pb.inc(1);
            // if i > 5 {
            //     break;
            // }
        }

        pb.finish();
        //pb.finish_with_message(&name.0);

        // TODO: Write new files to disk
    }

    imap_session.logout()?;
    Ok(())
}

fn list_mailboxes(opt: &OptList) -> Result<(), Error> {
    let mut imap_session = create_session(&opt.conn)?;

    // Directory names
    let names = get_names(&mut imap_session)?;
    let hdelim = get_hdelim(&mut imap_session)?;

    if names.len() > 0 {
        println!("Found mailboxes:");
    }
    for (mailbox, filename) in get_filenames(names, hdelim)? {
        println!("  {}: {:?}", mailbox, filename);
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
