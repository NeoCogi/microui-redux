//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//

//! Shared formatting and inline editing for numeric widgets.

use std::{
    error::Error,
    fmt::{Display, Formatter, Write},
};

use crate::*;

use super::textbox::{textbox_paint, textbox_update};

/// Validated number of fractional decimal digits rendered by numeric widgets.
///
/// [`Real`] is `f32`, whose [`f32::DIGITS`] significant decimal digits can be represented without
/// loss. Allowing a larger fixed formatting precision cannot add source precision and would let
/// one widget request an arbitrarily large string.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DecimalPrecision(u8);

impl DecimalPrecision {
    /// Zero fractional digits, used by integer-like numeric controls and [`Default`].
    pub const ZERO: Self = Self(0);

    /// Greatest meaningful formatting precision for the crate's [`Real`] representation.
    pub const MAX: Self = Self(Real::DIGITS as u8);

    /// Returns the validated number of fractional digits as a formatting precision.
    pub const fn digits(self) -> usize {
        self.0 as usize
    }
}

impl Default for DecimalPrecision {
    /// Selects integer-like formatting without a fallible conversion.
    fn default() -> Self {
        Self::ZERO
    }
}

/// Error returned when a requested decimal precision exceeds [`DecimalPrecision::MAX`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct DecimalPrecisionError {
    /// Complete rejected value, retained without narrowing or allocation.
    requested: usize,
}

impl DecimalPrecisionError {
    /// Returns the rejected number of fractional digits.
    pub const fn requested(self) -> usize {
        self.requested
    }

    /// Returns the largest accepted number of fractional digits.
    pub const fn maximum(self) -> usize {
        DecimalPrecision::MAX.digits()
    }
}

impl Display for DecimalPrecisionError {
    /// Describes both the rejected precision and the semantic `Real`-based limit.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "decimal precision must be at most {}, received {}",
            DecimalPrecision::MAX.digits(),
            self.requested
        )
    }
}

impl Error for DecimalPrecisionError {}

impl TryFrom<usize> for DecimalPrecision {
    type Error = DecimalPrecisionError;

    /// Validates before narrowing so even `usize::MAX` is rejected without truncation.
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        if value <= Self::MAX.digits() {
            Ok(Self(value as u8))
        } else {
            Err(DecimalPrecisionError { requested: value })
        }
    }
}

/// Concrete construction failure shared by [`NumberParameters`] and [`SliderParameters`].
///
/// Numeric fields on both parameter types are private, so successfully constructing a parameter
/// value establishes these invariants for the complete one-shot handoff to its widget builder.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum NumericParameterError {
    /// The initial number or slider value was NaN or infinite.
    NonFiniteValue {
        /// Rejected initial value.
        value: Real,
    },
    /// The number drag or slider snapping step was NaN or infinite.
    NonFiniteStep {
        /// Rejected step.
        step: Real,
    },
    /// A finite step pointed in the unsupported negative direction.
    NegativeStep {
        /// Rejected step.
        step: Real,
    },
    /// At least one slider endpoint was NaN or infinite.
    NonFiniteRangeBounds {
        /// Rejected lower endpoint.
        low: Real,
        /// Rejected upper endpoint.
        high: Real,
    },
    /// The finite endpoints described the removed descending orientation.
    DescendingRange {
        /// Rejected lower endpoint.
        low: Real,
        /// Rejected upper endpoint.
        high: Real,
    },
    /// Subtracting otherwise finite endpoints produced an infinite range span.
    NonFiniteRangeSpan {
        /// Rejected lower endpoint.
        low: Real,
        /// Rejected upper endpoint.
        high: Real,
    },
}

impl Display for NumericParameterError {
    /// Reports the exact invariant and values rejected by numeric parameter construction.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteValue { value } => write!(formatter, "numeric initial value must be finite, received {value}"),
            Self::NonFiniteStep { step } => write!(formatter, "numeric step must be finite, received {step}"),
            Self::NegativeStep { step } => write!(formatter, "numeric step must be non-negative, received {step}"),
            Self::NonFiniteRangeBounds { low, high } => write!(formatter, "slider range bounds must be finite, received {low}..={high}"),
            Self::DescendingRange { low, high } => write!(formatter, "slider range must satisfy low <= high, received {low}..={high}"),
            Self::NonFiniteRangeSpan { low, high } => write!(formatter, "slider range span must be finite, received {low}..={high}"),
        }
    }
}

impl Error for NumericParameterError {}

/// Validates the scalar inputs shared by both numeric widgets.
pub(super) fn validate_numeric_value_and_step(value: Real, step: Real) -> Result<(), NumericParameterError> {
    // Reject non-finite inputs at construction rather than relying on later formatting or
    // interaction paths to repair them differently.
    if !value.is_finite() {
        return Err(NumericParameterError::NonFiniteValue { value });
    }
    if !step.is_finite() {
        return Err(NumericParameterError::NonFiniteStep { step });
    }
    if step < 0.0 {
        return Err(NumericParameterError::NegativeStep { step });
    }
    Ok(())
}

