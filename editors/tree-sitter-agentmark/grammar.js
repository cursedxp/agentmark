/// <reference types="tree-sitter-cli/dsl" />
// tree-sitter grammar for AgentMark (.agent.md), SPEC sections 1-5 and 13-18.
//
// The grammar is line oriented. Indentation is not modelled (every statement
// line is a sibling inside its block); that is all highlighting needs, and it
// keeps the grammar free of an external scanner. See OPEN_QUESTIONS Q260.

/** Case-insensitive keyword (headings are case-insensitive, SPEC 1). */
function ci(word) {
  const re = word
    .split('')
    .map((c) => (/[a-z]/i.test(c) ? `[${c.toLowerCase()}${c.toUpperCase()}]` : c))
    .join('');
  return alias(token(prec(2, new RegExp(re))), word.toLowerCase());
}

/** `Tool`: `tool` is a prefix of the property key `tools`, so this keyword takes no lexical precedence and the longer word wins. */
function ciTool() {
  return alias(token(/[tT][oO][oO][lL]/), 'tool');
}

const PREC = {
  or: 1,
  and: 2,
  not: 3,
  compare: 4,
  add: 5,
  mul: 6,
  unary: 7,
  postfix: 8,
  field: 9,
};

// Words that start a collection expression; they stay usable as names.
const COLLECTION_WORDS = [
  'list', 'map', 'empty', 'sort', 'group', 'sum', 'min', 'max', 'last', 'take', 'skip',
  'unique', 'join', 'keys', 'values',
];

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

