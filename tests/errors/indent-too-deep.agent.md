> tests/errors/indent-too-deep.agent.md:14
>       1. log: "nested under a step that takes no steps"
>          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
>   Nothing can be indented under this step.

# Agent: greeter
Greets on start.

- model: claude
- tools: email, log

## On start
1. log: "hello"
    1. log: "nested under a step that takes no steps"
