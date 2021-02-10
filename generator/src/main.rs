use std::io;

use simple_logger::SimpleLogger;
use rand::{Rng, thread_rng, seq::SliceRandom};
use clap::{Arg, App};
use rust_decimal::Decimal;
use csv::Writer;
use serde::Serialize;
use log::LevelFilter;

type ClientId = u16;
type TransactionId = u32;
type Currency = Decimal;

#[derive(Debug, Serialize)]
struct Transaction {
    #[serde(rename = "type")]
    command: &'static str,
    client: ClientId,
    tx: TransactionId,
    amount: Option<Currency>
}

fn main() {
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
        .arg(Arg::with_name("v")
            .short("v")
            .multiple(true)
            .help("Sets the level of verbosity"))
        .get_matches();

    // bootstrap logger
    let level = match arg_matches.occurrences_of("v") {
        0 => LevelFilter::Off,
        1 => LevelFilter::Info,
        2 => LevelFilter::Debug,
        3 | _ => LevelFilter::Trace,
    };
    SimpleLogger::new()
        .with_level(level)
        .init()
        .unwrap();

    // get data generation scope
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

    log::info!("Generating {} transactions for {} clients", total_transactions, total_clients);

    // generate transactions
    let mut transactions_written: u32 = 0;
    let destination = io::stdout();
    let mut writer = Writer::from_writer(destination);

    log::debug!("Generating {} initial deposits", total_clients);

    let mut rng = thread_rng();
    let client_ids: Vec<_> = (1..total_clients).collect();
    for client_chunk in client_ids.chunks(50) {
        let mut ids = client_chunk.to_vec();
        ids.shuffle(&mut rng);
        for client in ids.iter() {
            writer.serialize(Transaction {
                command: "deposit",
                client: *client,
                tx: transactions_written + 1,
                amount: Some(Decimal::new(rng.gen_range(300000..5000000), 4))
            }).unwrap();
            transactions_written += 1;
        }
    }

    // fixme - logic too clean / predictable
    let remaining_transactions = (total_transactions - transactions_written) as f64;
    let mut deposits = (remaining_transactions * 0.4) as u32;
    let mut withdrawals = (remaining_transactions * 0.4) as u32;
    let mut disputes = (remaining_transactions * 0.15) as u32;
    let mut resolves = (remaining_transactions * 0.025) as u32;
    let mut chargebacks = (remaining_transactions * 0.025) as u32;

    writer.flush().unwrap();
    log::debug!("Generating {} deposits", deposits);
    log::debug!("Generating {} withdrawals", withdrawals);
    log::debug!("Generating {} disputes", disputes);
    log::debug!("Generating {} resolves", resolves);
    log::debug!("Generating {} chargebacks", chargebacks);

    let mut rounded_total = deposits + withdrawals + disputes + resolves + chargebacks;

    // todo - refactor pls
    while rounded_total > 0 {
        let client = rng.gen_range(1..total_clients);
        if deposits > 0 {
            writer.serialize(Transaction {
                command: "deposit",
                client,
                tx: transactions_written + 1,
                amount: Some(Decimal::new(rng.gen_range(300000..5000000), 4))
            }).unwrap();
            deposits -= 1;
            transactions_written += 1;
            rounded_total -= 1;
        }
        if rounded_total > 0 && withdrawals > 0 {
            writer.serialize(Transaction {
                command: "withdraw",
                client,
                tx: transactions_written + 1,
                amount: Some(Decimal::new(rng.gen_range(100000..4000000), 4))
            }).unwrap();
            withdrawals -= 1;
            transactions_written += 1;
            rounded_total -= 1;
        }
        if rounded_total > 0 && disputes > 0 {
            let dispute_id = transactions_written - 1;
            writer.serialize(Transaction {
                command: "dispute",
                client,
                tx: dispute_id,
                amount: None
            }).unwrap();
            disputes -= 1;
            transactions_written += 1;
            rounded_total -= 1;
            if rounded_total > 0 && resolves > 0 {
                writer.serialize(Transaction {
                    command: "resolve",
                    client,
                    tx: dispute_id,
                    amount: None
                }).unwrap();
                resolves -= 1;
                transactions_written += 1;
                rounded_total -= 1;
            } else if rounded_total > 0 && chargebacks > 0 {
                writer.serialize(Transaction {
                    command: "chargeback",
                    client,
                    tx: dispute_id,
                    amount: None
                }).unwrap();
                chargebacks -= 1;
                transactions_written += 1;
                rounded_total -= 1;
            }
        }
    }

    writer.flush().unwrap();
    log::debug!("Generating {} more deposits", total_transactions - transactions_written);

    while total_transactions > transactions_written {
        let client = rng.gen_range(1..total_clients);
        writer.serialize(Transaction {
            command: "deposit",
            client,
            tx: transactions_written + 1,
            amount: Some(Decimal::new(rng.gen_range(300000..5000000), 4))
        }).unwrap();
        transactions_written += 1;
    }

    writer.flush().unwrap();
    log::info!("Generated {} transactions for {} clients", total_transactions, total_clients);
}