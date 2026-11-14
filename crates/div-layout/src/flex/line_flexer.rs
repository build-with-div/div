//! LineFlexer — resolve flexible lengths per CSS Flexbox §9.7.
//!
//! Extracted character-by-character from Blink's `LineFlexer`
//! (core/layout/flex/line_flexer.cc, 182 lines).
//!
//! This is the heart of flexbox: given a set of items on a line and the
//! available main-axis space, distribute extra space (grow) or absorb
//! overflow (shrink) among the items.

use super::item::{FlexItem, FlexerState};
use div_geometry::LayoutUnit;

/// Mode of operation for the flexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexMode {
    Grow,
    Shrink,
}

/// Resolves flexible lengths for a single flex line.
///
/// Implements CSS Flexbox §9.7 "Resolving Flexible Lengths".
/// Blink: `LineFlexer` class in `line_flexer.cc`.
pub struct LineFlexer<'a> {
    items: &'a mut [FlexItem],
    line_indices: &'a [usize],
    mode: FlexMode,
    initial_free_space: LayoutUnit,
    total_flex_factor: f64,
    free_space: LayoutUnit,
    gap_total: LayoutUnit,
    main_axis_inner_size: LayoutUnit,
}

impl<'a> LineFlexer<'a> {
    /// Create a new LineFlexer for the given line.
    ///
    /// Blink constructor: `LineFlexer::LineFlexer(...)` at line_flexer.cc:9.
    ///
    /// - `items`: mutable slice of all flex items
    /// - `line_indices`: indices of items on this line
    /// - `main_axis_inner_size`: container main axis content-box size
    /// - `sum_hypothetical_main_sizes`: sum of all items' hypothetical margin-box sizes
    /// - `gap_between_items`: resolved gap between items
    pub fn new(
        items: &'a mut [FlexItem],
        line_indices: &'a [usize],
        main_axis_inner_size: LayoutUnit,
        sum_hypothetical_main_sizes: LayoutUnit,
        gap_between_items: LayoutUnit,
    ) -> Self {
        let num_gaps = if line_indices.len() > 1 {
            line_indices.len() as i32 - 1
        } else {
            0
        };
        let gap_total = gap_between_items * num_gaps;

        // Step 1: Determine mode (Blink line_flexer.cc:14)
        let mode = if sum_hypothetical_main_sizes + gap_total < main_axis_inner_size {
            FlexMode::Grow
        } else {
            FlexMode::Shrink
        };

        // Step 2: Initial freeze (Blink line_flexer.cc:20-41)
        // Freeze items that cannot flex in the current mode.
        for &idx in line_indices {
            let item = &mut items[idx];
            let flex_factor = match mode {
                FlexMode::Grow => item.flex_grow,
                FlexMode::Shrink => item.flex_shrink,
            };

            let should_freeze = flex_factor == 0.0
                || (mode == FlexMode::Grow
                    && item.base_content_size > item.hypothetical_content_size)
                || (mode == FlexMode::Shrink
                    && item.base_content_size < item.hypothetical_content_size);

            if should_freeze {
                item.state = FlexerState::Frozen;
                item.flexed_content_size = item.hypothetical_content_size;
            } else {
                item.state = FlexerState::None;
            }
        }

        let mut flexer = Self {
            items,
            line_indices,
            mode,
            initial_free_space: LayoutUnit::zero(),
            total_flex_factor: 0.0,
            free_space: LayoutUnit::zero(),
            gap_total,
            main_axis_inner_size,
        };

        flexer.freeze_items();
        flexer.initial_free_space = flexer.free_space;
        flexer
    }

    /// Run the resolve-flexible-lengths loop until convergence.
    /// Blink: called from `PlaceFlexItems` after constructing LineFlexer.
    pub fn run(&mut self) {
        // Loop until no more violations (Blink line_flexer.cc:88)
        while self.resolve_flexible_lengths() {}
    }

