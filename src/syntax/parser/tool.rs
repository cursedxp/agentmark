//! `# Tool: name` (SPEC 22.5): a tool written in AgentMark. An optional `- touches:` property
//! (SPEC 23.7) and one `## verb(params) -> type` block per verb; the block holds numbered steps or
//! a fenced `rust` block.

use super::*;

impl<'l> Builder<'l> {
    pub(super) fn tool(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
        blocks: Vec<(&'l Line, Vec<&'l Line>)>,
    ) -> Option<Decl> {
        let props = self.props(nodes, &["touches"], "a Tool");
        for p in &props {
            self.no_kids(p);
        }
        let touches = props
            .iter()
            .find(|p| p.key.name == "touches")
            .and_then(|p| self.touches(p));

        if blocks.is_empty() {
            let d = Diagnostic::error(
                E_MISSING_PROP,
                self.file,
                name.span,
                format!("The tool \"{}\" has no verbs.", name.name),
            )
            .with_help(
                "Add a block such as \"## slug(title: text) -> text\" with the steps of the verb under it.",
            );
            self.diags.push(d);
        }
        let mut verbs = Vec::new();
        for (h2, lines) in blocks {
            let Some((vname, params, out)) =
                self.parse(|g| &g.binding_item, &h2.text, h2.line_no, h2.col)
            else {
                continue;
            };
            let mut pos = 0;
            let kids = self.build(&lines, &mut pos, 0);
            let has_fence = kids.iter().any(|n| n.fence.is_some());
            let body = if has_fence {
                if let Some(extra) = kids.iter().find(|n| n.fence.is_none()) {
                    self.kid_error(
                        extra.line,
                        "A verb is either steps or one fenced rust block, not both.",
                    );
                }
                match self.fence_in(&kids, h2, &vname.name) {
                    Some(rust) => ToolBody::Rust(rust),
                    None => continue,
                }
            } else {
                ToolBody::Stmts(self.stmts(&kids))
            };
            verbs.push(ToolVerb {
                span: line_span(h2),
                doc: self.docs.get(&h2.line_no).cloned(),
                name: vname,
                params,
                out,
                body,
            });
        }
        Some(Decl::Tool(Tool {
            span,
            doc,
            prop_comments: prop_comments(&props),
            name,
            touches,
            verbs,
        }))
    }
}
