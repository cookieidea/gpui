use gpui::{
    AccessibleAction, App, Background, ColorSpace, CursorStyle, Entity, Hsla, IntoElement,
    KeyDownEvent, Orientation, RenderOnce, Rgba, Role, Window, black, checkerboard, div, hsla,
    linear_color_stop, linear_gradient, prelude::*, px, relative,
};

use super::{
    ColorPickerState, Hsva,
    interaction::{ColorInteraction, InteractionPhase},
};

#[derive(Clone, Copy, Debug)]
pub struct AlphaSliderAppearance {
    pub checker: Hsla,
    pub marker: Hsla,
    pub border: Hsla,
    pub focus_border: Hsla,
    pub height: gpui::Pixels,
    pub radius: gpui::Pixels,
}

impl Default for AlphaSliderAppearance {
    fn default() -> Self {
        Self {
            checker: hsla(0.0, 0.0, 0.45, 0.45),
            marker: hsla(0.0, 0.0, 1.0, 1.0),
            border: hsla(0.0, 0.0, 0.0, 0.0),
            focus_border: hsla(0.52, 0.85, 0.48, 1.0),
            height: px(18.0),
            radius: px(9.0),
        }
    }
}

#[derive(IntoElement)]
pub struct AlphaSlider {
    state: Entity<ColorPickerState>,
    appearance: AlphaSliderAppearance,
}

impl AlphaSlider {
    pub fn new(state: &Entity<ColorPickerState>) -> Self {
        Self {
            state: state.clone(),
            appearance: AlphaSliderAppearance::default(),
        }
    }

    pub fn appearance(mut self, appearance: AlphaSliderAppearance) -> Self {
        self.appearance = appearance;
        self
    }
}

impl RenderOnce for AlphaSlider {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let appearance = self.appearance;
        let (hsva, focus, capture) = {
            let state = self.state.read(cx);
            (state.hsva(), state.alpha_focus(), state.alpha_capture())
        };
        let opaque = Hsva { a: 1.0, ..hsva }.to_rgba();
        let transparent = Rgba { a: 0.0, ..opaque };
        let pointer_state = self.state.clone();
        let keyboard_state = self.state.clone();
        let increment_state = self.state.clone();
        let decrement_state = self.state.clone();

        div()
            .id(("color-picker-alpha", self.state.entity_id()))
            .relative()
            .debug_selector(|| "color-picker-alpha".to_string())
            .track_focus(&focus)
            .tab_stop(true)
            .role(Role::Slider)
            .aria_numeric_value(f64::from(hsva.a * 100.0))
            .aria_numeric_value_step(1.0)
            .aria_min_numeric_value(0.0)
            .aria_max_numeric_value(100.0)
            .aria_orientation(Orientation::Horizontal)
            .on_key_down(move |event, _, cx| {
                adjust_from_key(&keyboard_state, event, cx);
            })
            .on_a11y_action(AccessibleAction::Increment, move |_, _, cx| {
                adjust_alpha(&increment_state, 0.01, cx);
            })
            .on_a11y_action(AccessibleAction::Decrement, move |_, _, cx| {
                adjust_alpha(&decrement_state, -0.01, cx);
            })
            .w_full()
            .h(appearance.height)
            .overflow_hidden()
            .rounded(appearance.radius)
            .border_1()
            .border_color(appearance.border)
            .cursor(CursorStyle::ResizeLeftRight)
            .bg(checkerboard(appearance.checker, 6.0))
            .focus_visible(move |style| style.border_2().border_color(appearance.focus_border))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(appearance.radius)
                    .bg(two_stop_gradient(90.0, transparent, opaque)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(-3.0))
                    .bottom(px(-3.0))
                    .left(relative(hsva.a))
                    .ml(px(-2.0))
                    .w(px(4.0))
                    .rounded_full()
                    .border_1()
                    .border_color(black())
                    .bg(appearance.marker),
            )
            .child(
                ColorInteraction::new(capture, focus, move |point, phase, _, cx| {
                    pointer_state.update(cx, |state, cx| {
                        state.update_alpha(point.x, phase == InteractionPhase::Commit, cx);
                    });
                })
                .absolute()
                .inset_0(),
            )
    }
}

fn adjust_from_key(state: &Entity<ColorPickerState>, event: &KeyDownEvent, cx: &mut App) {
    let step = if event.keystroke.modifiers.shift {
        0.1
    } else {
        0.01
    };
    let delta = match event.keystroke.key.as_str() {
        "left" | "down" => Some(-step),
        "right" | "up" => Some(step),
        _ => None,
    };
    if let Some(delta) = delta {
        adjust_alpha(state, delta, cx);
        cx.stop_propagation();
    }
}

fn adjust_alpha(state: &Entity<ColorPickerState>, delta: f32, cx: &mut App) {
    state.update(cx, |state, cx| {
        state.update_alpha(state.hsva().a + delta, true, cx);
    });
}

fn two_stop_gradient(angle: f32, from: impl Into<Hsla>, to: impl Into<Hsla>) -> Background {
    linear_gradient(
        angle,
        linear_color_stop(from, 0.0),
        linear_color_stop(to, 1.0),
    )
    .color_space(ColorSpace::Srgb)
}

#[cfg(test)]
mod tests {
    use gpui::{
        Context, Modifiers, MouseButton, Render, TestAppContext, VisualTestContext, point, rgba,
        size,
    };

    use super::*;

    struct TestAlphaSlider {
        state: Entity<ColorPickerState>,
    }

    impl Render for TestAlphaSlider {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(300.0)).child(AlphaSlider::new(&self.state))
        }
    }

    fn open_test_slider(cx: &mut TestAppContext) -> gpui::WindowHandle<TestAlphaSlider> {
        cx.open_window(size(px(500.0), px(200.0)), |_, cx| TestAlphaSlider {
            state: cx.new(|cx| ColorPickerState::new(rgba(0xff000040), cx)),
        })
    }

    #[gpui::test]
    fn drag_clamps_after_leaving_the_alpha_slider(cx: &mut TestAppContext) {
        let window = open_test_slider(cx);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| {
            window.draw(cx).clear();
        });
        let bounds = visual
            .debug_bounds("color-picker-alpha")
            .expect("alpha slider should be rendered");
        let start = point(bounds.left() + bounds.size.width * 0.25, bounds.center().y);
        let outside = point(bounds.right() + px(40.0), bounds.center().y);

        visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(outside, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());

        window
            .update(&mut visual.cx, |view, _, cx| {
                assert_eq!(view.state.read(cx).value().a, 1.0);
            })
            .unwrap();
    }

    #[gpui::test]
    fn keyboard_uses_one_and_ten_percent_steps(cx: &mut TestAppContext) {
        let window = open_test_slider(cx);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| {
            window.draw(cx).clear();
        });
        let bounds = visual
            .debug_bounds("color-picker-alpha")
            .expect("alpha slider should be rendered");
        visual.simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::default());
        visual.simulate_keystrokes("right shift-right");

        window
            .update(&mut visual.cx, |view, _, cx| {
                assert!((view.state.read(cx).value().a - 0.61).abs() < 1e-5);
            })
            .unwrap();
    }
}
