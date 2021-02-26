//! Domain models for event sourcing the `Account` aggregate.

use simple_error::*;
use rust_decimal::prelude::Decimal;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

use crate::events::{Actor, Cause, Effect};

/// Version used to determine events applied to `Account` aggregate. Increments with event stream.
type Version = u32;
/// Client Id which is equivalent to `Account` aggregate Id.
type ClientId = u16;
/// Transaction Id representing initial command to aggregate (Withdrawal or Deposit).
type TransactionId = u32;
/// Current using Decimal package to avoid float arithmetic issues. (91 bits)
type Currency = Decimal;
/// Idempotency Key (UUID Version 4)
type IdempotencyKey = [u8; 16];

/// An action to perform for a given `Account` aggregate.
///
/// `Commands` are decoupled from query responsibilities.
/// Every command results in _at least_ one event.
///
/// (See [CQRS](https://martinfowler.com/bliki/CQRS.html)).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Command {
    #[serde(rename = "type")]
    name: CommandType,
    client: ClientId,
    tx: TransactionId,
    amount: Option<Currency>
}

impl Cause for Command {
    type ActorId = ClientId;
    fn actor_id(&self) -> Self::ActorId { self.client }
}

/// Type of `Commands` that can be handled by the `Account` aggregate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CommandType {
    Deposit,
    Withdraw,
    Dispute,
    Resolve,
    Chargeback,
}

/// Events that can occur from the `Account` aggregate.
///
/// When a change happens to an `Account` those effects are propagated outward using events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Event {
    Credited { key: IdempotencyKey, tx: TransactionId, amount: Currency },
    Debited { key: IdempotencyKey, tx: TransactionId, amount: Currency },
    Held { key: IdempotencyKey, tx: TransactionId, amount: Currency },
    Released { key: IdempotencyKey, tx: TransactionId, amount: Currency },
    Reversed { key: IdempotencyKey, tx: TransactionId, amount: Currency },
    Locked { key: IdempotencyKey },
}

impl Effect for Event {
    type Version = Version;
    type Key = IdempotencyKey;
    fn version(&self) -> Self::Version { 1 }
    fn idempotency_key(&self) -> Self::Key {
        match self {
            Event::Credited {key, ..} |
            Event::Debited {key, ..} |
            Event::Held {key, ..} |
            Event::Released {key, ..} |
            Event::Reversed {key, ..} |
            Event::Locked {key, ..} => { *key }
        }
    }
}

/// Aggregate that summarizes all `client` transactions.
///
/// Equivalent of a bank account.
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
    /// Returns new `Account` with `client` id set and defaults.
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

    /// Returns `amount` for first transaction event (ordered) matching key to transaction id(`tx`).
    fn find_genesis_amount(&self, key: TransactionId) -> Option<Currency> {
        let mut transaction_amount: Option<Currency> = None;
        for event in &self.events {
            if let Event::Credited { tx, amount, .. } = event {
                if *tx == key {
                    transaction_amount = Some(amount.clone());
                    break;
                }
            }
            if let Event::Debited { tx, amount, .. } = event {
                if *tx == key {
                    transaction_amount = Some(amount.clone());
                    break;
                }
            }
        }
        transaction_amount
    }

    /// Returns `amount` for first transaction event of type `Held` (ordered)
    /// matching key to transaction id(`tx`).
    ///
    /// `Event::Held` is emitted for `dispute` commands.
    fn find_dispute_amount(&self, key: TransactionId) -> Option<Currency> {
        let mut transaction_amount: Option<Currency> = None;
        for event in &self.events {
            if let Event::Held { tx, amount, .. } = event {
                if *tx == key {
                    transaction_amount = Some(amount.clone());
                    break;
                }
            }
        }
        transaction_amount
    }
}

impl Actor<Command, Event> for Account {
    type Id = ClientId;

    fn handle(&self, command: Command) -> Result<Vec<Event>, SimpleError> {
        if self.locked {
            bail!("unable to process transaction({}) having locked account({})", command.tx, command.client);
        }

        let key = *Uuid::new_v4().as_bytes();

        let events = match command.name {
            CommandType::Deposit => {
                let amount = command.amount;
                if amount.is_none() {
                    bail!("amount is none for deposit account({}) transaction({})", command.client, command.tx);
                }
                let event = Event::Credited {key, tx: command.tx, amount: amount.unwrap()};
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
                let event = Event::Debited {key, tx: command.tx, amount: amount_value};
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
                let event = Event::Held {key, tx: command.tx, amount: amount.unwrap()};
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
                let event = Event::Released {key, tx: command.tx, amount: amount.unwrap()};
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
                let event = Event::Reversed {key, tx: command.tx, amount: amount.unwrap()};
                if self.has_event(&event) {
                    bail!("duplicate chargeback account({}) transaction({})", command.client, command.tx);
                }
                vec![event, Event::Locked {key: *Uuid::new_v4().as_bytes()}]
            }
        };

        Ok(events)
    }

