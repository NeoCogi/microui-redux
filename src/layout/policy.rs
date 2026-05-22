//! Public layout sizing policy types.

/// Size policy used by rows and columns when resolving cells.
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

impl SizePolicy {
    /// Normalizes negative, NaN, and infinite weights before distribution.
    pub(super) fn clamp_weight(value: f32) -> f32 {
        if value.is_finite() { value.max(0.0) } else { 0.0 }
    }

    /// Normalizes fractions to the usable `0.0..=1.0` sizing range.
    fn clamp_fraction(value: f32) -> f32 {
        if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 }
    }

    /// Resolves a fractional policy against non-negative reference space.
    pub(super) fn resolve_fraction(fraction: f32, reference_space: i32) -> i32 {
        let reference = reference_space.max(0) as f32;
        (reference * Self::clamp_fraction(fraction)).floor() as i32
    }

    /// Resolves a weighted policy against either sibling total weight or its own weight.
    fn resolve_weight(weight: f32, reference_space: i32, total_weight: Option<f32>) -> i32 {
        let w = Self::clamp_weight(weight);
        if w <= 0.0 {
            return 0;
        }

        let denom = match total_weight {
            Some(total) if total.is_finite() && total > 0.0 => total,
            _ => w,
        };
        let reference = reference_space.max(0) as f32;
        (reference * (w / denom)).floor() as i32
    }

    /// Sums positive sibling weights for proportional row/grid distribution.
    pub(super) fn total_weight(policies: &[SizePolicy]) -> Option<f32> {
        let total = policies
            .iter()
            .map(|policy| match policy {
                SizePolicy::Weight(value) => Self::clamp_weight(*value),
                _ => 0.0,
            })
            .sum::<f32>();
        if total > 0.0 { Some(total) } else { None }
    }

    /// Resolves a policy without a sibling weight context.
    pub(super) fn resolve(self, default_size: i32, available_space: i32) -> i32 {
        let resolved = match self {
            SizePolicy::Auto => default_size,
            SizePolicy::Fixed(value) => value,
            SizePolicy::Weight(weight) => Self::resolve_weight(weight, available_space, None),
            SizePolicy::Fraction(fraction) => Self::resolve_fraction(fraction, available_space),
            SizePolicy::Remainder(margin) => available_space.saturating_sub(margin),
        };
        resolved.max(0)
    }

    /// Resolves a policy with a specific reference space and optional sibling weight total.
    pub(super) fn resolve_with_reference(self, default_size: i32, available_space: i32, reference_space: i32, total_weight: Option<f32>) -> i32 {
        let resolved = match self {
            SizePolicy::Weight(weight) => Self::resolve_weight(weight, reference_space, total_weight),
            SizePolicy::Fraction(fraction) => Self::resolve_fraction(fraction, reference_space),
            _ => self.resolve(default_size, available_space),
        };
        resolved.max(0)
    }
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
