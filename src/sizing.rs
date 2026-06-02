//! Public retained sizing policy types.

/// Size policy used by retained nodes, row/grid tracks, and stack items when resolving cells.
///
/// Cell sizing resolves in this order:
/// 1. A retained node [`crate::retained::Policy`] override wins when it is not `Auto`.
/// 2. Otherwise the active row/grid/stack track policy is used.
/// 3. `Auto` uses the widget's measured preferred size when it is positive.
/// 4. If a widget reports no preferred size for an axis, the style/default cell fallback is used.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SizePolicy {
    /// Uses the default cell size defined by the style.
    Auto,
    /// Reserves a fixed number of pixels.
    Fixed(i32),
    /// Uses weighted distribution of the current row/column size.
    ///
    /// When multiple sibling tracks use `Weight`, each track receives
    /// `weight / total_weight` of the available track space.
    /// When no sibling weight context exists, a positive weight receives the full reference space.
    Weight(f32),
    /// Uses an explicit `0.0..=1.0` fraction of the reference space.
    ///
    /// This is the proportional sizing policy for single-track flows such as vertical stacks or
    /// uniform row heights.
    Fraction(f32),
    /// Consumes the remaining space with an optional margin.
    Remainder(i32),
}

impl Default for SizePolicy {
    fn default() -> Self {
        SizePolicy::Auto
    }
}

/// Direction used by stack flows when emitting vertical cells.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StackDirection {
    /// Place cells from the current row start downward.
    TopToBottom,
    /// Place cells from the bottom of the current scope upward.
    BottomToTop,
}

impl Default for StackDirection {
    fn default() -> Self {
        Self::TopToBottom
    }
}
