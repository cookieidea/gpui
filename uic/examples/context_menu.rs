use gpui::{
    Context, Entity, IntoElement, Render, Window, WindowOptions, div, prelude::*, px, rgb, rgba,
    transparent_black,
};
use gpui_effects::{FrostedGlass, FrostedGlassAppearance};
use uic::components::context_menu::{self, ContextMenu, ContextMenuAppearance, ContextMenuExt};

#[derive(Clone, Copy)]
enum MenuMaterial {
    DarkFrosted,
    LightFrosted,
    Plain,
}

impl MenuMaterial {
    fn label(self) -> &'static str {
        match self {
            Self::DarkFrosted => "Dark frosted",
            Self::LightFrosted => "Light frosted",
            Self::Plain => "Plain div",
        }
    }
}

struct ContextMenuExample {
    material: MenuMaterial,
    last_action: &'static str,
}

impl ContextMenuExample {
    fn set_material(
        entity: &Entity<Self>,
        material: MenuMaterial,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        entity.update(cx, |this, cx| {
            this.material = material;
            this.last_action = material.label();
            cx.notify();
        });
        window.refresh();
    }

    fn menu(entity: Entity<Self>, cx: &gpui::App) -> ContextMenu {
        let material = entity.read(cx).material;
        let dark_entity = entity.clone();
        let light_entity = entity.clone();
        let plain_entity = entity.clone();
        let rename_entity = entity.clone();
        let archive_entity = entity.clone();

        ContextMenu::new()
            .appearance(
                ContextMenuAppearance::default()
                    .muted_foreground(rgb(0x94a3b8).into())
                    .danger_foreground(rgb(0xfb7185).into())
                    .selected_background(rgba(0x94a3b829).into())
                    .selected_foreground(rgb(0xffffff).into())
                    .item_height(px(34.0))
                    .item_padding_x(px(10.0))
                    .item_radius(px(8.0))
                    .separator(rgb(0x64748b).into())
                    .separator_margin(px(5.0)),
            )
            .w(px(220.0))
            .max_h(px(420.0))
            .p(px(8.0))
            .rounded(px(16.0))
            .border(px(0.0))
            .bg(transparent_black())
            .text_color(rgb(0xf8fafc))
            .font_family(".SystemUIFont")
            .action_with_shortcut("Open", "Enter", |_, _| {})
            .submenu("Material", move |menu| {
                menu.action("Dark frosted", move |window, cx| {
                    Self::set_material(&dark_entity, MenuMaterial::DarkFrosted, window, cx);
                })
                .action("Light frosted", move |window, cx| {
                    Self::set_material(&light_entity, MenuMaterial::LightFrosted, window, cx);
                })
                .submenu("More", move |menu| {
                    menu.action("Plain div", move |window, cx| {
                        Self::set_material(&plain_entity, MenuMaterial::Plain, window, cx);
                    })
                    .action("Material settings…", |_, _| {})
                })
            })
            .separator()
            .action("Rename", move |_, cx| {
                rename_entity.update(cx, |this, cx| {
                    this.last_action = "Rename";
                    cx.notify();
                });
            })
            .item(
                uic::components::context_menu::ContextMenuItem::action("Archive", move |_, cx| {
                    archive_entity.update(cx, |this, cx| {
                        this.last_action = "Archive";
                        cx.notify();
                    });
                })
                .danger(),
            )
            .root_surface(move |state, content, _, _| match material {
                MenuMaterial::DarkFrosted => {
                    FrostedGlass::with_appearance(FrostedGlassAppearance::dark())
                        .id(("context-menu-dark-frosted", state.session_id))
                        .rounded(px(16.))
                        .shadow_lg()
                        .child(content)
                        .into_any_element()
                }
                MenuMaterial::LightFrosted => {
                    FrostedGlass::with_appearance(FrostedGlassAppearance::light())
                        .id(("context-menu-light-frosted", state.session_id))
                        .rounded(px(14.))
                        .shadow_lg()
                        .child(content)
                        .into_any_element()
                }
                MenuMaterial::Plain => div()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(rgb(0x334155))
                    .bg(rgb(0x111827))
                    .shadow_lg()
                    .child(content)
                    .into_any_element(),
            })
            .submenu_surface(|state, content, _, _| {
                FrostedGlass::with_appearance(FrostedGlassAppearance::dark())
                    .id((
                        "context-submenu-frosted",
                        state.session_id * 10 + state.depth as u64,
                    ))
                    .rounded(px(13.))
                    .shadow_lg()
                    .child(content)
            })
            .surface_for_depth(2, |_, content, _, _| {
                div()
                    .rounded(px(11.))
                    .border_1()
                    .border_color(rgb(0x475569))
                    .bg(rgb(0x1e294b))
                    .shadow_lg()
                    .child(content)
            })
    }
}

impl Render for ContextMenuExample {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let menu_entity = entity.clone();
        let context_menu_layer = context_menu::layer(cx);

        div()
            .relative()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x0f172a))
            .text_color(rgb(0xf8fafc))
            .child(
                div()
                    .w(px(440.))
                    .p_8()
                    .rounded(px(24.))
                    .border_1()
                    .border_color(rgb(0x334155))
                    .bg(rgb(0x172033))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .context_menu(move |_, cx| Self::menu(menu_entity.clone(), cx))
                    .child(div().text_xl().child("Context menu surfaces"))
                    .child("Right-click this card. The menu supports three levels.")
                    .child(format!("Root material: {}", self.material.label()))
                    .child(format!("Last action: {}", self.last_action)),
            )
            // The layer must be the last child so all menu levels paint above application content.
            .child(context_menu_layer)
    }
}

fn main() {
    gpui_platform::application().run(|cx| {
        uic::init(cx);
        cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|_| ContextMenuExample {
                material: MenuMaterial::DarkFrosted,
                last_action: "None",
            })
        })
        .expect("failed to open context menu example window");
    });
}
