extern crate csv;
extern crate getopts;
extern crate hipchat_client;
extern crate hipchat_export;
extern crate hyper;
extern crate time;

use getopts::Options;

use hipchat_client::Client as HipchatClient;
use hipchat_client::message::{Messages, MessagesRequest};
use hipchat_client::user::{UsersRequest};

use std::env;
use std::io::prelude::*;
use std::fs::{File, DirBuilder};

use hyper::Client as HyperClient;

use std::io;

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

fn encode_messages(messages: &Messages) -> Result<String,()> {
    let mut wtr = csv::Writer::from_memory();
    for message in &(messages.items) {

        let user_name = match message.from {
            Some(ref from) => from.name.to_string(),
            None => "".to_string()
        };

        let file_name = match message.file {
            Some(ref file) => file.name.to_string(),
            None => "".to_string()
        };

        let tuple = (&message.date, user_name, &message.message, file_name);
        wtr.encode(tuple).expect("failed to encode");
    }
    return Ok(wtr.as_string().to_string())
}

fn download_files(messages: &Messages, out_path:&String) -> Result<(), io::Error> {
    for message in &(messages.items) {

        if message.file.is_some() {
            let http = HyperClient::new();
            let ref file = message.file.as_ref();

            let file_path = &format!("{}/{} {}", out_path, &message.date, file.unwrap().name);
            println!("Downloading file '{}' to '{}'", file.unwrap().url, file_path);

            let mut res = http.get(&file.unwrap().url).send();
            let mut attachment_file = try!(File::create(file_path));
            try!(io::copy(res.as_mut().unwrap(), &mut attachment_file));
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("o", "", "set output directory", "PATH");
    opts.optopt("k", "", "hipchat API key", "KEY");
    opts.optopt("s", "", "resume at user with name", "NAME");
    opts.optflag("h", "help", "print this help menu");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => { m }
        Err(f) => {
            println!("{}", f.to_string());
            return;
        }
    };

    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }

    let output = matches.opt_str("o");
    if !output.is_some() {
        print_usage(&program, opts);
        return;
    };

    let apikey = matches.opt_str("k");
    if !apikey.is_some() {
        print_usage(&program, opts);
        return;
    };

    let hipchat_client = HipchatClient::new("https://api.hipchat.com", apikey.unwrap());

    let max_results = 500;
    let user_request = UsersRequest{start_index: Some(0), max_results: Some(max_results), ..Default::default()};
    let users = hipchat_client.get_users(Some(&user_request)).unwrap();

    let mut user_list:Vec<hipchat_client::user::User> = Vec::new();

    let skip_to = matches.opt_str("s");
    if skip_to.is_some() {
        let split_string = skip_to.unwrap();
        let split_at = users.items.iter().enumerate().find(|&r| r.1.name == split_string.to_string());
        if split_at.is_some() {
            let (_, short_list) = users.items.split_at(split_at.unwrap().0);
            user_list.append(&mut short_list.to_vec());
        } else {
            println!("can't find '{}'", split_string);
            return;
        }
    } else {
        user_list = users.items.clone();
    }

    for user in &(user_list) {
        let path = &format!("{}/{}", output.as_ref().unwrap(), user.name);
        let messages_path = &format!("{}/messages.csv", path);
        println!("Fetching messages from '{}' to '{}'", user.name, path);

        DirBuilder::new().recursive(true).create(path).unwrap();
        let mut messages_file = File::create(messages_path).unwrap();

        let mut request = MessagesRequest{
            start_index: Some(0),
            max_results: Some(max_results),
            reversed: Some(false),
            date: Some(format!("{}", time::now().rfc3339())),
            include_deleted: None,
            timezone: None,
            end_date: None};

        loop {
            let messages_option = hipchat_client.get_private_messages(user.id.to_string(), Some(&request));
            let messages = messages_option.as_ref().unwrap();

            if messages.items.is_empty()
                || messages.items.len() < max_results as usize {
                break;
            }

            let messages_encoded = encode_messages(&messages);
            if messages_encoded.is_ok() {
                messages_file.write_all(messages_encoded.as_ref().unwrap().as_bytes()).expect(&format!("failed to write to file {}", messages_path));
            }
            let downloads_res = download_files(&messages, &path);
            if downloads_res.is_err() {
                println!("Error downloading file: {:?}", downloads_res.err());
            }

            request.start_index = Some(request.start_index.unwrap() + max_results)
        }
        std::thread::sleep(std::time::Duration::new(3, 0)); // hipchat only allows 20 calls per minute
    }
    println!("Done!");
}
