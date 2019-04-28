use std::io;
use std::path::PathBuf;

use structopt::StructOpt;
use imap;
use native_tls;
use rpassword;
use env_logger;

use imapfetch;

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

fn main() -> io::Result<()>{
    env_logger::init();
    let opt = Opt::from_args();

    let tls = native_tls::TlsConnector::builder().build().unwrap();

    let client = imap::connect((&*opt.host, 993), &opt.host, &tls).unwrap();

    let password = match &opt.password {
        Some(pw) => pw.clone(),
        None => rpassword::read_password_from_tty(Some("Password:")).expect("Not a tty"),
    };
    let mut imap_session = client.login(&opt.user, password).unwrap();
    //imap_session.logout();

    // open mailbox in readonly mode
    let mailbox = imap_session.examine("INBOX").unwrap();

    println!("Found {} mail", mailbox.exists);

    for i in 1..mailbox.exists/10 {
        let start = (i-1)*10+1;
        let end = i*10;
        println!("Fetching {}:{}", start, end);
        let seq = format!("{}:{}", start, end);
        //let seq = format!("{}", i);
        let messages = imap_session.fetch(seq, "ENVELOPE").unwrap();
        for message in &*messages {
            let id = message.envelope().map(|e| e.message_id);
            println!("E-mail {:?}", id);
        }
        if i > 5 {
            break;
        }
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
    Ok(())
}
