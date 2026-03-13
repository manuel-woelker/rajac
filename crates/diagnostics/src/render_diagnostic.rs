use crate::diagnostic::Diagnostic;
use crate::severity::Severity;
use rajac_base::shared_string::SharedString;

#[allow(unused_imports)]
use crate::source_chunk::SourceChunk;

pub fn render_diagnostic(diagnostic: &Diagnostic) -> SharedString {
    use annotate_snippets::{
        AnnotationKind, Group, Level, Renderer, Snippet, renderer::DecorStyle,
    };

    let level = match diagnostic.severity {
        Severity::Error => Level::ERROR,
        Severity::Warning => Level::WARNING,
        Severity::Note => Level::NOTE,
        Severity::Help => Level::HELP,
    };

    let title = level.primary_title(&*diagnostic.message);

    let mut group = Group::with_title(title);

    for chunk in &diagnostic.chunks {
        let path = chunk.path.as_str().to_string();
        let mut snippet: Snippet<'static, annotate_snippets::Annotation<'static>> =
            Snippet::source(chunk.fragment.as_str().to_string())
                .line_start(chunk.line)
                .path(path);

        for annotation in &chunk.annotations {
            snippet = snippet.annotation(
                AnnotationKind::Primary
                    .span(annotation.span.0.clone())
                    .label(annotation.message.as_str().to_string()),
            );
        }

        group = group.element(snippet);
    }

    let renderer = Renderer::styled().decor_style(DecorStyle::Unicode);
    SharedString::from(renderer.render(&[group]).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::Annotation;
    use crate::span::Span;
    use expect_test::expect;
    use rajac_base::file_path::FilePath;
    use strip_ansi::strip_ansi;

    #[test]
    /// Tests that an error diagnostic is rendered correctly.
    fn test_render_error() {
        let diagnostic = Diagnostic {
            severity: Severity::Error,
            message: "expected type, found `i32`".into(),
            chunks: vec![SourceChunk {
                path: FilePath::new("test.java"),
                fragment: "let x: String = 42;".into(),
                offset: 0,
                line: 1,
                annotations: vec![],
            }],
        };

        let output = render_diagnostic(&diagnostic);
        let stripped = strip_ansi(&output);

        let expected = expect![[r#"
            error: expected type, found `i32`
              ╭▸ test.java
              │"#]];
        expected.assert_eq(&stripped);
    }

    #[test]
    /// Tests that a warning diagnostic is rendered correctly.
    fn test_render_warning() {
        let diagnostic = Diagnostic {
            severity: Severity::Warning,
            message: "unused variable `x`".into(),
            chunks: vec![SourceChunk {
                path: FilePath::new("test.java"),
                fragment: "let x = 42;".into(),
                offset: 0,
                line: 5,
                annotations: vec![],
            }],
        };

        let output = render_diagnostic(&diagnostic);
        let stripped = strip_ansi(&output);

        let expected = expect![[r#"
            warning: unused variable `x`
              ╭▸ test.java
              │"#]];
        expected.assert_eq(&stripped);
    }

    #[test]
    /// Tests rendering a diagnostic with an annotation.
    fn test_render_with_annotation() {
        let diagnostic = Diagnostic {
            severity: Severity::Error,
            message: "mismatched types".into(),
            chunks: vec![SourceChunk {
                path: FilePath::new("test.java"),
                fragment: "let x: String = 42;".into(),
                offset: 0,
                line: 1,
                annotations: vec![Annotation {
                    span: Span(9..15),
                    message: "expected `String` but found `i32`".into(),
                }],
            }],
        };

        let output = render_diagnostic(&diagnostic);
        let stripped = strip_ansi(&output);

        let expected = expect![[r#"
            error: mismatched types
              ╭▸ test.java:1:10
              │
            1 │ let x: String = 42;
              ╰╴         ━━━━━━ expected `String` but found `i32`"#]];
        expected.assert_eq(&stripped);
    }

    #[test]
    /// Tests rendering a diagnostic with multiple chunks.
    fn test_render_multiple_chunks() {
        let diagnostic = Diagnostic {
            severity: Severity::Error,
            message: "undefined variable".into(),
            chunks: vec![
                SourceChunk {
                    path: FilePath::new("main.java"),
                    fragment: "fn main() {}".into(),
                    offset: 0,
                    line: 1,
                    annotations: vec![],
                },
                SourceChunk {
                    path: FilePath::new("main.java"),
                    fragment: "    x;".into(),
                    offset: 12,
                    line: 2,
                    annotations: vec![],
                },
            ],
        };

        let output = render_diagnostic(&diagnostic);
        let stripped = strip_ansi(&output);

        let expected = expect![[r#"
            error: undefined variable
              ╭▸ main.java
              │
              │
              ⸬  main.java
              │"#]];
        expected.assert_eq(&stripped);
    }
}
