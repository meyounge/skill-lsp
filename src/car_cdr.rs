//! Computed car/cdr selectors, independent of the user's Finder library.

/// Use the user's keyword2 blue, taking precedence over Finder categories.
pub const TOKEN_TYPE: u32 = 2;

fn operations(name: &str) -> Option<&[u8]> {
    let ops = name.as_bytes().strip_prefix(b"c")?.strip_suffix(b"r")?;
    (!ops.is_empty() && ops.iter().all(|b| matches!(b, b'a' | b'd'))).then_some(ops)
}

pub fn is_accessor(name: &str) -> bool {
    operations(name).is_some()
}

/// One-based nested element indices; a final range selects the remaining tail.
fn path(ops: &[u8]) -> String {
    let mut path = String::from("lst");
    let mut skipped = 0;
    for op in ops.iter().rev() {
        if *op == b'd' {
            skipped += 1;
        } else {
            path.push_str(&format!("[{}]", skipped + 1));
            skipped = 0;
        }
    }
    if skipped > 0 {
        path.push_str(&format!("[{}..]", skipped + 1));
    }
    path
}

/// Build the minimum cons structure needed to reach x. Both construction and
/// rendering are iterative and linear, even for very long composed names.
fn diagram(ops: &[u8]) -> String {
    enum Node {
        Selected,
        Unknown,
        Pair(usize, usize),
    }
    let mut nodes = vec![Node::Selected, Node::Unknown];
    let mut selected = 0;
    // Reconstruct the input by wrapping the result, from outermost to innermost.
    for op in ops {
        nodes.push(if *op == b'a' {
            Node::Pair(selected, 1)
        } else {
            Node::Pair(1, selected)
        });
        selected = nodes.len() - 1;
    }
    enum Part {
        Value(usize),
        Tail(usize),
    }
    let mut output = String::new();
    let mut stack = vec![Part::Value(selected)];
    while let Some(part) = stack.pop() {
        match part {
            Part::Value(i) => match nodes[i] {
                Node::Selected => output.push('x'),
                Node::Unknown => output.push('-'),
                Node::Pair(head, tail) => {
                    output.push('(');
                    stack.push(Part::Tail(tail));
                    stack.push(Part::Value(head));
                }
            },
            Part::Tail(i) => match nodes[i] {
                Node::Unknown => output.push_str(" -)"),
                Node::Selected => output.push_str(" . x)"),
                Node::Pair(head, tail) => {
                    output.push(' ');
                    stack.push(Part::Tail(tail));
                    stack.push(Part::Value(head));
                }
            },
        }
    }
    output
}

pub fn hover(name: &str) -> Option<String> {
    let ops = operations(name)?;
    let mut expanded = String::new();
    for op in ops {
        expanded.push_str(if *op == b'a' { "car(" } else { "cdr(" });
    }
    expanded.push_str("lst");
    expanded.push_str(&")".repeat(ops.len()));
    let steps = ops
        .iter()
        .rev()
        .map(|op| if *op == b'a' { "car" } else { "cdr" })
        .collect::<Vec<_>>()
        .join(" -> ");
    Some(format!(
        "**{name}** — car/cdr selector\n\n\
         ```text\n{}\n```\n\n\
         x = selected value; - = unspecified data or remaining tail, not a fixed list length.\n\n\
         Path (1-based): `{}`\n\n\
         Expansion: `{expanded}`\n\n\
         Apply right to left: {steps}.\n\n\
         For a tail selector, x is the returned tail; `. x` marks that tail.",
        diagram(ops),
        path(ops)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_complete_lowercase_selectors() {
        for name in ["car", "cdr", "caar", "cadr", "cadar", "caadar", "cddddr"] {
            assert!(is_accessor(name), "{name}");
        }
        for name in [
            "cr",
            "ca",
            "carpet",
            "caddrExtra",
            "Cadr",
            "caDr",
            "cadr?",
            "cadr_",
            "cadr1",
            "cαr",
        ] {
            assert!(!is_accessor(name), "{name}");
        }
    }

    #[test]
    fn paths_and_shapes_follow_right_to_left_composition() {
        for (name, expected_path, shape) in [
            ("car", "lst[1]", "(x -)"),
            ("cdr", "lst[2..]", "(- . x)"),
            ("cadr", "lst[2]", "(- x -)"),
            ("caar", "lst[1][1]", "((x -) -)"),
            ("cadar", "lst[1][2]", "((- x -) -)"),
            ("caadar", "lst[1][2][1]", "((- (x -) -) -)"),
            ("caddr", "lst[3]", "(- - x -)"),
            ("cdar", "lst[1][2..]", "((- . x) -)"),
            ("cddr", "lst[3..]", "(- - . x)"),
        ] {
            let ops = operations(name).unwrap();
            assert_eq!(path(ops), expected_path, "{name}");
            assert_eq!(diagram(ops), shape, "{name}");
            assert_eq!(shape.matches('x').count(), 1);
        }
        assert!(hover("caadar").unwrap().contains("car(car(cdr(car(lst))))"));
    }

    #[test]
    fn arbitrary_depth_does_not_recurse_or_expand_exponentially() {
        let name = format!("c{}r", "ad".repeat(5_000));
        let value = hover(&name).unwrap();
        assert!(value.len() < name.len() * 40);
    }
}
