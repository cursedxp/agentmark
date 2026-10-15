> tests/errors/unsupported-model-bedrock.agent.md:10
>   - model: bedrock:anthropic.claude-3
>            ^^^^^^^^^^^^^^^^^^^^^^^^^^
>   The model provider "bedrock" is not supported in this version.
>   A model is claude, claude:<model>, openai:<model>, local:<model> or mock.

# Agent: greeter
Greets on start.

- model: bedrock:anthropic.claude-3
- tools: log

## On start
1. log: "hi"
