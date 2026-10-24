//! Regression cases for Python behavior deliberately preserved by the runtime.
use skill_lsp::{
    compat::Document,
    fnd::{self, Database},
};
use tower_lsp::lsp_types::{FormattingOptions, Position};

fn database() -> Database {
    Database::new(
        fnd::load_dir(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/SKILL"))
            .unwrap(),
    )
}

#[test]
fn declaration_ranges_and_raw_text_symbols_match_python() {
    let d = Document::new(
        "  procedure(myFn(x) x)\n; procedure(fake() nil)\nsetq result 1\n(defun ignored (x) x)"
            .into(),
        &database(),
    );
    assert_eq!(d.symbol("myFn").unwrap().range.start, Position::new(0, 2));
    assert!(d.symbol("fake").is_some());
    assert!(d.symbol("ignored").is_none());
    let names: Vec<_> = d.document_symbols().into_iter().map(|s| s.name).collect();
    assert_eq!(names, vec!["myFn", "fake"]);
}

#[test]
fn references_keep_comments_and_python_punctuation_boundaries() {
    let d = Document::new(
        "procedure(foo() nil)\n; foo\n\"foo\" foo* ?foo foo?bar".into(),
        &database(),
    );
    assert_eq!(d.references("foo").len(), 6);
    assert!(d.references("foo*").is_empty());
    assert!(d.references("?foo").is_empty());
    assert_eq!(d.references("foo?bar").len(), 1);
}

#[test]
fn signature_help_retains_line_and_call_style_limitations() {
    let db = database();
    let d = Document::new(
        "dbSynthetic(nil, t)\n(dbSynthetic nil, t\n(dbSynthetic\n nil".into(),
        &db,
    );
    assert_eq!(d.call(Position::new(0, 17)).unwrap().0, "nil");
    assert_eq!(
        d.call(Position::new(1, 19)).unwrap(),
        ("dbSynthetic".into(), 1)
    );
    assert!(d.call(Position::new(3, 4)).is_none());
}

#[test]
fn formatting_and_columns_retain_python_behavior() {
    let d = Document::new("\"😀\" dbSynthetic\r\n f(\r\n x\r\n )".into(), &database());
    assert_eq!(d.word(Position::new(0, 5)), "dbSynthetic");
    let edits = d.format(&FormattingOptions {
        tab_size: 2,
        insert_spaces: false,
        ..Default::default()
    });
    assert_eq!(edits[0].new_text, "\"😀\" dbSynthetic\nf(\n  x\n)\n");
    assert_eq!(edits[0].range.end, Position::new(4, 0));
}

#[test]
fn api_precedence_and_line_comments_match_python_semantic_tokens() {
    let d = Document::new(
        "procedure(dbSynthetic() nil)\ndbSynthetic(nil) ; dbSynthetic\n?width 2".into(),
        &database(),
    );
    assert_eq!(d.semantic.iter().filter(|t| t.token_type == 3).count(), 2);
    assert!(!d.semantic.iter().any(|t| t.token_type == 13));
    assert!(d.semantic.iter().any(|t| t.token_type == 17));
    assert!(d.diagnostics.is_empty());
}
