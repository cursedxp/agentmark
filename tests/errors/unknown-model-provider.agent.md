> tests/errors/unknown-model-provider.agent.md:8
>   - model: opnai:gpt-4o
>            ^^^^^^^^^^^^
>   Unknown model provider "opnai". Did you mean "openai"?
>   A model is claude, claude:<model>, openai:<model>, local:<model> or mock.

# Agent: greeter
- model: opnai:gpt-4o
- tools: log

## On start
- log: "hi"
