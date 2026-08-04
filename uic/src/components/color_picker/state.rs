use gpui::{Context, FocusHandle, Rgba};

use super::{Hsva, interaction::CaptureToken};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColorPickerEvent {
    Preview(Rgba),
    Commit(Rgba),
}

pub struct ColorPickerState {
    hsva: Hsva,
    sv_focus: FocusHandle,
    hue_focus: FocusHandle,
    alpha_focus: FocusHandle,
    sv_capture: CaptureToken,
    hue_capture: CaptureToken,
    alpha_capture: CaptureToken,
}

impl gpui::EventEmitter<ColorPickerEvent> for ColorPickerState {}

impl ColorPickerState {
    pub fn new(value: Rgba, cx: &mut Context<Self>) -> Self {
        Self {
            hsva: Hsva::from(value),
            sv_focus: cx.focus_handle(),
            hue_focus: cx.focus_handle(),
            alpha_focus: cx.focus_handle(),
            sv_capture: CaptureToken::default(),
            hue_capture: CaptureToken::default(),
            alpha_capture: CaptureToken::default(),
        }
    }

    pub fn value(&self) -> Rgba {
        self.hsva.to_rgba()
    }

    pub fn hsva(&self) -> Hsva {
        self.hsva
    }

    pub fn set_value(&mut self, value: Rgba, cx: &mut Context<Self>) {
        self.hsva = Hsva::from_rgba_preserving_hue(value, self.hsva.h);
        cx.notify();
    }

    pub(crate) fn sv_focus(&self) -> FocusHandle {
        self.sv_focus.clone()
    }

    pub(crate) fn hue_focus(&self) -> FocusHandle {
        self.hue_focus.clone()
    }

    pub(crate) fn alpha_focus(&self) -> FocusHandle {
        self.alpha_focus.clone()
    }

    pub(crate) fn sv_capture(&self) -> CaptureToken {
        self.sv_capture.clone()
    }

    pub(crate) fn hue_capture(&self) -> CaptureToken {
        self.hue_capture.clone()
    }

    pub(crate) fn alpha_capture(&self) -> CaptureToken {
        self.alpha_capture.clone()
    }

    pub(crate) fn update_sv(
        &mut self,
        saturation: f32,
        value: f32,
        commit: bool,
        cx: &mut Context<Self>,
    ) {
        let saturation = saturation.clamp(0.0, 1.0);
        let value = value.clamp(0.0, 1.0);
        if !commit && self.hsva.s == saturation && self.hsva.v == value {
            return;
        }
        self.hsva.s = saturation;
        self.hsva.v = value;
        self.publish(commit, cx);
    }

    pub(crate) fn update_hue(&mut self, hue: f32, commit: bool, cx: &mut Context<Self>) {
        let hue = hue.clamp(0.0, 1.0);
        if !commit && self.hsva.h == hue {
            return;
        }
        self.hsva.h = hue;
        self.publish(commit, cx);
    }

    pub(crate) fn update_alpha(&mut self, alpha: f32, commit: bool, cx: &mut Context<Self>) {
        let alpha = alpha.clamp(0.0, 1.0);
        if !commit && self.hsva.a == alpha {
            return;
        }
        self.hsva.a = alpha;
        self.publish(commit, cx);
    }

    fn publish(&mut self, commit: bool, cx: &mut Context<Self>) {
        let value = self.value();
        if commit {
            cx.emit(ColorPickerEvent::Commit(value));
        } else {
            cx.emit(ColorPickerEvent::Preview(value));
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, Rgba, TestAppContext};

    use super::*;

    #[gpui::test]
    fn programmatic_gray_values_keep_the_active_hue(cx: &mut TestAppContext) {
        let state = cx.update(|cx| {
            cx.new(|cx| {
                ColorPickerState::new(
                    Rgba {
                        r: 0.0,
                        g: 0.0,
                        b: 1.0,
                        a: 1.0,
                    },
                    cx,
                )
            })
        });

        cx.update(|cx| {
            state.update(cx, |state, cx| {
                state.set_value(
                    Rgba {
                        r: 0.4,
                        g: 0.4,
                        b: 0.4,
                        a: 0.5,
                    },
                    cx,
                );
                assert!((state.hsva().h - 2.0 / 3.0).abs() < 1e-5);
                assert_eq!(state.hsva().s, 0.0);
                assert_eq!(state.hsva().a, 0.5);
            });
        });
    }
}
