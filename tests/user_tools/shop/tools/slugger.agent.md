# Tool: slugger
Turns titles into file-name words. Pure, so it can be used inside expressions.

- touches: nothing

## slug(title: text) -> text
"Hello World" becomes "hello-world".

1. text.lower(title) into lowered
2. return regex.replace(lowered, "[^a-z0-9]+", "-")

## shout(word: text) -> text
The word in capitals; written in Rust, which is what a verb may do when AgentMark has no word for it.

```rust
word.to_uppercase()
```
