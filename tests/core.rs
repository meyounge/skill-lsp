use skill_lsp::{
    document::{Document, Kind},
    fnd::{self, Database},
};
use tower_lsp::lsp_types::{FormattingOptions, Position};
fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/SKILL")
}
fn doc(text: &str) -> Document {
    Document::new(text.into(), 1)
}
#[test]
fn finder_escaping_and_parentheses() {
    let s = r#"("demo" "demo(
    t_name
    )
    => t_result" "A \"quote\" and (parentheses).")"#;
    let e = fnd::parse_fnd(s, "synthetic.fnd").unwrap();
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].description, "A \"quote\" and (parentheses).");
    assert_eq!(e[0].return_type.as_deref(), Some("string (result)"));
    assert_eq!(e[0].params[0].type_str, "string");
}
#[test]
fn keywords_merge_on_one_line_and_across_lines() {
    let (p, r) = fnd::parse_signature(
        "demo(\n d_cell\n [ ?width x_width ]\n [\n ?purpose\n t_purpose\n ]\n)\n=> t / nil",
    );
    assert!(r.is_none());
    assert_eq!(p.len(), 3);
    assert_eq!(p[1].name, "?width");
    assert_eq!(p[1].type_str, "integer");
    assert!(p[1].optional);
    assert_eq!(p[2].name, "?purpose");
    assert_eq!(p[2].type_str, "string");
    assert!(p[2].optional);
    assert_eq!(fnd::resolve_prefix("txf"), "string | integer | float");
}
#[test]
fn nested_optional_brackets() {
    let (p, _) =
        fnd::parse_signature("demo(\n [\n t_first\n [ x_second ]\n g_third\n ]\n l_required\n)");
    assert_eq!(
        p.iter().map(|p| p.optional).collect::<Vec<_>>(),
        vec![true, true, true, false]
    );
}
#[test]
fn malformed_finder_matches_python_tolerance() {
    for text in ["(\"demo\" \"unfinished", ")", "/* unclosed"] {
        assert!(fnd::parse_fnd(text, "bad.fnd").unwrap().is_empty());
    }
}
#[test]
fn two_pass_categories_duplicates_and_prefix_index() {
    let db = Database::new(fnd::load_dir(&fixture()).unwrap());
    assert_eq!(db.get("synthetic").unwrap().keyword_category, 1);
    assert_eq!(db.get("sampleHead").unwrap().keyword_category, 2);
    assert_eq!(db.get("dbSynthetic").unwrap().keyword_category, 3);
    assert_eq!(db.get("layoutSynthetic").unwrap().keyword_category, 4);
    assert_eq!(db.get("strExternal").unwrap().keyword_category, 2);
    assert_eq!(db.get("synthetic").unwrap().params[0].name, "value");
    assert_eq!(db.prefix("STR").count(), 2);
    assert_eq!(db.prefix("missing").count(), 0);
    // A second load has no leaked global prefix state.
    assert_eq!(fnd::load_dir(&fixture()).unwrap(), db.entries);
}
#[test]
fn both_function_syntaxes_and_variable_forms() {
    let d = doc("procedure(alpha(x) x)\n(defun beta (y) y)\n(setq result 2)\ncount = 3\n");
    assert!(d.symbol("alpha").unwrap().function);
    assert!(d.symbol("beta").unwrap().function);
    assert!(!d.symbol("result").unwrap().function);
    assert!(!d.symbol("count").unwrap().function);
    assert_eq!(d.definition("alpha").unwrap().start, Position::new(0, 10));
    assert_eq!(d.definition("beta").unwrap().start, Position::new(1, 7));
    assert!(d.diagnostics.is_empty());
}
#[test]
fn comments_strings_dont_define_or_reference_symbols() {
    let d=doc("procedure(real(x) x)\n; procedure(fake() real)\n/* (real)\n procedure(other()) */\n\"real procedure(hidden())\"\nreal(2)");
    assert!(d.symbol("fake").is_none());
    assert!(d.symbol("other").is_none());
    assert!(d.symbol("hidden").is_none());
    assert_eq!(d.occurrences["real"].len(), 2);
    assert!(d.diagnostics.is_empty());
}
#[test]
fn unicode_positions_and_crlf() {
    let d = doc("\"😀\" myFn()\r\nprocedure(myFn() t)");
    let offset = d.text.find("myFn").unwrap();
    assert_eq!(d.position(offset), Position::new(0, 5));
    assert_eq!(d.offset(Position::new(0, 5)), offset);
    assert_eq!(d.value(d.token_at(Position::new(0, 5)).unwrap()), "myFn");
    assert_eq!(d.definition("myFn").unwrap().start, Position::new(1, 10));
    assert!(d.diagnostics.is_empty());
}
#[test]
fn parentheses_inside_literals_and_multiline_comments() {
    assert!(doc("/* (\n ) */\n\"escaped \\\" )\"\n; )\n()")
        .diagnostics
        .is_empty());
    assert_eq!(doc("(\n)").diagnostics.len(), 0);
    assert_eq!(doc(")\n(").diagnostics.len(), 2);
    assert_eq!(doc("\"unfinished").diagnostics.len(), 1);
    assert_eq!(doc("/* unfinished").diagnostics.len(), 1);
}
#[test]
fn signatures_both_styles_nested_and_multiline() {
    for source in ["dbSynthetic(\n cell\n name ", "(dbSynthetic\n cell\n name "] {
        let d = doc(source);
        let call = d.call_at(d.position(source.len())).unwrap();
        assert_eq!(call.name, "dbSynthetic");
        assert_eq!(call.active, 2);
    }
    let source = "dbSynthetic(nested(1 2) name ";
    let d = doc(source);
    let call = d.call_at(d.position(source.len())).unwrap();
    assert_eq!(call.name, "dbSynthetic");
    assert_eq!(call.active, 2);
    let source = "dbSynthetic(cell name ?width 3";
    let d = doc(source);
    assert_eq!(
        d.call_at(d.position(source.len()))
            .unwrap()
            .keyword
            .as_deref(),
        Some("?width")
    );
    let source = "dbSynthetic(cell, name, ";
    let d = doc(source);
    assert_eq!(d.call_at(d.position(source.len())).unwrap().active, 2);
}
#[test]
fn semantic_legend_categories_utf16_and_multiline_segments() {
    let db = Database::new(fnd::load_dir(&fixture()).unwrap());
    let d = doc("\"😀\" dbSynthetic(nil)\n/* a\n b */\n?width 2");
    let tokens = d.semantic_tokens(&db);
    assert_eq!(tokens[0].token_type, 14);
    assert_eq!(tokens[0].length, 4);
    assert_eq!(tokens[1].token_type, 3);
    assert_eq!(tokens[1].delta_start, 5);
    assert_eq!(tokens.iter().filter(|t| t.token_type == 13).count(), 2);
    assert!(tokens.iter().any(|t| t.token_type == 17));
    assert!(tokens.iter().any(|t| t.token_type == 15));
}
#[test]
fn formatter_is_idempotent_preserves_multiline_content_and_line_endings() {
    let text = "procedure(f()\r\n dbSynthetic(\r\n nil\r\n )\r\n)\r\n";
    let options = FormattingOptions {
        tab_size: 2,
        insert_spaces: true,
        ..Default::default()
    };
    let formatted = doc(text).format(&options);
    assert_eq!(
        formatted,
        "procedure(f()\r\n  dbSynthetic(\r\n    nil\r\n  )\r\n)\r\n"
    );
    assert_eq!(doc(&formatted).format(&options), formatted);
    let text = "f(\n \"first\n  keep these spaces\nend\"\n/* a\n  exact */\n)";
    let formatted = doc(text).format(&options);
    assert!(formatted.contains("\n  keep these spaces\nend\""));
    assert!(formatted.contains("\n  exact */"));
    assert!(!formatted.ends_with('\n'));
}
#[test]
fn number_scanner_does_not_swallow_operators() {
    let d = doc("1-2 1e-2 0xFF .25");
    let values: Vec<_> = d
        .tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.kind == Kind::Number)
        .map(|(i, _)| d.value(i))
        .collect();
    assert_eq!(values, vec!["1", "-2", "1e-2", "0xFF", ".25"]);
}