    fn apply(&mut self, events: Vec<Event>) {
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
                Event::Locked { .. } => {
                    self.locked = true;
                }
            };
            self.total = self.available + self.held;
            self.version += 1;
            self.events.push(event);
        }
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
    fn deposit_when_locked_declined() {
        let client = 1;
        let tx = 10;

        let mut account = Account::new(client);
        account.locked = true;
        let command = Command {
            name: CommandType::Deposit,
            client,
            tx,
            amount: Some(Decimal::new(990000, 4))
        };
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 0);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(0, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(0, 4));
        assert!(account.locked);
        assert_eq!(account.events.len(), 0);
    }

    #[test]
    #[ignore]
    fn deposit_duplicate_declined() {
        let client = 1;
        let tx = 10;

        let mut account = Account::new(client);
        let command = Command {
            name: CommandType::Deposit,
            client,
            tx,
            amount: Some(Decimal::new(990000, 4))
        };
        let events = account.handle(command.clone()).unwrap();
        account.apply(events);
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
            tx: tx + 1,
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
    fn withdraw_when_locked_declined() {
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
        account.locked = true;
        let command = Command {
            name: CommandType::Withdraw,
            client,
            tx: tx + 1,
            amount: Some(Decimal::new(400000, 4))
        };
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 1);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(990000, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(account.locked);
        assert_eq!(account.events.len(), 1);
    }

    #[test]
    #[ignore]
    fn withdraw_duplicate_declined() {
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
            tx: tx + 1,
            amount: Some(Decimal::new(400000, 4))
        };
        let events = account.handle(command.clone()).unwrap();
        account.apply(events);
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 2);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(590000, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(590000, 4));
        assert!(!account.locked);
        assert_eq!(account.events.len(), 2);
    }

    #[test]
    fn withdraw_when_balance_insufficient_declined() {
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
            tx: tx + 1,
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

    #[test]
    fn dispute_accepted() {
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
            name: CommandType::Dispute,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);

        assert_eq!(account.version, 2);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(0, 4));
        assert_eq!(account.held, Decimal::new(990000, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(!account.locked);
        assert_eq!(account.events.len(), 2);
    }

    #[test]
    fn dispute_when_locked_declined() {
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
        account.locked = true;
        let command = Command {
            name: CommandType::Dispute,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 1);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(990000, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(account.locked);
        assert_eq!(account.events.len(), 1);
    }

    #[test]
    fn dispute_missing_transaction_declined() {
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
            name: CommandType::Dispute,
            client,
            tx: tx + 1,
            amount: None
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
    fn resolve_for_dispute_accepted() {
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
            name: CommandType::Dispute,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        let command = Command {
            name: CommandType::Resolve,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);

        assert_eq!(account.version, 3);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(990000, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(!account.locked);
        assert_eq!(account.events.len(), 3);
    }

    #[test]
    #[ignore]
    fn resolve_for_dispute_duplicate_declined() {
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
            name: CommandType::Dispute,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        let command = Command {
            name: CommandType::Resolve,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        let command = Command {
            name: CommandType::Resolve,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 3);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(990000, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(!account.locked);
        assert_eq!(account.events.len(), 3);
    }

    #[test]
    fn resolve_for_dispute_when_locked_declined() {
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
            name: CommandType::Dispute,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        account.locked = true;
        let command = Command {
            name: CommandType::Resolve,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 2);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(0, 4));
        assert_eq!(account.held, Decimal::new(990000, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(account.locked);
        assert_eq!(account.events.len(), 2);
    }

    #[test]
    fn resolve_for_dispute_when_missing_transaction_declined() {
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
            name: CommandType::Dispute,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        let command = Command {
            name: CommandType::Resolve,
            client,
            tx: tx + 1,
            amount: None
        };
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 2);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(0, 4));
        assert_eq!(account.held, Decimal::new(990000, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(!account.locked);
        assert_eq!(account.events.len(), 2);
    }

    #[test]
    fn resolve_when_dispute_missing_declined() {
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
            name: CommandType::Resolve,
            client,
            tx,
            amount: None
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
    fn chargeback_for_dispute_accepted() {
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
            name: CommandType::Dispute,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        let command = Command {
            name: CommandType::Chargeback,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);

        assert_eq!(account.version, 4);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(0, 4));
        assert_eq!(account.held, Decimal::new(0, 4));
        assert_eq!(account.total, Decimal::new(0, 4));
        assert!(account.locked);
        assert_eq!(account.events.len(), 4);
    }

    #[test]
    fn chargeback_for_dispute_when_locked_declined() {
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
            name: CommandType::Dispute,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        account.locked = true;
        let command = Command {
            name: CommandType::Chargeback,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 2);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(0, 4));
        assert_eq!(account.held, Decimal::new(990000, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(account.locked);
        assert_eq!(account.events.len(), 2);
    }

    #[test]
    fn chargeback_for_dispute_when_missing_transaction_declined() {
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
            name: CommandType::Dispute,
            client,
            tx,
            amount: None
        };
        let events = account.handle(command).unwrap();
        account.apply(events);
        account.locked = true;
        let command = Command {
            name: CommandType::Chargeback,
            client,
            tx: tx + 1,
            amount: None
        };
        let events = account.handle(command);

        assert!(events.is_err());
        assert_eq!(account.version, 2);
        assert_eq!(account.client, client);
        assert_eq!(account.available, Decimal::new(0, 4));
        assert_eq!(account.held, Decimal::new(990000, 4));
        assert_eq!(account.total, Decimal::new(990000, 4));
        assert!(account.locked);
        assert_eq!(account.events.len(), 2);
    }

    #[test]
    fn chargeback_when_dispute_missing_declined() {
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
            name: CommandType::Chargeback,
            client,
            tx,
            amount: None
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