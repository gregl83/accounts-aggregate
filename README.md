# acounts-aggregate

Minimal accounts aggregation example using Rust

Processing account transactions basing bits off:

- [Event Sourcing](https://martinfowler.com/eaaDev/EventSourcing.html)
- [CQRS](https://martinfowler.com/bliki/CQRS.html)
- [Domain Driven Design](https://martinfowler.com/tags/domain%20driven%20design.html)

## Domain Vocabulary

### Aggregate

`Accounts` constructed from an immutable Event Stream. Executing commands on aggregate results in new events.

Responsible for domain business rules.

### Commands

Transactions to be performed on aggregate.

### Event Stream

Ordered collection of immutable events emitted from commands on aggregates.  

### Projection

State of `Accounts` after processing commands.

## Usage

```bash
accounts-aggregate <source-filepath>
```

## License

[MIT](LICENSE)
