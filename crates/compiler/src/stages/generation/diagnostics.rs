use rajac_base::file_path::FilePath;
use rajac_base::shared_string::SharedString;
use rajac_bytecode::bytecode::UnsupportedFeature;
use rajac_diagnostics::{Annotation, Diagnostic, Diagnostics, Severity, SourceChunk, Span};

pub(crate) fn generation_diagnostics_for_unsupported_features(
    source_file: &FilePath,
    source: &str,
    unsupported_features: &[UnsupportedFeature],
) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    for unsupported_feature in unsupported_features {
        diagnostics.add(Diagnostic {
            severity: Severity::Error,
            message: unsupported_feature.message.clone(),
            chunks: vec![source_chunk_for_marker(
                source_file,
                source,
                unsupported_feature.marker.as_str(),
            )],
        });
    }
    diagnostics
}

fn source_chunk_for_marker(source_file: &FilePath, source: &str, marker: &str) -> SourceChunk {
    let offset = source.find(marker).unwrap_or(0);
    let (line, line_start, line_end) = line_bounds_for_offset(source, offset);
    let fragment = &source[line_start..line_end];
    let annotation_start = fragment.find(marker).unwrap_or(0);
    let annotation_end = annotation_start + marker.len().max(1);

    SourceChunk {
        path: source_file.clone(),
        fragment: SharedString::new(fragment),
        offset: line_start,
        line,
        annotations: vec![Annotation {
            span: Span(annotation_start..annotation_end),
            message: SharedString::new(""),
        }],
    }
}

fn line_bounds_for_offset(source: &str, offset: usize) -> (usize, usize, usize) {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[offset..]
        .find('\n')
        .map_or(source.len(), |index| offset + index);
    let line = source[..line_start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    (line, line_start, line_end)
}
