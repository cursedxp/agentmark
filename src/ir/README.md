# ir: what codegen reads

```rust
let l2  = agentmark::lower::agent_lower(&project, &checked);   // L3 -> L2 AST (Lowered)
let prog = agentmark::ir::build::build(&project, &checked, &l2); // or: agentmark::ir::lower(&project, &checked)
agentmark::ir::effect_summary(&prog)  // [(unit, label, EffectSet)], Display = SPEC 8 string
```

The IR has no agent, rule, trigger, `ask model` or guard node. Constants are inlined. Every `Stmt` and
`Expr` has an `effect: EffectSet` (from `Checked`; unioned from the children for nodes lowering made up).
Types are `sema::Type`; `Type::Unknown` means "untyped JSON value" (e.g. `http post … -> x`).
`SourceRef { file, line }` on every statement is for the `// <file>.agent.md:<line>` comments.
All names are the DECLARED names (functions, records, services, commands, `-` included); codegen maps them to Rust identifiers and dedups. Names lowering makes are `<agent>__<block>` (a rule that shares a name with a function becomes `<name>__rule`); a verb of a `# Tool` is the `Fn` `<tool>__<verb>` (`display` = `(tool, verb)`), and a Rust-bodied verb adds the `RustFn` `<tool>_<verb>_rust` that it calls (SPEC 22.5, Q380); `Fn.display` holds `(unit, label)` for a former `## Do` block so the effect summary does not show the internal name.

## Runtime context (not in the IR)
`Entry.unit`/`label` are the trace labels. Codegen calls `ctx.enter(unit, label)` once at the start of an
entry (route: service name + `Route.label`; command: name + `"Run"`); every `Fn` it calls runs under that
context, so a former `## Do` block reports the *entry's* label. `Trace`/model/tool lines read it from `ctx`.

