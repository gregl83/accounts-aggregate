use simple_error::*;
use rust_decimal::prelude::Decimal;
use serde::{Serialize, Deserialize};

type Version = u32;
type ClientId = u16;
type TransactionId = u32;
type Currency = Decimal;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Command {
    #[serde(rename = "type")]
    name: CommandType,
    pub client: ClientId,
    tx: TransactionId,
    amount: Option<Currency>
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CommandType {
    Deposit,
    Withdraw,
    Dispute,
    Resolve,
    Chargeback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Credited { tx: TransactionId, amount: Currency },
    Debited { tx: TransactionId, amount: Currency },
    Held { tx: TransactionId, amount: Currency },
    Released { tx: TransactionId, amount: Currency },
    Reversed { tx: TransactionId, amount: Currency },
    Locked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    #[serde(skip_serializing)]
    version: Version,
    client:  ClientId,
    available: Currency,
    held: Currency,
    total: Currency,
    #[serde(skip_serializing)]
    locked: bool,
    #[serde(skip_serializing)]
    events: Vec<Event>
}

impl Account {
    pub fn new(client: ClientId) -> Self {
        Account {
            version: 0,
            client,
            available: Currency::new(0, 4),
            held: Currency::new(0, 4),
            total: Currency::new(0, 4),
            locked: false,
            events: vec![]
        }
    }

    fn find_genesis_amount(&self, key: TransactionId) -> Option<Currency> {
        let mut transaction_amount: Option<Currency> = None;
        for event in &self.events {
            if let Event::Credited { tx, amount } = event {
                if *tx == key {
                    transaction_amount = Some(amount.clone());
                    break;
                }
            }
            if let Event::Debited { tx, amount } = event {
                if *tx == key {
                    transaction_amount = Some(amount.clone());
                    break;
                }
            }
        }
        transaction_amount
    }

    fn find_dispute_amount(&self, key: TransactionId) -> Option<Currency> {
        let mut transaction_amount: Option<Currency> = None;
        for event in &self.events {
            if let Event::Held { tx, amount } = event {
                if *tx == key {
                    transaction_amount = Some(amount.clone());
                    break;
                }
            }
        }
        transaction_amount
    }

    pub fn apply(&mut self, events: Vec<Event>) {
        for event in events {
            match event {
                Event::Credited { amount, .. } => {
                    self.available += amount;
                }
                Event::Debited { amount, .. } => {
                    self.available -= amount;
                }
                Event::Held { amount, .. } => {
                    self.available -= amount;
                    self.held += amount;
                }
                Event::Released { amount, .. } => {
                    self.held -= amount;
                    self.available += amount;
                }
                Event::Reversed { amount, .. } => {
                    self.held -= amount;
                }
                Event::Locked => {
                    self.locked = true;
                }
            };
            self.total = self.available + self.held;
            self.version += 1;
            self.events.push(event);
        }
    }

    pub fn handle(&self, command: Command) -> Result<Vec<Event>, SimpleError> {
        if self.locked {
            bail!("unable to process transaction({}) having locked account({})", command.tx, command.client);
        }

        let events = match command.name {
            CommandType::Deposit => {
                vec![Event::Credited {tx: command.tx, amount: command.amount.unwrap()}]
            }
            CommandType::Withdraw => {
                vec![Event::Debited {tx: command.tx, amount: command.amount.unwrap()}]
            }
            CommandType::Dispute => {
                let amount = self.find_genesis_amount(command.tx);
                if amount.is_none() {
                    bail!("unable to find account({}) transaction({}) to dispute", command.client, command.tx);
                }
                vec![Event::Held {tx: command.tx, amount: amount.unwrap()}]
            }
            CommandType::Resolve => {
                let amount = self.find_dispute_amount(command.tx);
                if amount.is_none() {
                    bail!("unable to find disputed account({}) transaction({}) to resolve", command.client, command.tx);
                }
                vec![Event::Released {tx: command.tx, amount: amount.unwrap()}]
            }
            CommandType::Chargeback => {
                let amount = self.find_dispute_amount(command.tx);
                if amount.is_none() {
                    bail!("unable to find disputed account({}) transaction({}) to chargeback", command.client, command.tx);
                }
                vec![Event::Reversed {tx: command.tx, amount: amount.unwrap()}]
            }
        };

        Ok(events)
    }
}