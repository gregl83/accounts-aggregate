# accounts-aggregate

Minimal accounts aggregation example using Rust

Processing account transactions basing bits off:

- [Event Sourcing](https://martinfowler.com/eaaDev/EventSourcing.html)
- [CQRS](https://martinfowler.com/bliki/CQRS.html)
- [Domain Driven Design](https://martinfowler.com/tags/domain%20driven%20design.html)

This software is far from complete.

There are known issues, uncovered test cases, brute force implementations, missing or incomplete docs, etc. Production systems are far more complicated and matured using software development practices to harden services more-so when having large fiscal implications.

Does the current solution scale?

Well, sort of... trying to accomplish solutions with temporal constraints often means sacrificing _something_. In this case, transaction ids are `u32` value types giving them a maximum value of ~4.3B. That means the id of the transaction alone is `32 bits * ~4.3 * 10^9` records. Then there are the other values associates with transactions that must also be accounted for when determining how much memory is needed to process all the records without writing to disk (database, etc).

Using streams we can release _some_ of that data from memory and paired with a lookup hash for references required downstream (deposit and withdrawal).

Most of all event-driven architectures are backed by a store that persists to disk which helps prevent running out of memory when processing large volumes of data.

With more time, this solution _should_ be refactored to use a disk-backed store when memory constraints become a problem.

[sled](https://github.com/spacejam/sled) is an embedded disk-backed store that handles billions of transactions per minute but is still maturing.

A solution like `sled`, with more testing, could work well for this use-case.

Are there enough tests?

_Never._

## Usage 

```bash
cargo run -- <source-filepath>
```

## Docs

```bash
cargo doc --open
```

## Domain Vocabulary

#### Aggregate

`Accounts` constructed from an immutable Event Stream. Executing commands on aggregate results in new events.

Responsible for domain business rules.

#### Commands

Transactions to be performed on aggregate.

#### Event Stream

Ordered collection of immutable events emitted from commands on aggregates.  

#### Projection

State of `Accounts` after processing commands.

## Testing

Test data can be generated using the [generator](./generator) subpackage.

Account model command business rules have test coverage.

```bash
cargo test
```

## License

[MIT](LICENSE)
