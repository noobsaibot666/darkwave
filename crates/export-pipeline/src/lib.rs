#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportIntent {
    pub preserve_original: bool,
    pub include_license_record: bool,
}

pub fn default_editorial_export_intent() -> ExportIntent {
    ExportIntent {
        preserve_original: true,
        include_license_record: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_preserve_traceability_by_default() {
        assert_eq!(
            default_editorial_export_intent(),
            ExportIntent {
                preserve_original: true,
                include_license_record: true,
            }
        );
    }
}
