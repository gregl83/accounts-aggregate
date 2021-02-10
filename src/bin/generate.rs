use std::io;
use std::fs::File;

use simple_logger::SimpleLogger;
use clap::{Arg, App};
use rust_decimal::Decimal;
use csv::{
    Writer,
    Reader,
};
use serde::{Serialize, Deserialize};

type ClientId = u16;
type TransactionId = u32;

fn main() {
    // bootstrap logger
    SimpleLogger::new().init().unwrap();

    // process command line arguments / options
    let arg_matches = App::new("generate")
        .version("0.1.0")
        .arg(Arg::with_name("clients")
            .short("c")
            .long("clients")
            .value_name("clients")
            .help("Number of clients")
            .takes_value(true))
        .arg(Arg::with_name("transactions")
            .short("t")
            .long("transactions")
            .value_name("transactions")
            .help("Number of transactions")
            .takes_value(true))
        .get_matches();

    let total_clients: u16 = arg_matches
        .value_of("clients")
        .unwrap_or(format!("{}", ClientId::MAX).as_str())
        .parse()
        .unwrap();
    let total_transactions: u32 = arg_matches
        .value_of("transactions")
        .unwrap_or(format!("{}", TransactionId::MAX).as_str())
        .parse()
        .unwrap();

    log::info!("Generating {} transactions over {} clients", total_transactions, total_clients);

    //
    let destination = io::stdout();
    let mut writer = Writer::from_writer(destination);


}