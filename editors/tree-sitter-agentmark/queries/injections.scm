; Fenced ```rust blocks hold verbatim Rust (Layer 1, SPEC 17).
((code_fence
   (fence_open) @_lang
   (code_content) @injection.content)
 (#match? @_lang "^`{3,}rust")
 (#set! injection.language "rust"))