## Program
- `Program` — everything: package, records, functions, entries, services, commands, rust_fns, bindings, crates, tools, run.
- `Package` — `agentmark.toml` name/version.
- `Record` / `RecordField` — a record with resolved (inferred) field types.
- `Fn` — `name, display, doc, params, ret, effect, body, source`; former `# Rule` (`Fn(rec) -> bool`, pure) and `## Do` (`<agent>__<block>`, takes the caller's record) are plain Fns.
- `Param` / `LocalVar` — parameter (with optional default) / variable a statement introduces.
- `Entry` — `unit, label, trigger, param, timeout, memory_file, body, effect`; stop = return from the body.
- `Trigger` — `Every(Duration) | DailyAt | Weekly | Event{tool,event} | Start`; `Duration {amount, unit}`.
- `Service` / `Route` — `name, listen, state record`; route has `method, path, path_params (text vars), label ("GET /x"), body`. `state` is a variable of the state record.
- `Command` — `name, args (with defaults), state (Q9: the project's service state record, variable `state`), body`; `source` = `# Command` heading, `run` = its `## Run` heading.
- `RustFn` — `## Rust` block: verbatim `code`, `code_line`, declared `effect`; `BindingFn` — one binding item, verbatim `code`, `effect` (`Unverified` until approved); `on` = the handle type when the item is a method (`ir::binding_key(item, on)` tells methods of different handles apart), `modes` = how each parameter holds a handle (`Shared` borrowed, `Mut`, `Consume` moved in; `Shared` for every non-handle).
- `CrateImport` — `# Import: crate x` (`binding` = binding name if a binding file exists; versions live in the binding header).
- `ToolDef` (re-export of `manifest::ToolDef`) — `tools.toml` entry, for one trait per tool.
- `run` — names under `# Run`, in order.

## Statements (`StmtKind`)
- `Let{name,ty,value}` declares; `Assign{name,ty,value}` changes an existing variable (`ty` = its declared type, wrap `T` into `maybe T` when `value.ty` differs).
- `If{cond,then,else_}`, `IfSome{expr,bind,then,else_}` (bind has the inner `T`), `ForEach{var,source,body}`.
- `Return(e)`; `Stop` = leave the enclosing Fn/entry/route/command (SPEC 4.1).
- `Effect{call, into}` — an effect; `into` declares a fresh variable (shadows) for its result.
- `Respond{status,body}`, `Append{value,list}` (list = path to a list, e.g. `state.invoices`), `Parallel(stmts)`.
- `Try{stmt, otherwise, defines}` — `defines` is the variable `stmt` introduces; it is visible after the try and `otherwise` may assign it (declare it before, assign in both arms). No `otherwise`: on failure `ctx.trace_error` and stop. If `otherwise` falls through without assigning/stopping, codegen uses the type's default (Q94). In a function whose `ret` is `Result(T)` a failing `try` without `otherwise` returns the error to the caller instead (Q200); `defines` of a `try do f()` on such a function is the plain `T`.

## Effect calls (`EffectCall`)
- `Tool(ToolCall{tool, verb, target, payload})` — manifest tool; nothing left to resolve.
- `Target::Field{word,var,field,value}` — `email send customer`: address = `var.field` (`customer_email` else `customer`); trace `target` = `word`.
- `Target::Source{word,source,record,id,id_from}` — `crm update invoice`: `source` is the plural (`invoices`), `id` the record id as an expression, `id_from` = `(var, field)` when it is a field of an in-scope record.
- `Target::Literal{word,value}` — `email send address "x"` (word `address`) and the URL of `http post "…"` (word None).
- `Payload::Assign(fields)` — `status = "x"` (trace `set` = these); `Payload::Value(e)` — text, record literal or record value.
- `Model(ModelCall{prompt, context, schema, model})` — `ask model`: interpolated prompt; `context` = the `with` value (trace `{"id": v.id}`); `schema` = `as Record` (parse + validate); `model` = agent driver string (`claude:haiku`), None outside agents.
- `Binding{binding,item,on,args}` — call of a `# Binding` item (`pdf render doc`); for a method call on a handle (`- hasher update data`) `on` is the handle type and `args[0]` is the handle variable (codegen lends it as `&h` / `&mut h` or moves it, by the item's `modes`); `Rust{name,args}` — `## Rust` block call; `Fn{name,args}` — function call statement (`do f(x) -> y`, former `do <action>`).
- `Io(ReadFile{path,ty} | ReadBody{ty} | ReadInput | Print{message})` — `ReadInput`: all of standard input as `text`.
- `File(FileCall{verb, path, arg})` — the built-in `file` tool (SPEC 22.3, Q344): `verb` is `read | write | append | list | rename | exists | delete`, `arg` the new path of `rename` or the content of `write`/`append`; `into` types: `read` text, `list` list of text, `exists` bool. Codegen emits `agentmark_rt::stdlib::file::<verb>(ctx, …)`, which traces `{"kind":"file",…}`.
- `Std(expr)` — a failing library call (`number.parse`, `csv.parse … as …`) whose value is stored by a `try`: `expr` is an `ExprKind::Std` whose `ty` is `Result(T)`; the statement's `into` has the plain `T`.
- `Trace(ToolCall{tool, verb, args})` — the first statement of every tool verb's `Fn`: `{"kind":"tool","tool":…,"verb":…,"args":{param: value}}` (Q385). A statement `- slugger slug title -> s` is an `Effect{Fn{"slugger__slug", args}}`; the expression `slugger.slug(title)` is `Call{Fn, "slugger__slug"}`.
- `Trace(Guard{name, ok})` — one `guard` trace line; `name` is the guard source with constants inlined (`reminder length < 800`), `ok` a `bool` expression. Every guard, in agents, services, commands and functions alike, is `Let guard_ok_<line> = cond; Effect(Trace); If(not tmp){else | Stop}` (Q90).

## Expressions (`ExprKind`, each with `ty` and `effect`)
- Literals `Str Int Money{minor,currency} Duration Date Bool Today Now`; `Var(name)`; `Field{base,field}`.
- `TextToBytes(inner)` — a `text` argument of a binding call whose parameter is `bytes` (Q151); codegen emits `inner.as_bytes().to_vec()`; `ty` is `bytes`.
- `Unwrap{inner,what}` — `inner` is `maybe T` known present (narrowing from `Checked.unwraps`); codegen emits `inner.expect("<what> is not empty here")` (no bare `.unwrap()`); the node's `ty` is `T`.
- `Interp(parts)`, `Binary{op,lhs,rhs}`, `Not`, `Record(fields)` (ty is `Object`).
- `Std{module, name, args, decode}` — a call of the built-in library (SPEC 22.3; the one table is `crate::stdlib`). `ty` is the type of the whole call (`Result(T)` for `number.parse`, `money.parse`, `date.parse`, `json.parse`, `csv.parse`); `decode` is `T` of `json.parse(t) as T` / `list of T` of `csv.parse`. Pure; codegen writes `agentmark_rt::stdlib::<module>::<name>(…)`.
- `Call{callee: Fn|Rust, name, args}`; `Source{tool,source,record}` (read `crm.invoices`, effect network).
- `Count{source, filter}` — `filter` is the name of a pure `Fn(rec) -> bool`; `First{var,source,cond}` -> `maybe T`.
- `IsEmpty`, `IsNotEmpty`, `Length`, `Matches{expr,record}`.
- `NoAmountsChanged{output,input}` — `input` = the `with` value (else prompt) of the model call that wrote `output`; re-evaluated at the guard (Q93).

## Functions
`build::build`, `ir::lower`, `ir::effect_summary` (entries, routes, commands, functions; a former `## Do` block shows as its `(unit, label)`), `ir::union_effects`.
