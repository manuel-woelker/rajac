use rajac_diagnostics::Diagnostics;

#[derive(Debug, Default)]
pub struct GenerationResult {
    pub class_count: usize,
    pub diagnostics: Diagnostics,
}
