> Turns titles into file-name words. Pure, so it can be used inside expressions.
# Tool: slugger
- effect: pure

> "Hello World" becomes "hello-world".
## slug(title: text) -> text
- set lowered = text.lower(title)
- return regex.replace(lowered, "[^a-z0-9]+", "-")

> The word in capitals; written in Rust, which is what a verb may do when AgentMark has no word for it.
## shout(word: text) -> text
```rust
word.to_uppercase()
```
