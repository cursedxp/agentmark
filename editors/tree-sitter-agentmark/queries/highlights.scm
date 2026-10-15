; Highlights for AgentMark (.agent.md). Capture names follow the common
; nvim-treesitter / Helix / Zed vocabulary.

; ---- comments -------------------------------------------------------------
(comment) @comment

; ---- headings ---------------------------------------------------------------
(declaration_kind) @keyword.type
(declaration name: (identifier) @type.definition)
(program_declaration "#" @punctuation.special)
(program_declaration "program" @keyword.type)
(program_declaration name: (identifier) @type.definition)
(uses_declaration "#" @punctuation.special)
(uses_declaration "uses" @keyword.type)
(uses_item alias: (identifier) @module)
(uses_item "as" @keyword.import)
(link) @markup.link.url
(rule_declaration "#" @punctuation.special)
(rule_declaration "rule" @keyword.type)
(rule_declaration name: (identifier) @type.definition)
(rule_declaration record: (identifier) @type)
(binding_declaration "#" @punctuation.special)
(binding_declaration "binding" @keyword.type)
(binding_declaration name: (identifier) @type.definition)
(record_declaration "#" @punctuation.special)
(record_declaration name: (identifier) @type.definition)
(import_declaration "#" @punctuation.special)
(declaration "#" @punctuation.special)
"##" @punctuation.special

(record_declaration "record" @keyword.type)
(enum_declaration "#" @punctuation.special)
(enum_declaration "enum" @keyword.type)
(enum_declaration name: (identifier) @type.definition)
(import_declaration "import" @keyword.import)
(import_declaration "crate" @keyword.import)
(import_declaration crate: (identifier) @module)
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
(route_heading method: (http_method) @keyword)
(route_heading path: (route_path) @string.special.path)
(rust_heading "rust" @keyword)
(rust_heading name: (identifier) @function)
(binding_heading name: (identifier) @function)
(handle_heading "handle" @keyword)
(handle_heading name: (handle_name) @type)
(not_bound_block ["not" "bound"] @keyword)
(not_bound_item name: (identifier) @variable)
(not_bound_item reason: (reason) @comment)

(time) @number
(duration) @number.duration

; ---- properties and fields --------------------------------------------------
(property_key) @property
(binding_property "-" @punctuation.list_marker)
(field_bullet name: (identifier) @property)
(name_phrase (identifier) @type)
(record_field name: (identifier) @property)
(enum_variant name: (identifier) @constructor)
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
 "guard" "do" "read" "to" "respond" "print" "with" "as" "where"
 "first" "count" "of" "match"] @keyword
; collection words and statements (SPEC 22.2)
(collection_expression
  ["list" "map" "empty" "sort" "group" "sum" "min" "max" "last" "take" "skip" "unique" "join"
   "keys" "values" "by" "descending" "at" "from" "in" "to" "of" "where" "with"] @keyword)
(put_statement ["put" "at" "in"] @keyword)
(remove_statement ["remove" "from"] @keyword)
(for_each ["with" "index"] @keyword)
(ask_statement ["ask" "model"] @keyword)
(no_amounts_changed ["no" "amounts" "changed" "in"] @keyword)
(predicate ["is" "not" "empty" "some" "matches"] @keyword.operator)
(length_expression "length" @keyword.operator)
(stop_statement) @keyword
(step_marker) @punctuation.list_marker
(statement_line ["-" "*" "+"] @punctuation.list_marker)
(property ["-" "*" "+"] @punctuation.list_marker)
(record_field ["-" "*" "+"] @punctuation.list_marker)
(enum_variant ["-" "*" "+"] @punctuation.list_marker)
(uses_item ["-" "*" "+"] @punctuation.list_marker)
(not_bound_item ["-" "*" "+"] @punctuation.list_marker)
(add_statement "add" @keyword)

; Tool calls: `- <tool> <verb> [target]: payload`
(tool_call tool: (identifier) @function.builtin)
(tool_call verb: (identifier) @function.method)
(do_statement (identifier) @function)
(call_expression function: (identifier) @function.call)
; the built-in library: `text.split(t, ",")`, `json.parse(t) as Note`
(module_call module: (identifier) @module.builtin)
(module_call function: (identifier) @function.builtin)
(decode_expression "as" @keyword)
(for_each rule: (identifier) @function)
(for_each variable: (identifier) @variable)
(for_each value: (identifier) @variable)
(for_each index: (identifier) @variable)
(count_expression rule: (identifier) @function)
(first_expression item: (identifier) @variable)
(bind variable: (identifier) @variable)
(bind "into" @keyword)
(bind type: (type (identifier) @type))
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
(type_phrase ["list" "maybe" "result" "of" "map" "to"] @type.builtin
  (#set! priority 110))
(binary_expression operator: ["and" "or"] @keyword.operator
  (#set! priority 110))

; handle types (SPEC 22.4): `mut handle of sha2.Sha256`
(handle_type) @type
