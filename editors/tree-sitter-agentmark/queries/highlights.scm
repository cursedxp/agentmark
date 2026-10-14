; Highlights for AgentMark (.agent.md). Capture names follow the common
; nvim-treesitter / Helix / Zed vocabulary.

; ---- comments -------------------------------------------------------------
(comment) @comment

; ---- headings ---------------------------------------------------------------
(declaration_kind) @keyword.type
(declaration name: (identifier) @type.definition)
(record_declaration "#" @punctuation.special)
(record_declaration name: (identifier) @type.definition)
(import_declaration "#" @punctuation.special)
(run_declaration "#" @punctuation.special)
(declaration "#" @punctuation.special)
"##" @punctuation.special

(record_declaration "record" @keyword.type)
(enum_declaration "#" @punctuation.special)
(enum_declaration "enum" @keyword.type)
(enum_declaration name: (identifier) @type.definition)
(import_declaration "import" @keyword.import)
(import_target "crate" @keyword.import)
(run_declaration "run" @keyword.type)
(import_target path: (import_path) @string.special.path)
(import_target crate: (identifier) @module)
(import_alias alias: (identifier) @module)
(import_alias "as" @keyword.import)

; Block (trigger) headings
(every_heading "every" @keyword)
(every_heading "day" @keyword)
(every_heading "at" @keyword)
(every_heading (weekday) @constant)
(on_start_heading ["on" "start"] @keyword)
(when_heading ["when" "receives"] @keyword)
(when_heading tool: (identifier) @function.builtin)
(when_heading event: (identifier) @constant)
(do_heading "do" @keyword)
(do_heading name: (identifier) @function)
(run_heading "run" @keyword)
(route_heading method: (http_method) @keyword)
(route_heading path: (route_path) @string.special.path)
(rust_heading "rust" @keyword)
(rust_heading name: (identifier) @function)
(binding_heading name: (identifier) @function)
(not_bound_block ["not" "bound"] @keyword)
(not_bound_item name: (identifier) @variable)
(not_bound_item reason: (reason) @comment)

(time) @number
(duration) @number.duration

; ---- properties and fields --------------------------------------------------
(property_key) @property
(record_field name: (identifier) @property)
(enum_variant name: (identifier) @constructor)
(run_item name: (identifier) @module)
(parameter name: (identifier) @variable.parameter)
(driver) @string.special
(address) @string.special

; ---- types ----------------------------------------------------------------
(type (identifier) @type)
((identifier) @type.builtin
  (#any-of? @type.builtin "text" "int" "money" "date" "datetime" "duration" "bool" "bytes"))
((identifier) @type
  (#match? @type "^[A-Z]"))

; ---- statements -------------------------------------------------------------
["for" "each" "in" "if" "else" "otherwise" "parallel" "try" "return"
 "set" "guard" "do" "read" "append" "to" "respond" "print" "with" "as" "where"
 "first" "count" "of" "match"] @keyword
(ask_statement ["ask" "model"] @keyword)
(no_amounts_changed ["no" "amounts" "changed" "in"] @keyword)
(predicate ["is" "not" "empty" "some" "matches"] @keyword.operator)
(length_expression "length" @keyword.operator)
(stop_statement) @keyword
(statement_line "-" @punctuation.list_marker)
(property "-" @punctuation.list_marker)
(record_field "-" @punctuation.list_marker)
(enum_variant "-" @punctuation.list_marker)
(run_item "-" @punctuation.list_marker)
(not_bound_item "-" @punctuation.list_marker)

; Tool calls: `- <tool> <verb> [target]: payload`
(tool_call tool: (identifier) @function.builtin)
(tool_call verb: (identifier) @function.method)
(do_statement (identifier) @function)
(call_expression function: (identifier) @function.call)
(for_each rule: (identifier) @function)
(for_each variable: (identifier) @variable)
(count_expression rule: (identifier) @function)
(first_expression item: (identifier) @variable)
(bind variable: (identifier) @variable)
(bind "->" @operator)
(ask_statement record: (type (identifier) @type))
(record_pair key: (identifier) @property)
(field_expression field: (identifier) @property)
(predicate binding: (identifier) @variable)

; enum variants: `Circle { radius: 3 }`, branches of a `match`
(variant_expression variant: (identifier) @constructor)
(variant_expression variant: (field_expression object: (identifier) @type field: (identifier) @constructor))
(match_arm pattern: (identifier) @constructor)
(match_arm pattern: (field_expression object: (identifier) @type field: (identifier) @constructor))
(match_arm binding: (identifier) @variable)
(some_expression "some" @keyword)
((identifier) @constant.builtin
  (#eq? @constant.builtin "none"))
((match_arm pattern: (identifier) @keyword)
  (#any-of? @keyword "some" "none" "ok" "error"))

; ---- expressions ------------------------------------------------------------
(string) @string
(string_content) @string
(escape_sequence) @string.escape
(interpolation ["{" "}"] @punctuation.special)
(interpolation) @embedded
(number) @number
(money) @number.money
(date) @number.date
(boolean) @boolean
(builtin) @variable.builtin
(unary_expression "not" @keyword.operator)
(binary_expression operator: _ @operator)
(unary_expression "-" @operator)
(parameter "=" @operator)
["(" ")" "{" "}"] @punctuation.bracket
["," ":"] @punctuation.delimiter

; ---- fenced Rust (Layer 1) --------------------------------------------------
(fence_open) @punctuation.special
(fence_close) @punctuation.special
(code_content) @none

; ---- specific overrides (after the generic lists above, priority raised so
; Neovim (last wins) and Helix (first wins) agree) ----------------------------
(type_phrase ["list" "maybe" "result" "of"] @type.builtin
  (#set! priority 110))
(binary_expression operator: ["and" "or"] @keyword.operator
  (#set! priority 110))

; handle types (SPEC 22.4): `mut handle of sha2.Sha256`
(handle_type) @type
