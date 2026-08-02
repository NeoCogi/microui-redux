//! Public retained sizing policy types.

/// Size policy used by retained nodes, row/grid tracks, and stack items when resolving cells.
///
/// Cell sizing resolves in this order:
/// 1. A retained node [`crate::Policy`] override wins when it is not `Auto`.
/// 2. Otherwise the active row/grid/stack track policy is used.
/// 3. `Auto` uses the widget's measured preferred size.
/// 4. Containers may define a style fallback for explicit empty tracks.
///
/// Widget measurement itself reports content only. When a container turns those preferences into
/// intrinsic tracks, flexible policies keep the content size and only `Fixed` forces an extent.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SizePolicy {
    /// Uses measured content.
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

/// Placement policy attached to one retained [`crate::Node`].
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Policy {
    /// Width policy associated with the node.
    pub width: SizePolicy,
    /// Height policy associated with the node.
    pub height: SizePolicy,
}

impl Policy {
    /// Creates a policy from explicit width and height rules.
    pub const fn new(width: SizePolicy, height: SizePolicy) -> Self {
        Self { width, height }
    }

    /// Uses automatic sizing on both axes.
    pub const fn auto() -> Self {
        Self::new(SizePolicy::Auto, SizePolicy::Auto)
    }

    /// Uses fixed sizing on both axes.
    pub const fn fixed(width: i32, height: i32) -> Self {
        Self::new(SizePolicy::Fixed(width), SizePolicy::Fixed(height))
    }

    /// Uses a fixed width and automatic height.
    pub const fn fixed_width(width: i32) -> Self {
        Self::new(SizePolicy::Fixed(width), SizePolicy::Auto)
    }

    /// Uses a fixed height and automatic width.
    pub const fn fixed_height(height: i32) -> Self {
        Self::new(SizePolicy::Auto, SizePolicy::Fixed(height))
    }

    /// Uses remainder sizing on both axes.
    pub const fn fill() -> Self {
        Self::new(SizePolicy::Remainder(0), SizePolicy::Remainder(0))
    }
}

impl Default for SizePolicy {
    fn default() -> Self {
        SizePolicy::Auto
    }
}

impl SizePolicy {
    /// Returns this policy's contribution when no finite parent extent exists.
    ///
    /// Only `Fixed` can force a size without a reference axis. Every flexible policy falls back to
    /// the widget's measured content so auto-size cannot manufacture space from an arbitrary probe.
    pub(crate) fn intrinsic_extent(self, content: i32) -> i32 {
        match self {
            Self::Fixed(value) => value,
            _ => content,
        }
        .max(0)
    }

    /// Converts a parent allocation into the width or height offered during child measurement.
    ///
    /// Zero retains the public "unbounded" convention. A fixed policy remains meaningful without
    /// a parent bound; other flexible policies need content measurement to establish a preference.
    pub(crate) fn measurement_bound(self, available: i32) -> i32 {
        if available <= 0 {
            return match self {
                Self::Fixed(value) => value.max(0),
                _ => 0,
            };
        }
        // A positive measurement offer must remain positive even when a policy resolves to zero;
        // zero is reserved for the distinct unbounded-measurement request.
        self.allocated_extent(available).max(1)
    }

    /// Combines measured content with this policy for a container's preferred extent.
    ///
    /// `Auto` preserves content under a positive bound. Other policies resolve against that bound,
    /// while an unbounded query uses the intrinsic rule above.
    pub(crate) fn preferred_extent(self, content: i32, available: i32) -> i32 {
        if available <= 0 {
            self.intrinsic_extent(content)
        } else {
            match self {
                Self::Auto => content.max(0),
                _ => self.allocated_extent(available),
            }
        }
    }

    /// Resolves one policy inside an already allocated parent slot.
    ///
    /// Sibling-aware weight sharing is handled by the private container-axis cursor. At this final
    /// node boundary, a valid positive `Weight` consumes the slot the parent already assigned it.
    pub(crate) fn allocated_extent(self, available: i32) -> i32 {
        match self {
            Self::Auto => available,
            Self::Fixed(value) => value,
            Self::Fraction(value) => scaled(available, value.clamp(0.0, 1.0)),
            Self::Weight(value) if value.is_finite() && value > 0.0 => available,
            Self::Weight(_) => 0,
            Self::Remainder(margin) => available.saturating_sub(margin.max(0)),
        }
        .max(0)
    }
}

/// Multiplies a non-negative pixel extent by a finite positive ratio.
///
/// Invalid and non-positive ratios deliberately resolve to zero rather than propagating NaN or
/// producing a negative layout extent. Flooring keeps allocation deterministic in integer pixels.
pub(crate) fn scaled(total: i32, ratio: f32) -> i32 {
    if ratio.is_finite() && ratio > 0.0 {
        ((total.max(0) as f32) * ratio).floor() as i32
    } else {
        0
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
