#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DropdownPlacement {
    #[default]
    BottomStart,
    BottomEnd,
    TopStart,
    TopEnd,
}
