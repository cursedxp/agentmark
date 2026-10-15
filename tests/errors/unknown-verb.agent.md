> tests/errors/unknown-verb.agent.md:13
>   1. email sned customer: "Hello"
>            ^^^^
>   Unknown verb "sned" for tool email. Did you mean "send"?

# Agent: greeter
Greets on start.

- model: claude
- tools: email, log

## On start
1. email sned customer: "Hello"
