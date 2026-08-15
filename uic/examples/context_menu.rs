use gpui::{
    Context, Entity, IntoElement, Render, Window, WindowOptions, div, prelude::*, px, rgb, rgba,
};
use gpui_effects::{GlassMaterial, GlassPanel};
use uic::components::context_menu::{self, ContextMenu, ContextMenuAppearance, ContextMenuExt};

#[derive(Clone, Copy)]
enum MenuMaterial {
    Liquid,
    Frosted,
    Plain,
}

impl MenuMaterial {
    fn label(self) -> &'static str {
        match self {
            Self::Liquid => "Liquid glass",
            Self::Frosted => "Frosted glass",
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
        let liquid_entity = entity.clone();
        let frosted_entity = entity.clone();
        let plain_entity = entity.clone();
        let rename_entity = entity.clone();
        let archive_entity = entity.clone();

        ContextMenu::new()
            .appearance(ContextMenuAppearance {
                background: rgb(0x111827).into(),
                foreground: rgb(0xf8fafc).into(),
                muted_foreground: rgb(0x94a3b8).into(),
                danger_foreground: rgb(0xfb7185).into(),
                selected_background: rgb(0x334155).into(),
                selected_foreground: rgb(0xffffff).into(),
                border: rgb(0x64748b).into(),
                separator: rgb(0x64748b).into(),
                ..ContextMenuAppearance::default()
            })
            .action_with_shortcut("Open", "Enter", |_, _| {})
            .submenu("Material", move |menu| {
                menu.action("Liquid glass", move |window, cx| {
                    Self::set_material(&liquid_entity, MenuMaterial::Liquid, window, cx);
                })
                .action("Frosted glass", move |window, cx| {
                    Self::set_material(&frosted_entity, MenuMaterial::Frosted, window, cx);
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
                MenuMaterial::Liquid => GlassPanel::liquid()
                    .id(("context-menu-liquid", state.session_id))
                    .material(GlassMaterial::Thick)
                    .tint(rgba(0x0a1222a8))
                    .optics([6.0, 0.15, 0.12, 1.02])
                    .surface([0.08, 0.14, 0.0, 1.0])
                    .deformation(0.18)
                    .edge_color(rgba(0xffffff28))
                    .rounded(px(16.))
                    .shadow_lg()
                    .child(content)
                    .into_any_element(),
                MenuMaterial::Frosted => GlassPanel::frosted()
                    .id(("context-menu-frosted", state.session_id))
                    .material(GlassMaterial::Thick)
                    .tint(rgba(0x101827a8))
                    .edge_color(rgba(0xffffff20))
                    .rounded(px(14.))
                    .shadow_lg()
                    .child(content)
                    .into_any_element(),
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
                GlassPanel::frosted()
                    .id((
                        "context-submenu-frosted",
                        state.session_id * 10 + state.depth as u64,
                    ))
                    .material(GlassMaterial::Thick)
                    .tint(rgba(0x111a2aee))
                    .edge_color(rgba(0xffffff20))
                    .rounded(px(13.))
                    .shadow_lg()
                    .child(content)
            })
            .surface_for_depth(2, |_, content, _, _| {
                div()
                    .rounded(px(11.))
                    .border_1()
                    .border_color(rgb(0x475569))
                    .bg(rgb(0x1e293b))
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
                material: MenuMaterial::Liquid,
                last_action: "None",
            })
        })
        .expect("failed to open context menu example window");
    });
}
