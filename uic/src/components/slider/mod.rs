mod appearance;
mod interaction;
mod slider;
mod state;

pub use appearance::SliderAppearance;
pub use slider::Slider;
pub use state::SliderState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SliderThumbVisibility {
    #[default]
    Always,
    Hover,
    Never,
}

/// Places a slider value tooltip relative to the interaction area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SliderTooltipPlacement {
    #[default]
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderEvent {
    /// The pointer is actively changing the value.
    Changing(f64),
    /// A pointer, keyboard, or accessibility interaction committed the value.
    Changed(f64),
}
