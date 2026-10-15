> tests/errors/unknown-model-provider.agent.md:10
>   - model: opnai:gpt-4o
>            ^^^^^^^^^^^^
>   Unknown model provider "opnai". Did you mean "openai"?
>   A model is claude, claude:<model>, openai:<model>, local:<model> or mock.

# Agent: greeter
Greets on start.

- model: opnai:gpt-4o
- tools: log

## On start
1. log: "hi"