    /// Recalculate totals after freezing items.
    /// Blink: `LineFlexer::FreezeItems()` at line_flexer.cc:44.
    fn freeze_items(&mut self) {
        self.total_flex_factor = 0.0;
        self.free_space = self.main_axis_inner_size - self.gap_total;
        let mut total_weighted_flex_shrink: f64 = 0.0;

        for &idx in self.line_indices {
            let item = &mut self.items[idx];
            if item.state == FlexerState::Frozen {
                self.free_space = self.free_space - item.flexed_margin_box_size();
                continue;
            }

            // Blink resets an unfrozen item to its hypothetical size here.
            item.flexed_content_size = item.hypothetical_content_size;

            // Each item's fraction is taken against the running sum of the
            // factors so far, not the line's total (line_flexer.cc:70-81).
            // `resolve_flexible_lengths` walks the items in reverse, so each
            // takes its share of what the later items left, and the first
            // (fraction 1) takes the rest.
            match self.mode {
                FlexMode::Grow => {
                    let flex_factor = item.flex_grow as f64;
                    self.total_flex_factor += flex_factor;
                    item.free_space_fraction = flex_factor / self.total_flex_factor;
                }
                FlexMode::Shrink => {
                    let flex_factor = item.flex_shrink as f64;
                    self.total_flex_factor += flex_factor;
                    let weighted_flex_shrink = flex_factor * item.base_content_size.to_f64();
                    total_weighted_flex_shrink += weighted_flex_shrink;
                    item.free_space_fraction = weighted_flex_shrink / total_weighted_flex_shrink;
                }
            }

            self.free_space = self.free_space - item.flex_base_margin_box_size();
        }
    }