module.exports = grammar({
  name: 'agentmark',

  word: ($) => $.identifier,

  extras: () => [/[ \t\r]/],

  // A collection word is also a valid name (`values`, `last`, ...): the parser tries both.
  conflicts: ($) => [
    [$._atom, $.collection_expression],
    [$._expression, $.collection_expression],
    [$.type, $._atom],
    [$._atom, $.decode_expression],
  ],

  rules: {
    // Statement lines and blocks may appear before the first declaration so
    // that fragments (as shown in docs) still highlight.
    source_file: ($) =>
      seq(repeat($._body_line), repeat($._block), repeat($._declaration)),

    // ---- lines ----------------------------------------------------------

    _newline: () => token(choice('\n', '\0')),

    _filler: ($) => choice($._newline, seq($.comment, $._newline)),

    comment: () => token(seq('>', /[^\n]*/)),

    // ---- declarations ---------------------------------------------------

    _declaration: ($) =>
      choice(
        $.import_declaration,
        $.record_declaration,
        $.enum_declaration,
        $.run_declaration,
        $.declaration,
      ),

    import_declaration: ($) =>
      seq('#', ci('import'), ':', $.import_target, $._newline, repeat($._filler)),

    import_target: ($) =>
      choice(
        seq(ci_crate(), field('crate', $.identifier), optional($.import_alias)),
        seq(field('path', $.import_path), optional($.import_alias)),
      ),

    import_alias: ($) => seq('as', field('alias', $.identifier)),

    import_path: () => token(/\.\.?\/[^\s]*|\.\.?/),

    record_declaration: ($) =>
      seq(
        '#',
        ci('record'),
        ':',
        field('name', $.identifier),
        $._newline,
        repeat(choice($._filler, $.record_field)),
      ),

    record_field: ($) =>
      seq(
        '-',
        field('name', $.identifier),
        optional(seq(':', field('type', $.type))),
        $._newline,
      ),

    // `# Enum: Shape` and one `- Variant: field: type, ...` line per variant.
    enum_declaration: ($) =>
      seq(
        '#',
        ci('enum'),
        ':',
        field('name', $.identifier),
        $._newline,
        repeat(choice($._filler, $.enum_variant)),
      ),

    enum_variant: ($) =>
      seq(
        '-',
        field('name', $.identifier),
        optional(seq(':', commaSep1($.parameter))),
        $._newline,
      ),

    run_declaration: ($) =>
      seq('#', ci('run'), $._newline, repeat(choice($._filler, $.run_item))),

    run_item: ($) => seq('-', field('name', $.identifier), $._newline),

    declaration: ($) =>
      seq(
        '#',
        field('kind', $.declaration_kind),
        ':',
        field('name', $.identifier),
        $._newline,
        repeat($._body_line),
        repeat($._block),
      ),

    declaration_kind: () =>
      choice(
        ci('agent'),
        ci('rule'),
        ci('function'),
        ci('service'),
        ci('command'),
        ci('constant'),
        ci('binding'),
        ci('type'),
        ciTool(),
      ),

    // A line that may appear in a declaration body or a block.
    _body_line: ($) =>
      choice($._filler, $.property, $.statement_line, $.code_fence),

    // ---- blocks ---------------------------------------------------------

    _block: ($) =>
      choice($.trigger_block, $.code_block, $.not_bound_block),

    trigger_block: ($) =>
      seq($._trigger_heading, repeat($._block_line)),

    _block_line: ($) => choice($._filler, $.statement_line, $.code_fence),

    _trigger_heading: ($) =>
      choice(
        $.every_heading,
        $.on_start_heading,
        $.when_heading,
        $.do_heading,
        $.route_heading,
        $.run_heading,
      ),

    every_heading: ($) =>
      seq(
        '##',
        ci('every'),
        choice(
          $.duration,
          seq(ci('day'), ci('at'), $.time),
          seq($.weekday, ci('at'), $.time),
        ),
        $._newline,
      ),

    weekday: () =>
      choice(
        ci('monday'),
        ci('tuesday'),
        ci('wednesday'),
        ci('thursday'),
        ci('friday'),
        ci('saturday'),
        ci('sunday'),
      ),

    time: () => token(/\d{1,2}:\d{2}/),

    on_start_heading: ($) => seq('##', ci('on'), ci('start'), $._newline),

    when_heading: ($) =>
      seq(
        '##',
        ci('when'),
        field('tool', $.identifier),
        ci('receives'),
        field('event', $.identifier),
        $._newline,
      ),

    do_heading: ($) =>
      seq('##', ci('do'), field('name', $.identifier), $._newline),

    route_heading: ($) =>
      seq(
        '##',
        field('method', $.http_method),
        field('path', $.route_path),
        $._newline,
      ),

    http_method: () => token(prec(2, /GET|POST|PUT|PATCH|DELETE|HEAD/)),

    route_path: () => token(/\/[^\s]*/),

    run_heading: ($) => seq('##', ci('run'), $._newline),

    // `## Rust: name -> type` and `## name(params) -> type` (binding
    // functions) carry properties and/or a fenced Rust body.
    code_block: ($) =>
      seq(
        choice($.rust_heading, $.binding_heading, $.wrapped_heading, $.handle_heading),
        repeat($._body_line),
      ),

    rust_heading: ($) =>
      seq(
        '##',
        ci('rust'),
        ':',
        field('name', $.identifier),
        optional(seq('->', field('type', $.type))),
        $._newline,
      ),

    binding_heading: ($) =>
      seq(
        '##',
        field('name', $.identifier),
        '(',
        optional(commaSep1($.parameter)),
        ')',
        optional(seq('->', field('type', $.type))),
        $._newline,
      ),

    // `## execute` with `- in:` / `- out:` lines under it: a binding item whose one-line heading would be
    // longer than 120 characters (Q416)
    wrapped_heading: ($) => seq('##', field('name', $.identifier), $._newline),

    // `## Handle: sqlx.query.Query` gives the Rust type of a handle (SPEC 22.4, Q370)
    handle_heading: ($) =>
      seq('##', ci('handle'), ':', field('name', $.handle_name), $._newline),

    handle_name: () => token(/[A-Za-z_][A-Za-z0-9_-]*(\.[A-Za-z_][A-Za-z0-9_-]*)*/),

    not_bound_block: ($) =>
      seq(
        '##',
        ci('not'),
        ci('bound'),
        $._newline,
        repeat(choice($._filler, $.not_bound_item)),
      ),

    not_bound_item: ($) =>
      seq('-', field('name', $.identifier), ':', field('reason', $.reason), $._newline),

    reason: () => token(/[^\n]+/),

    // ---- fenced Rust code (Layer 1) --------------------------------------

    code_fence: ($) =>
      seq(
        $.fence_open,
        optional($.code_content),
        $.fence_close,
        $._newline,
      ),

    fence_open: () => token(seq('```', /[A-Za-z0-9_-]*/)),

    code_content: () => token(prec(-1, /([^`]|`{1,2}[^`])+/)),

    fence_close: () => '```',

    // ---- properties ------------------------------------------------------

    property: ($) =>
      seq(
        '-',
        field('key', $.property_key),
        ':',
        field('value', $.property_value),
        $._newline,
      ),

    property_key: () =>
      choice(
        'model',
        'tools',
        'memory',
        'timeout',
        'for',
        'when',
        'value',
        'in',
        'out',
        'effect',
        'args',
        'listen',
        'state',
        'crate',
        'proposed-effect',
        'is',
      ),

    property_value: ($) =>
      commaSep1(choice($.parameter, prec.dynamic(2, $.type_phrase), $._expression, $.driver, $.address)),

    parameter: ($) =>
      seq(
        // `first`, `count` and `no` start expressions, so they are keywords
        // where a property value begins; as a parameter name they are plain.
        field('name', choice($.identifier, alias(choice('first', 'count', 'no', ...COLLECTION_WORDS), $.identifier))),
        ':',
        field('type', $.type),
        optional(seq('=', field('default', $._expression))),
      ),

    driver: () => token(/[a-z][a-z0-9_-]*:[A-Za-z0-9_.\/-]+/),

    address: () => token(/\d+\.\d+\.\d+\.\d+:\d+/),

    // ---- types -----------------------------------------------------------

    type: ($) => choice($.type_phrase, $.identifier),

    type_phrase: ($) =>
      choice(
        seq('list', 'of', $.type),
        seq('maybe', $.type),
        seq('result', 'of', $.type),
        seq('map', 'of', $.type, 'to', $.type),
        $.handle_type,
      ),

    // `handle of sha2.Sha256`, `mut handle of …`, `consume handle of …` (SPEC 22.4). One token,
    // so `handle`, `mut` and `consume` stay plain identifiers everywhere else.
    handle_type: () =>
      token(/((mut|consume)[ \t]+)?handle[ \t]+of[ \t]+[A-Za-z_][A-Za-z0-9_-]*(\.[A-Za-z_][A-Za-z0-9_-]*)*/),

    // ---- statements ------------------------------------------------------

    // Without a leading dash only control lines are statements, so stray
    // prose is an ERROR rather than a tool call.
    statement_line: ($) =>
      seq(
        choice(
          seq('-', $._statement),
          choice(
            $.for_each,
            $.if_statement,
            $.else_statement,
            $.otherwise_statement,
            $.parallel_statement,
            $.match_statement,
            $.try_statement,
          ),
        ),
        $._newline,
      ),

    _statement: ($) =>
      choice(
        $.for_each,
        $.if_statement,
        $.else_statement,
        $.otherwise_statement,
        $.parallel_statement,
        $.match_statement,
        $.match_arm,
        $.try_statement,
        $._simple,
      ),

    _simple: ($) =>
      choice(
        $.stop_statement,
        $.set_statement,
        $.return_statement,
        $.guard_statement,
        $.ask_statement,
        $.do_statement,
        $.read_statement,
        $.append_statement,
        $.put_statement,
        $.remove_statement,
        $.respond_statement,
        $.print_statement,
        $.call_statement,
        $.tool_call,
      ),

    for_each: ($) =>
      seq(
        'for',
        'each',
        choice(
          field('variable', $.identifier),
          seq(field('rule', $.identifier), field('variable', $.identifier)),
          // `for each region, total in totals:` (SPEC 22.2)
          seq(field('variable', $.identifier), ',', field('value', $.identifier)),
        ),
        'in',
        field('source', $._expression),
        // `for each row in rows with index i:`
        optional(seq('with', 'index', field('index', $.identifier))),
        ':',
      ),

    if_statement: ($) => seq('if', field('condition', $._expression), ':'),

    else_statement: () => seq('else', ':'),

    otherwise_statement: () => seq('otherwise', ':'),

    parallel_statement: () => seq('parallel', ':'),

    // `match shape:` and its branches `Circle c:`, `Shape.Empty:`, `some row:`, `none:`.
    // (`otherwise:` is the statement above.)
    match_statement: ($) => seq('match', field('subject', $._expression), ':'),

    match_arm: ($) =>
      seq(
        field('pattern', choice($.identifier, $.field_expression)),
        optional(field('binding', $.identifier)),
        ':',
      ),

    try_statement: ($) => seq('try', choice($._simple, $.bare_call)),

    // `try f(x) -> v`: a call of a function that can fail, without `do`
    bare_call: ($) => seq($.call_expression, optional($.bind)),

    stop_statement: () => 'stop',

    set_statement: ($) =>
      seq('set', field('target', $._atom), '=', field('value', $._expression)),

    return_statement: ($) => seq('return', $._expression),

    guard_statement: ($) =>
      seq(
        'guard',
        ':',
        field('condition', $._expression),
        optional(seq('else', field('otherwise', $._simple))),
      ),

    ask_statement: ($) =>
      seq(
        'ask',
        'model',
        optional(seq('with', field('context', $._expression))),
        ':',
        field('prompt', $._expression),
        optional(seq('as', field('record', $.type))),
        optional($.bind),
      ),

    do_statement: ($) =>
      seq(
        'do',
        repeat1($._atom),
        optional(seq('as', field('type', $.type))),
        optional(seq(':', commaSep1($._expression))),
        optional($.bind),
      ),

    // `read file p as T -> x`, `read body as T -> x`, `read input -> x`
    read_statement: ($) =>
      seq(
        'read',
        repeat1($._atom),
        optional(seq('as', field('type', $.type))),
        optional($.bind),
      ),

    // `csv.parse(content) as list of Sale -> sales` (after `try` or `do`, SPEC 22.3)
    call_statement: ($) =>
      seq(
        choice($.module_call, $.decode_expression),
        $.bind,
      ),

    append_statement: ($) =>
      seq('append', $._expression, 'to', field('target', $._expression)),

    put_statement: ($) =>
      seq('put', $._expression, 'at', $._expression, 'in', field('target', $._expression)),

    remove_statement: ($) =>
      seq('remove', $._expression, 'from', field('target', $._expression)),

    respond_statement: ($) =>
      seq('respond', field('status', $.number), optional(field('body', $._atom))),

    print_statement: ($) => seq('print', ':', commaSep1($._expression)),

    tool_call: ($) =>
      seq(
        field('tool', $.identifier),
        optional(seq(field('verb', $.identifier), repeat($._atom))),
        optional(seq('to', field('destination', $._atom))),
        optional(seq(':', field('payload', commaSep1($._expression)))),
        optional($.bind),
      ),

    bind: ($) => seq('->', field('variable', $.identifier)),

    // ---- expressions -----------------------------------------------------

    _atom: ($) =>
      choice(
        $.identifier,
        // `first`, `count`, `no` are only keywords when followed by the rest
        // of their phrase; otherwise they are ordinary variable names.
        alias(choice('first', 'count', 'no', ...COLLECTION_WORDS), $.identifier),
        $.string,
        $.number,
        $.money,
        $.duration,
        $.date,
        $.boolean,
        $.builtin,
        $.field_expression,
        $.call_expression,
        $.variant_expression,
        $.some_expression,
        $.module_call,
        $.record_literal,
      ),

    _expression: ($) =>
      choice(
        $._atom,
        $.parenthesized_expression,
        $.binary_expression,
        $.unary_expression,
        $.predicate,
        $.length_expression,
        $.decode_expression,
        $.count_expression,
        $.first_expression,
        $.no_amounts_changed,
        $.collection_expression,
      ),

    parenthesized_expression: ($) => seq('(', $._expression, ')'),

    binary_expression: ($) => {
      const table = [
        [PREC.or, 'or'],
        [PREC.and, 'and'],
        [PREC.compare, choice('=', '!=', '<=', '>=', '<', '>')],
        [PREC.add, choice('+', '-')],
        [PREC.mul, choice('*', '/')],
      ];
      return choice(
        ...table.map(([p, op]) =>
          prec.left(
            p,
            seq(
              field('left', $._expression),
              field('operator', op),
              field('right', $._expression),
            ),
          ),
        ),
      );
    },

    unary_expression: ($) =>
      choice(
        prec(PREC.not, seq('not', $._expression)),
        prec(PREC.unary, seq('-', $._expression)),
      ),

    predicate: ($) =>
      prec.left(
        PREC.compare,
        choice(
          seq($._expression, 'is', 'not', 'empty'),
          seq($._expression, 'is', 'empty'),
          seq($._expression, 'is', 'some', field('binding', $.identifier)),
          seq($._expression, 'matches', field('record', $.identifier)),
        ),
      ),

    length_expression: ($) => prec(PREC.postfix, seq($._atom, 'length')),

    count_expression: ($) =>
      prec.right(
        choice(
          seq(
            'count',
            'of',
            field('rule', $.identifier),
            field('item', $.identifier),
            'in',
            field('source', $._atom),
          ),
          seq('count', 'of', field('source', $._atom)),
        ),
      ),

    first_expression: ($) =>
      prec.right(
        seq(
          'first',
          field('item', $.identifier),
          'in',
          field('source', $._atom),
          'where',
          field('condition', $._expression),
        ),
      ),

    // What a collection word is applied to: a name, a call or a parenthesis (Q321).
    _source: ($) => choice($._atom, $.parenthesized_expression),

    // Collection words (SPEC 22.2). The grammar only needs the words and the places of the
    // expressions around them; the compiler checks the rest.
    collection_expression: ($) =>
      choice(
        prec.right(seq('list', 'of', commaSep1($._expression))),
        seq('empty', 'list', 'of', $.type),
        seq('empty', 'map', 'of', $.type, 'to', $.type),
        prec.right(seq('map', 'of', commaSep1(seq($._expression, 'to', $._expression)))),
        prec.right(seq('map', field('source', $._source), 'to', $._expression)),
        prec.right(seq(
          'sort', field('source', $._source), 'by', $._expression, optional('descending'),
        )),
        prec.right(seq('group', field('source', $._source), 'by', $._expression)),
        prec.right(seq(
          choice('sum', 'min', 'max'), 'of', $._expression,
          optional(seq('in', field('source', $._source))),
        )),
        prec.right(seq('count', 'of', field('source', $._source), 'where', $._expression)),
        seq(choice('first', 'last'), 'of', field('source', $._source)),
        seq(choice('take', 'skip'), $._atom, 'from', field('source', $._source)),
        prec.right(seq(
          'unique', $._expression, optional(seq('in', field('source', $._source))),
        )),
        seq('join', field('source', $._source), 'with', $._atom),
        seq(choice('keys', 'values'), 'of', field('source', $._source)),
        // `rows where cond` and `totals at "EU"`
        prec.right(PREC.or - 1, seq(field('source', $._atom), 'where', $._expression)),
        prec.dynamic(-1, seq(field('source', $._atom), 'at', $._atom)),
      ),

    no_amounts_changed: ($) =>
      seq('no', 'amounts', 'changed', 'in', field('variable', $.identifier)),

    field_expression: ($) =>
      prec.left(
        PREC.field,
        seq(
          field('object', choice($.identifier, $.field_expression, $.call_expression)),
          '.',
          field('field', $.identifier),
        ),
      ),

    call_expression: ($) =>
      seq(
        field('function', $.identifier),
        '(',
        optional(commaSep1($._expression)),
        ')',
      ),

    some_expression: ($) => prec.right(seq('some', $._atom)),

    // `Circle { radius: 3 }`
    variant_expression: ($) =>
      prec(
        1,
        seq(
          field('variant', choice($.identifier, $.field_expression)),
          $.record_literal,
        ),
      ),

    // `text.split(t, ",")`: a function of the built-in library (SPEC 22.3)
    module_call: ($) =>
      seq(
        field('module', $.identifier),
        '.',
        field('function', $.identifier),
        '(',
        optional(commaSep1($._expression)),
        ')',
      ),

    // `json.parse(t) as Note`, `csv.parse(t) as list of Sale`
    decode_expression: ($) =>
      prec.dynamic(1, seq($.module_call, 'as', field('type', $.type))),

    record_literal: ($) =>
      seq('{', optional(commaSep1($.record_pair)), '}'),

    record_pair: ($) =>
      seq(field('key', $.identifier), ':', field('value', $._expression)),

    builtin: () => choice('today', 'now'),

    boolean: () => choice('true', 'false'),

    // ---- literals --------------------------------------------------------

    number: () => token(/\d+(\.\d+)?/),

    money: () => token(prec(2, /\d+(\.\d+)?[ ]+[A-Z]{3}/)),

    duration: () =>
      token(prec(2, /\d+[ ]+(second|minute|hour|day|week|month|year)s?/)),

    date: () => token(prec(3, /\d{4}-\d{2}-\d{2}/)),

    identifier: () => /[A-Za-z_][A-Za-z0-9_]*(-[A-Za-z0-9_]+)*/,

    string: ($) =>
      seq(
        '"',
        repeat(choice($.interpolation, $.escape_sequence, $.string_content)),
        '"',
      ),

    string_content: () => token.immediate(prec(1, /[^"{\\\n]+/)),

    escape_sequence: () => token.immediate(/\\./),

    interpolation: ($) =>
      seq(token.immediate('{'), $._expression, '}'),
  },
});

function ci_crate() {
  return 'crate';
}
