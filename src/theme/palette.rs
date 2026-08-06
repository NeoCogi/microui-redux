//! Semantic color roles used by built-in controls.

#[derive(PartialEq, Copy, Clone)]
#[repr(u32)]
/// Identifiers for each of the built-in style colors.
pub enum ControlColor {
    /// Number of color entries in [`crate::Style::colors`].
    Max = 14,
    /// Thumb of scrollbars.
    ScrollThumb = 13,
    /// Base frame of scrollbars.
    ScrollBase = 12,
    /// Base color for focused widgets.
    BaseFocus = 11,
    /// Base color while the pointer hovers the widget.
    BaseHover = 10,
    /// Default base color.
    Base = 9,
    /// Button color while the widget is focused.
    ButtonFocus = 8,
    /// Button color while the pointer hovers the widget.
    ButtonHover = 7,
    /// Default button color.
    Button = 6,
    /// Panel background color.
    PanelBG = 5,
    /// Window title text color.
    TitleText = 4,
    /// Window title background color.
    TitleBG = 3,
    /// Window background color.
    WindowBG = 2,
    /// Outline/border color.
    Border = 1,
    /// Default text color.
    Text = 0,
}

impl ControlColor {
    /// Promotes the enum to the hover variant when relevant.
    pub fn hover(&mut self) {
        *self = match self {
            Self::Base => Self::BaseHover,
            Self::Button => Self::ButtonHover,
            _ => *self,
        }
    }

    /// Promotes the enum to the focused variant when relevant.
    pub fn focus(&mut self) {
        *self = match self {
            Self::Base => Self::BaseFocus,
            Self::Button => Self::ButtonFocus,
            Self::BaseHover => Self::BaseFocus,
            Self::ButtonHover => Self::ButtonFocus,
            _ => *self,
        }
    }
}
