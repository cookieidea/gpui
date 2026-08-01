use gpui::{
    AbsoluteLength, AnyElement, Div, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, div, rems,
};

#[derive(IntoElement)]
pub struct Space {
    div: Div,
    children: Vec<AnyElement>,
    size: AbsoluteLength,
}

impl Space {
    pub fn gap_size(mut self, size: AbsoluteLength) -> Self {
        self.size = size;
        self
    }
}

pub fn space() -> Space {
    Space {
        div: div().flex().items_center(),
        size: AbsoluteLength::Rems(rems(1.)),
        children: Vec::new(),
    }
}

impl Styled for Space {
    fn style(&mut self) -> &mut StyleRefinement {
        self.div.style()
    }
}

impl ParentElement for Space {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Space {
    fn render(self, _: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
        self.div.gap(self.size).children(self.children)
    }
}

pub fn v_flex() -> Div {
    div().items_center().flex()
}

pub fn h_flex() -> Div {
    div().items_center().flex().flex_col()
}
