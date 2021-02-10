//! Example Accounts Aggregate toolset.
//!
//! Included:
//! - main executable controlling workflow of application
//! - domain models used to build `Account` aggregate
//! - data generator subpackage
//!
//! For help:
//! ```bash
//! cargo run -- -h
//! ```

mod models;

use std::io;
use std::fs::File;
use std::collections::HashMap;

use clap::{Arg, App};
use csv::{Reader, Writer};

use models::{Command, Account};

/// Procedural execution of application workflow.
///
/// **Steps:**
/// 1. Bootstrap clap cli argument parser.
/// 2. Get file handle for data source.
/// 3. Stream transaction records using csv + serde to deserialize models.
/// 4. For each transaction record build aggregate and apply events to projection.
/// 5. For each aggregate account serialize using csv + serde and write to stdout.
///
/// Desperately needs a logger w/log levels.
fn main() {
    // bootstrap clap thus getting source filepath
    let arg_matches = App::new("account-aggregate")
        .version("0.1.0")
        .arg(Arg::with_name("source")
            .help("source of transactions (filepath)")
            .required(true)
            .index(1))
        .get_matches();
    let source = arg_matches.value_of("source").unwrap();

    // todo - sanity check file / input

    // todo - custom errors in domain model

    // todo - replace in-memory projection with disk-backed solution for scale... or get moar memories
    // todo - sled(beta) embedded vs external db
    let mut accounts: HashMap<u16, Account> = HashMap::new();

    // read source file while handling aggregate commands / transactions
    let file = File::open(source).unwrap();
    let mut reader = Reader::from_reader(file);
    // fixme - error handling / logging for failed transactions
    for result in reader.deserialize() {
        let record: Command = result.unwrap();
        let client = record.client.clone();
        // check for existing account
        if let Some(account) = accounts.get_mut(&client) {
            if let Ok(events) = account.handle(record) {
                account.apply(events);
            }
        } else {
            // account is new, genesis time
            let mut account = Account::new(client);
            if let Ok(events) = account.handle(record) {
                account.apply(events);
                accounts.insert(client, account);
            }
        }
    }

    // write aggregates to stdout
    let mut writer = Writer::from_writer(io::stdout());
    for (_, account) in accounts {
        writer.serialize(account).unwrap();
    }
    writer.flush().unwrap();
}