/// Validates the ascending finite interval required by slider mapping arithmetic.
pub(super) fn validate_slider_range(low: Real, high: Real) -> Result<(), NumericParameterError> {
    // Finite ordered endpoints make pointer, wheel, keyboard, and paint directions agree. Also
    // reject a mathematically valid but unrepresentable span before runtime subtraction yields
    // infinity and poisons percentage calculations.
    if !low.is_finite() || !high.is_finite() {
        return Err(NumericParameterError::NonFiniteRangeBounds { low, high });
    }
    if low > high {
        return Err(NumericParameterError::DescendingRange { low, high });
    }
    if !(high - low).is_finite() {
        return Err(NumericParameterError::NonFiniteRangeSpan { low, high });
    }
    Ok(())
}

/// Formats a numeric value using the widget's display precision.
pub(super) fn number_label(value: Real, precision: DecimalPrecision) -> String {
    let mut label = String::new();
    // DecimalPrecision makes this allocation proportional to one small documented bound.
    let _ = write!(label, "{:.*}", precision.digits(), value);
    label
}

/// Computes preferred size for numeric widgets with optional visual affordance width.
pub(super) fn number_preferred_size(
    style: &Skin,
    atlas: &AtlasHandle,
    font: FontChoice,
    value: Real,
    precision: DecimalPrecision,
    visual_width: i32,
    visual_height: i32,
) -> Dimensioni {
    let label = number_label(value, precision);
    let resolved_font = style.resolve_font_choice(font);
    let text_w = atlas.get_text_size(resolved_font, label.as_str()).width;
    let padding = style.metrics.padding.max(0);
    let vertical_pad = (padding / 2).max(1);
    let font_height = atlas.get_font_height(resolved_font) as i32;
    // Preferred sizes saturate because text metrics, visual hints, and style padding are independent
    // application inputs even though each value is individually representable.
    let width = text_w.max(0).saturating_add(padding.saturating_mul(2)).saturating_add(visual_width.max(0));
    let height = font_height.max(visual_height.max(0)).max(0).saturating_add(vertical_pad.saturating_mul(2));
    Dimensioni::new(width, height)
}

#[derive(Clone, Default, PartialEq)]
/// Editing buffer for number-style widgets.
pub(super) struct NumberEditState {
    /// Whether the widget is currently in edit mode.
    pub(super) editing: bool,
    /// Text buffer for numeric input.
    buf: String,
    /// Cursor position within the buffer (byte index).
    cursor: usize,
}

/// Runs the shared textbox editor for shift-click numeric input.
pub(super) fn number_textbox_update(
    ctx: &mut WidgetUpdateCtx<'_>,
    input: Option<&UiInputEvent>,
    edit: &mut NumberEditState,
    precision: DecimalPrecision,
    font: FontId,
    value: &mut Real,
) -> bool {
    let shift_click = matches!(input, Some(UiInputEvent::MouseDown { button, .. }) if button.intersects(MouseButton::LEFT))
        && ctx.modifiers().intersects(Modifiers::SHIFT)
        && ctx.hovered();

    if shift_click {
        edit.editing = true;
        edit.buf.clear();
        // The same validated precision drives passive labels and the initial editor contents.
        let _ = write!(edit.buf, "{:.*}", precision.digits(), value);
        edit.cursor = edit.buf.len();
    }

    if edit.editing {
        let res = textbox_update(ctx, input, &mut edit.buf, &mut edit.cursor, WidgetOption::NONE, font);
        if res.submitted || !ctx.focused() {
            commit_finite_number(edit.buf.as_str(), value);
            edit.editing = false;
            edit.cursor = 0;
        } else {
            return true;
        }
    }
    false
}

/// Replaces a retained numeric value only when the editor contains one finite [`Real`].
fn commit_finite_number(text: &str, value: &mut Real) {
    // Rust accepts spellings such as `NaN`, `inf`, and exponents that overflow to infinity. They
    // are valid parser outputs but invalid widget state, so failed or non-finite input leaves the
    // last committed value untouched for both Number and Slider.
    if let Ok(parsed) = text.parse::<Real>()
        && parsed.is_finite()
    {
        *value = parsed;
    }
}

/// Paints the shared textbox editor for a numeric widget.
pub(super) fn number_textbox_paint(ctx: &mut WidgetPaintCtx<'_>, edit: &NumberEditState, font: FontId) {
    textbox_paint(ctx, edit.buf.as_str(), edit.cursor, WidgetOption::NONE, font);
}

#[cfg(test)]
mod tests {
    //! Tests for the numeric editor's finite retained-state boundary.

    use super::*;

    /// Verifies parser-supported non-finite spellings and overflow never replace retained state.
    #[test]
    fn numeric_text_commit_retains_the_previous_value_for_non_finite_input() {
        for text in ["NaN", "inf", "-inf", "1e100"] {
            let mut value = 12.5;
            commit_finite_number(text, &mut value);
            assert_eq!(value, 12.5, "{text} must not enter Number or Slider state");
        }

        let mut value = 12.5;
        commit_finite_number("-3.25", &mut value);
        assert_eq!(value, -3.25);
    }
}