    /// One iteration of the resolve-flexible-lengths algorithm.
    /// Returns true if the loop should continue, false if converged.
    /// Blink: `LineFlexer::ResolveFlexibleLengths()` at line_flexer.cc:88.
    fn resolve_flexible_lengths(&mut self) -> bool {
        // Step a: Fractional flex factors (Blink line_flexer.cc:97-107)
        // If total_flex_factor < 1.0, limit distribution.
        let mut free_space = self.free_space;
        if self.total_flex_factor > 0.0 && self.total_flex_factor < 1.0 {
            // `LayoutUnit(double)` truncates, as `from_f64` does.
            let fractional =
                LayoutUnit::from_f64(self.initial_free_space.to_f64() * self.total_flex_factor);
            if fractional.to_f64().abs() < free_space.to_f64().abs() {
                free_space = fractional;
            }
        }

        // Step b: Early exit if no space to distribute (Blink line_flexer.cc:109-115).
        // Unfrozen items keep the hypothetical size `freeze_items` gave them.
        if self.mode == FlexMode::Grow && free_space <= LayoutUnit::zero() {
            return false;
        }
        if self.mode == FlexMode::Shrink && free_space >= LayoutUnit::zero() {
            return false;
        }

        // Step c: Distribute free space (Blink line_flexer.cc:139-168). In
        // reverse, each item takes its fraction of the space the later items
        // left, rounded to the nearest unit, and the first takes the rest.
        let mut total_violation = LayoutUnit::zero();

        for &idx in self.line_indices.iter().rev() {
            let item = &mut self.items[idx];
            if item.state == FlexerState::Frozen {
                continue;
            }

            let extra_size = if item.free_space_fraction == 1.0 {
                free_space
            } else {
                let extra = free_space.to_f64() * item.free_space_fraction;
                if extra.is_finite() {
                    LayoutUnit::from_f64_round(extra)
                } else {
                    LayoutUnit::zero()
                }
            };
            free_space = free_space - extra_size;

            let unclamped = item.base_content_size + extra_size;
            let clamped = item.main_axis_min_max.clamp(unclamped);

            let violation = clamped - unclamped;
            total_violation = total_violation + violation;

            item.flexed_content_size = clamped;

            if violation < LayoutUnit::zero() {
                item.state = FlexerState::MaxViolation;
            } else if violation > LayoutUnit::zero() {
                item.state = FlexerState::MinViolation;
            }
        }

        // Step d: Freeze violators (Blink line_flexer.cc:171-181)
        if total_violation == LayoutUnit::zero() {
            // No violations — converged. Freeze all remaining unfrozen items.
            for &idx in self.line_indices {
                if self.items[idx].state != FlexerState::Frozen {
                    self.items[idx].state = FlexerState::Frozen;
                }
            }
            return false;
        }

        // Positive total → freeze min-violations
        // Negative total → freeze max-violations
        let freeze_state = if total_violation > LayoutUnit::zero() {
            FlexerState::MinViolation
        } else {
            FlexerState::MaxViolation
        };

        let mut any_frozen = false;
        for &idx in self.line_indices {
            let item = &mut self.items[idx];
            if item.state == freeze_state {
                item.state = FlexerState::Frozen;
                any_frozen = true;
            } else if item.state != FlexerState::Frozen {
                // Reset non-frozen, non-matching violations for next round
                item.state = FlexerState::None;
            }
        }

        if any_frozen {
            self.freeze_items();
        }

        any_frozen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use div_dom::NodeId;
    use div_geometry::MinMaxSizes;
    use div_style::{ItemPosition, OverflowAlignment};

    fn make_test_item(index: usize, base: i32, grow: f32, shrink: f32) -> FlexItem {
        FlexItem {
            node_id: NodeId::NONE,
            item_index: index,
            flex_grow: grow,
            flex_shrink: shrink,
            base_content_size: LayoutUnit::from_i32(base),
            hypothetical_content_size: LayoutUnit::from_i32(base),
            main_axis_min_max: MinMaxSizes::new(LayoutUnit::zero(), LayoutUnit::from_i32(10000)),
            main_axis_border_padding: LayoutUnit::zero(),
            margin: div_geometry::BoxStrut::zero(),
            main_axis_auto_margin_count: 0,
            alignment: ItemPosition::Stretch,
            alignment_overflow: OverflowAlignment::Default,
            flexed_content_size: LayoutUnit::zero(),
            state: FlexerState::None,
            free_space_fraction: 0.0,
            is_used_flex_basis_indefinite: false,
            is_horizontal_flow: true,
            is_collapsed: false,
        }
    }

    #[test]
    fn equal_grow_distributes_evenly() {
        let mut items = vec![
            make_test_item(0, 100, 1.0, 1.0),
            make_test_item(1, 100, 1.0, 1.0),
        ];
        let indices = vec![0, 1];
        let container_size = LayoutUnit::from_i32(400); // 200 extra space
        let sum_hyp = LayoutUnit::from_i32(200);

        let mut flexer = LineFlexer::new(
            &mut items,
            &indices,
            container_size,
            sum_hyp,
            LayoutUnit::zero(),
        );
        flexer.run();

        // Each gets 100 extra → 200 each
        assert_eq!(items[0].flexed_content_size, LayoutUnit::from_i32(200));
        assert_eq!(items[1].flexed_content_size, LayoutUnit::from_i32(200));
    }

    #[test]
    fn weighted_grow() {
        let mut items = vec![
            make_test_item(0, 0, 1.0, 1.0),
            make_test_item(1, 0, 2.0, 1.0),
            make_test_item(2, 0, 1.0, 1.0),
        ];
        let indices = vec![0, 1, 2];
        let container_size = LayoutUnit::from_i32(400);
        let sum_hyp = LayoutUnit::zero();

        let mut flexer = LineFlexer::new(
            &mut items,
            &indices,
            container_size,
            sum_hyp,
            LayoutUnit::zero(),
        );
        flexer.run();

        // Proportions: 1:2:1 of 400 = 100:200:100
        assert_eq!(items[0].flexed_content_size, LayoutUnit::from_i32(100));
        assert_eq!(items[1].flexed_content_size, LayoutUnit::from_i32(200));
        assert_eq!(items[2].flexed_content_size, LayoutUnit::from_i32(100));
    }

    #[test]
    fn equal_shrink() {
        let mut items = vec![
            make_test_item(0, 200, 0.0, 1.0),
            make_test_item(1, 200, 0.0, 1.0),
        ];
        let indices = vec![0, 1];
        let container_size = LayoutUnit::from_i32(300); // 100 overflow
        let sum_hyp = LayoutUnit::from_i32(400);

        let mut flexer = LineFlexer::new(
            &mut items,
            &indices,
            container_size,
            sum_hyp,
            LayoutUnit::zero(),
        );
        flexer.run();

        // Equal base sizes → equal shrink: each loses 50 → 150 each
        assert_eq!(items[0].flexed_content_size, LayoutUnit::from_i32(150));
        assert_eq!(items[1].flexed_content_size, LayoutUnit::from_i32(150));
    }

    #[test]
    fn shrink_weighted_by_base_size() {
        // Item 0: base 300, shrink 1.0 → weighted = 300
        // Item 1: base 100, shrink 1.0 → weighted = 100
        // Total weighted = 400, overflow = 100
        // Item 0 loses: 100 * 300/400 = 75 → 225
        // Item 1 loses: 100 * 100/400 = 25 → 75
        let mut items = vec![
            make_test_item(0, 300, 0.0, 1.0),
            make_test_item(1, 100, 0.0, 1.0),
        ];
        let indices = vec![0, 1];
        let container_size = LayoutUnit::from_i32(300);
        let sum_hyp = LayoutUnit::from_i32(400);

        let mut flexer = LineFlexer::new(
            &mut items,
            &indices,
            container_size,
            sum_hyp,
            LayoutUnit::zero(),
        );
        flexer.run();

        assert_eq!(items[0].flexed_content_size, LayoutUnit::from_i32(225));
        assert_eq!(items[1].flexed_content_size, LayoutUnit::from_i32(75));
    }

    #[test]
    fn min_constraint_freezes_item() {
        // Item 0: base 200, grow 1, min=180
        // Item 1: base 200, grow 1, min=0
        // Container: 300 → need to shrink by 100
        // First round: both shrink by 50 → 150 each
        // Item 0 hits min 180 → clamped, violation = 30
        // Item 0 frozen at 180, item 1 gets remaining: 300 - 180 = 120
        let mut items = vec![
            make_test_item(0, 200, 0.0, 1.0),
            make_test_item(1, 200, 0.0, 1.0),
        ];
        items[0].main_axis_min_max.min = LayoutUnit::from_i32(180);
        let indices = vec![0, 1];
        let container_size = LayoutUnit::from_i32(300);
        let sum_hyp = LayoutUnit::from_i32(400);

        let mut flexer = LineFlexer::new(
            &mut items,
            &indices,
            container_size,
            sum_hyp,
            LayoutUnit::zero(),
        );
        flexer.run();

        assert_eq!(items[0].flexed_content_size, LayoutUnit::from_i32(180));
        assert_eq!(items[1].flexed_content_size, LayoutUnit::from_i32(120));
    }

    #[test]
    fn no_flex_items_keep_hypothetical() {
        let mut items = vec![
            make_test_item(0, 100, 0.0, 0.0),
            make_test_item(1, 100, 0.0, 0.0),
        ];
        let indices = vec![0, 1];
        let container_size = LayoutUnit::from_i32(400);
        let sum_hyp = LayoutUnit::from_i32(200);

        let mut flexer = LineFlexer::new(
            &mut items,
            &indices,
            container_size,
            sum_hyp,
            LayoutUnit::zero(),
        );
        flexer.run();

        // No grow/shrink → keep hypothetical sizes
        assert_eq!(items[0].flexed_content_size, LayoutUnit::from_i32(100));
        assert_eq!(items[1].flexed_content_size, LayoutUnit::from_i32(100));
    }

    #[test]
    fn gap_reduces_free_space() {
        let mut items = vec![
            make_test_item(0, 100, 1.0, 1.0),
            make_test_item(1, 100, 1.0, 1.0),
        ];
        let indices = vec![0, 1];
        let container_size = LayoutUnit::from_i32(400);
        let sum_hyp = LayoutUnit::from_i32(200);
        let gap = LayoutUnit::from_i32(20); // 1 gap of 20px

        let mut flexer = LineFlexer::new(&mut items, &indices, container_size, sum_hyp, gap);
        flexer.run();

        // Free space = 400 - 200 - 20 = 180, each gets 90
        assert_eq!(items[0].flexed_content_size, LayoutUnit::from_i32(190));
        assert_eq!(items[1].flexed_content_size, LayoutUnit::from_i32(190));
    }

    #[test]
    fn fractional_flex_factor_less_than_one() {
        // Blink: when total_flex_factor < 1.0, limit to initial_free_space * total
        let mut items = vec![
            make_test_item(0, 100, 0.25, 1.0),
            make_test_item(1, 100, 0.25, 1.0),
        ];
        let indices = vec![0, 1];
        let container_size = LayoutUnit::from_i32(400); // 200 extra
        let sum_hyp = LayoutUnit::from_i32(200);

        let mut flexer = LineFlexer::new(
            &mut items,
            &indices,
            container_size,
            sum_hyp,
            LayoutUnit::zero(),
        );
        flexer.run();

        // total_flex_factor = 0.5, limit = 200 * 0.5 = 100
        // Each gets 50 → 150 each
        assert_eq!(items[0].flexed_content_size, LayoutUnit::from_i32(150));
        assert_eq!(items[1].flexed_content_size, LayoutUnit::from_i32(150));
    }

    #[test]
    fn max_constraint_freezes_item() {
        // CSS Flexbox §9.7: items hitting max-width should be frozen and
        // remaining space redistributed to other items.
        let mut items = vec![
            make_test_item(0, 0, 1.0, 1.0),
            make_test_item(1, 0, 1.0, 1.0),
        ];
        items[0].main_axis_min_max.max = LayoutUnit::from_i32(100);
        let indices = vec![0, 1];
        let container_size = LayoutUnit::from_i32(400);
        let sum_hyp = LayoutUnit::zero();

        let mut flexer = LineFlexer::new(
            &mut items,
            &indices,
            container_size,
            sum_hyp,
            LayoutUnit::zero(),
        );
        flexer.run();

        // Item 0 frozen at max=100, Item 1 gets remaining 300
        assert_eq!(items[0].flexed_content_size, LayoutUnit::from_i32(100));
        assert_eq!(items[1].flexed_content_size, LayoutUnit::from_i32(300));
    }

    #[test]
    fn multiple_max_constraints() {
        // Multiple items hitting max constraints in successive rounds
        let mut items = vec![
            make_test_item(0, 0, 1.0, 1.0),
            make_test_item(1, 0, 1.0, 1.0),
            make_test_item(2, 0, 1.0, 1.0),
        ];
        items[0].main_axis_min_max.max = LayoutUnit::from_i32(50);
        items[1].main_axis_min_max.max = LayoutUnit::from_i32(100);
        let indices = vec![0, 1, 2];
        let container_size = LayoutUnit::from_i32(600);
        let sum_hyp = LayoutUnit::zero();

        let mut flexer = LineFlexer::new(
            &mut items,
            &indices,
            container_size,
            sum_hyp,
            LayoutUnit::zero(),
        );
        flexer.run();

        // Round 1: each gets 200. Item 0 clamped to 50, Item 1 clamped to 100.
        // Both frozen, remaining = 600 - 50 - 100 = 450 → Item 2 = 450
        assert_eq!(items[0].flexed_content_size, LayoutUnit::from_i32(50));
        assert_eq!(items[1].flexed_content_size, LayoutUnit::from_i32(100));
        assert_eq!(items[2].flexed_content_size, LayoutUnit::from_i32(450));
    }
    fn flex_line(items: &mut [FlexItem], container: i32) {
        let indices: Vec<usize> = (0..items.len()).collect();
        let sum_hyp = items.iter().fold(LayoutUnit::zero(), |sum, item| {
            sum + item.hypothetical_content_size
        });
        LineFlexer::new(
            items,
            &indices,
            LayoutUnit::from_i32(container),
            sum_hyp,
            LayoutUnit::zero(),
        )
        .run();
    }

    fn raw_sizes(items: &[FlexItem]) -> Vec<i32> {
        items
            .iter()
            .map(|item| item.flexed_content_size.raw())
            .collect()
    }

    /// Fractional grow factors split space as Chromium 147 does: 276px and
    /// 120px, measured by sample-interactions in Chrome for Testing. Truncating
    /// shares of the total gave 276.015625 and 119.984375.
    #[test]
    fn fractional_grow_rounds_each_share_like_blink() {
        let mut items = vec![
            make_test_item(0, 0, 69.697, 1.0),
            make_test_item(1, 0, 30.303, 1.0),
        ];
        flex_line(&mut items, 396);
        assert_eq!(raw_sizes(&items), vec![276 * 64, 120 * 64]);
    }

    /// Grow 10, 70 and 20 over 792px. In reverse, the last item takes
    /// round(792 x 20/100) = 158.40625, the middle one round(633.59375 x
    /// 70/80) = 554.390625, and the first the rest, 79.203125, as in
    /// Chromium 147. Shares of the total, truncated, gave 79.1875, 554.40625
    /// and 158.390625.
    #[test]
    fn grow_shares_are_taken_from_what_later_items_left() {
        let mut items = vec![
            make_test_item(0, 0, 10.0, 1.0),
            make_test_item(1, 0, 70.0, 1.0),
            make_test_item(2, 0, 20.0, 1.0),
        ];
        flex_line(&mut items, 792);
        assert_eq!(raw_sizes(&items), vec![5069, 35481, 10138]);
    }

    /// Shrink weights each factor by the base size against the running sum,
    /// as Blink's `FreezeItems` does: 100px and 200px bases in 200px give the
    /// second item round(-100 x 200/300) = -66.671875 and the first the rest,
    /// -33.328125.
    #[test]
    fn shrink_shares_are_taken_from_what_later_items_left() {
        let mut items = vec![
            make_test_item(0, 100, 1.0, 1.0),
            make_test_item(1, 200, 1.0, 1.0),
        ];
        flex_line(&mut items, 200);
        assert_eq!(raw_sizes(&items), vec![100 * 64 - 2133, 200 * 64 - 4267]);
    }

    #[test]
    fn from_f64_round_matches_blink_from_double_round() {
        assert_eq!(LayoutUnit::from_f64_round(119.99988).raw(), 120 * 64);
        assert_eq!(LayoutUnit::from_f64_round(0.5 / 64.0).raw(), 1);
        assert_eq!(LayoutUnit::from_f64_round(-0.5 / 64.0).raw(), -1);
        assert_eq!(LayoutUnit::from_f64_round(f64::NAN).raw(), 0);
        assert_eq!(LayoutUnit::from_f64_round(f64::INFINITY).raw(), i32::MAX);
    }
}
