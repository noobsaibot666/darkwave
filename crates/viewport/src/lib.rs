#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VisibleRange {
    pub start: usize,
    pub end_exclusive: usize,
    pub offset_top_px: u64,
    pub spacer_bottom_px: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtualViewport {
    pub total_rows: usize,
    pub row_height_px: u64,
    pub viewport_height_px: u64,
    pub scroll_top_px: u64,
    pub overscan_rows: usize,
}

impl VirtualViewport {
    pub fn visible_range(&self) -> VisibleRange {
        if self.total_rows == 0 || self.row_height_px == 0 || self.viewport_height_px == 0 {
            return VisibleRange::default();
        }

        let first_visible_row = (self.scroll_top_px / self.row_height_px) as usize;
        let visible_row_count = self
            .viewport_height_px
            .div_ceil(self.row_height_px)
            .try_into()
            .unwrap_or(usize::MAX);
        let start = first_visible_row.saturating_sub(self.overscan_rows);
        let end_exclusive = first_visible_row
            .saturating_add(visible_row_count)
            .saturating_add(self.overscan_rows)
            .min(self.total_rows);
        let offset_top_px = start as u64 * self.row_height_px;
        let spacer_bottom_px =
            (self.total_rows.saturating_sub(end_exclusive)) as u64 * self.row_height_px;

        VisibleRange {
            start,
            end_exclusive,
            offset_top_px,
            spacer_bottom_px,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_range_includes_overscan_before_and_after_viewport() {
        let viewport = VirtualViewport {
            total_rows: 50_000,
            row_height_px: 52,
            viewport_height_px: 520,
            scroll_top_px: 5_200,
            overscan_rows: 4,
        };

        assert_eq!(
            viewport.visible_range(),
            VisibleRange {
                start: 96,
                end_exclusive: 114,
                offset_top_px: 4_992,
                spacer_bottom_px: 2_594_072,
            }
        );
    }

    #[test]
    fn visible_range_clamps_to_start_of_large_catalog() {
        let viewport = VirtualViewport {
            total_rows: 100_000,
            row_height_px: 44,
            viewport_height_px: 440,
            scroll_top_px: 0,
            overscan_rows: 8,
        };

        assert_eq!(viewport.visible_range().start, 0);
        assert_eq!(viewport.visible_range().end_exclusive, 18);
    }

    #[test]
    fn visible_range_clamps_to_end_of_catalog() {
        let viewport = VirtualViewport {
            total_rows: 100,
            row_height_px: 50,
            viewport_height_px: 500,
            scroll_top_px: 4_900,
            overscan_rows: 3,
        };

        assert_eq!(
            viewport.visible_range(),
            VisibleRange {
                start: 95,
                end_exclusive: 100,
                offset_top_px: 4_750,
                spacer_bottom_px: 0,
            }
        );
    }

    #[test]
    fn empty_catalog_has_empty_range_and_no_spacers() {
        let viewport = VirtualViewport {
            total_rows: 0,
            row_height_px: 52,
            viewport_height_px: 520,
            scroll_top_px: 0,
            overscan_rows: 4,
        };

        assert_eq!(viewport.visible_range(), VisibleRange::default());
    }
}
