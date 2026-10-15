# Uses
- [Invoice](../records/invoice.agent.md)

# Import: crate sha2

# Function: invoice_fingerprint
A short fingerprint of an invoice: SHA-256 of its id and amount, written as hex.

- in: invoice
- out: text
- touches: nothing
- steps:
  1. sha2 sha256_digest "{invoice.id}{invoice.amount}" into digest
  2. return fingerprint(digest)

## Rust: fingerprint -> text
Only formats the bytes; the hashing itself comes from the sha2 binding.

- in: digest: bytes
- touches: nothing

```rust
digest.iter().map(|b| format!("{:02x}", b)).collect::<String>()
```
