# Import: ../records/invoice.agent.md
# Import: crate sha2

> A short fingerprint of an invoice: SHA-256 of its id and amount, written as hex.
# Function: invoice_fingerprint
- in: invoice: Invoice
- out: text
- effect: pure
  - sha2 sha256_digest "{invoice.id}{invoice.amount}" -> digest
  - return fingerprint(digest)

> Only formats the bytes; the hashing itself comes from the sha2 binding.
## Rust: fingerprint -> text
- in: digest: bytes
- effect: pure
  ```rust
  digest.iter().map(|b| format!("{:02x}", b)).collect::<String>()
  ```
