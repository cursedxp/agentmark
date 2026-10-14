# `agentmark check --json`

One JSON document on stdout, pretty-printed. Exit code: 0 when `ok` is true, 1 otherwise. The keys below are stable;
new keys may be added, none are renamed or removed within v0.x. The VS Code panel (step 16) reads this.

```json
{
  "ok": true,
  "path": "examples/01-invoice-chaser",
  "diagnostics": [
    { "code": "W002", "severity": "warning", "file": "a.agent.md", "line": 4, "column": 3, "length": 12,
      "message": "...", "did_you_mean": null, "help": null }
  ],
  "units": [
    { "name": "invoice-chaser", "kind": "agent", "file": "examples/.../invoice-chaser.agent.md",
      "effects": { "class": "model", "summary": "model, network(crm, email), local(log)",
                   "tools": { "model": true, "network": ["crm", "email"], "local": ["log"], "unverified": [] } },
      "blocks": [
        { "label": "Every day at 09:00", "file": "...", "line": 8,
          "effects": { "class": "model", "summary": "model, network(crm, email), local(log)", "tools": { } } }
      ] }
  ],
  "inferred_fields": [
    { "record": "Order", "field": "amount", "type": "money", "fallback": false, "file": "...", "line": 2 }
  ],
  "unverified_bindings": [
    { "name": "sha2", "crate": "sha2", "file": "bindings/sha2.agent.md", "line": 5,
      "proposed_effect": "pure", "items": 1 }
  ],
  "rust_blocks": [
    { "name": "fingerprint", "file": "...", "line": 22, "parent": "net_amount", "effect": "pure" }
  ]
}
```

| key | meaning |
|---|---|
| `ok` | no error diagnostics **and** (no unverified bindings or `--allow-unverified` given) |
| `diagnostics` | every error and warning, ordered by file and line; `severity` is `"error"` or `"warning"`; `line`/`column` are 1-based; `did_you_mean` and `help` may be `null` |
| `units` | one entry per agent, service, command, function and rule (`kind` = `agent`, `service`, `command`, `function`, `rule`), in source order; `effects` is the union over its blocks; empty when the project has errors |
| `units[].blocks[]` | one per trigger, `## Do`, route, command `## Run`, function or rule body; `label` is the heading text (for a function or rule the name) |
| `effects.class` | highest class: `pure`, `local`, `network`, `model`, or `unverified` (only unapproved library calls) |
| `effects.summary` | the SPEC 8 text, e.g. `model, network(crm, email), local(log)` or `pure` |
| `effects.tools` | `model` (bool) and the names behind `network`, `local`, `unverified`, in first-use order |
| `inferred_fields` | every record field whose type came from its name (SPEC 2.1); `fallback: true` = no rule matched, it became `text` |
| `unverified_bindings` | libraries whose `- effect:` line nobody approved; `line` is that line, `proposed_effect` the binder's suggestion (or `null`) |
| `rust_blocks` | every `## Rust` block with its declared (trusted) `effect`; `parent` is the declaration it was written under or `null` |

Unused imports and declarations (SPEC 18.2) are ordinary warnings in `diagnostics`.
