> tests/errors/unsupported-model-bedrock.agent.md:8
>   - model: bedrock:anthropic.claude-3
>            ^^^^^^^^^^^^^^^^^^^^^^^^^^
>   The model provider "bedrock" is not supported in this version.
>   A model is claude, claude:<model>, openai:<model>, local:<model> or mock.

# Agent: greeter
- model: bedrock:anthropic.claude-3
- tools: log

## On start
- log: "hi"
