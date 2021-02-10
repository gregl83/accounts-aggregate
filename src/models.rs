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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

    fn has_event(&self, event: &Event) -> bool {
        self.events.iter().any(|e| { e == event })
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
                let amount = command.amount;
                if amount.is_none() {
                    bail!("amount is none for deposit account({}) transaction({})", command.client, command.tx);
                }
                let event = Event::Credited {tx: command.tx, amount: amount.unwrap()};
                if self.has_event(&event) {
                    bail!("duplicate deposit account({}) transaction({})", command.client, command.tx);
                }
                vec![event]
            }
            CommandType::Withdraw => {
                let amount = command.amount;
                if amount.is_none() {
                    bail!("amount is none for withdraw account({}) transaction({})", command.client, command.tx);
                }
                let amount_value = amount.unwrap();
                let event = Event::Debited {tx: command.tx, amount: amount_value};
                if self.has_event(&event) {
                    bail!("duplicate withdraw account({}) transaction({})", command.client, command.tx);
                }
                if amount_value > self.available {
                    bail!("amount({}) exceeds available({}) withdraw account({}) transaction({})", amount_value, self.available, command.client, command.tx);
                }
                vec![event]
            }
            CommandType::Dispute => {
                let amount = self.find_genesis_amount(command.tx);
                if amount.is_none() {
                    bail!("unable to find account({}) transaction({}) to dispute", command.client, command.tx);
                }
                let event = Event::Held {tx: command.tx, amount: amount.unwrap()};
                if self.has_event(&event) {
                    bail!("duplicate dispute account({}) transaction({})", command.client, command.tx);
                }
                vec![event]
            }
            CommandType::Resolve => {
                let amount = self.find_dispute_amount(command.tx);
                if amount.is_none() {
                    bail!("unable to find disputed account({}) transaction({}) to resolve", command.client, command.tx);
                }
                let event = Event::Released {tx: command.tx, amount: amount.unwrap()};
                if self.has_event(&event) {
                    bail!("duplicate resolve account({}) transaction({})", command.client, command.tx);
                }
                vec![event]
            }
            CommandType::Chargeback => {
                let amount = self.find_dispute_amount(command.tx);
                if amount.is_none() {
                    bail!("unable to find disputed account({}) transaction({}) to chargeback", command.client, command.tx);
                }
                let event = Event::Reversed {tx: command.tx, amount: amount.unwrap()};
                if self.has_event(&event) {
                    bail!("duplicate chargeback account({}) transaction({})", command.client, command.tx);
                }
                vec![event, Event::Locked]
            }
        };

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposit_accepted() {
        let client = 1;
        let tx = 10;

        let mut account = Account::new(client);
        let command = Command {
            name: CommandType::Deposit,
            client,
            tx,
            amount: Some(Decimal::new(990000, 4))
        };
        let events = account.handle(command).unwrap();
        account.apply(events);

        assert_eq!(account.version, 1);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(990000, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(!account.locked);
        assert_eq!(account.events.len(), 1);
    }

    #[test]
    fn deposit_declined_duplicate() {
        let client = 1;
        let tx = 10;

        let mut account = Account::new(client);
        let command = Command {
            name: CommandType::Deposit,
            client,
            tx,
            amount: Some(Decimal::new(990000, 4))
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        let command = Command {
            name: CommandType::Deposit,
            client,
            tx,
            amount: Some(Decimal::new(990000, 4))
        };
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 1);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(990000, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(!account.locked);
        assert_eq!(account.events.len(), 1);
    }

    #[test]
    fn withdraw_accepted() {
        let client = 1;
        let tx = 10;

        let mut account = Account::new(client);
        let command = Command {
            name: CommandType::Deposit,
            client,
            tx,
            amount: Some(Decimal::new(990000, 4))
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        let command = Command {
            name: CommandType::Withdraw,
            client,
            tx,
            amount: Some(Decimal::new(980000, 4))
        };
        let events = account.handle(command).unwrap();
        account.apply(events);

        assert_eq!(account.version, 2);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(10000, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(10000, 4));
        assert!(!account.locked);
        assert_eq!(account.events.len(), 2);
    }

    #[test]
    fn withdraw_declined_insufficient_balance() {
        let client = 1;
        let tx = 10;

        let mut account = Account::new(client);
        let command = Command {
            name: CommandType::Deposit,
            client,
            tx,
            amount: Some(Decimal::new(990000, 4))
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        let command = Command {
            name: CommandType::Withdraw,
            client,
            tx,
            amount: Some(Decimal::new(1000000, 4))
        };
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 1);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(990000, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(!account.locked);
        assert_eq!(account.events.len(), 1);
    }
}