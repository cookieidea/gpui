use crate::{MacPlatform, ns_string};
use anyhow::{Context as _, Result, anyhow};
use cocoa::{
    appkit::{
        NSControl as _, NSEventType, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
        NSVisualEffectState, NSVisualEffectView,
    },
    base::{NO, YES, id, nil},
    foundation::{NSAutoreleasePool, NSData, NSInteger, NSUInteger},
};
use ctor::ctor;
use gpui::{Action, Menu, MenuItem, TrayId, TrayOptions};
use image::{ExtendedColorType, ImageEncoder as _, codecs::png::PngEncoder};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{ffi::c_void, ptr};

const PLATFORM_IVAR: &str = "platform";
const TRAY_ID_IVAR: &str = "trayId";
static mut TRAY_TARGET_CLASS: *const Class = ptr::null();

#[ctor(unsafe)]
unsafe fn build_tray_target_class() {
    unsafe {
        let mut decl = ClassDecl::new("GPUITrayTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>(PLATFORM_IVAR);
        decl.add_ivar::<u32>(TRAY_ID_IVAR);
        decl.add_method(
            sel!(handleGPUITrayActivate:),
            handle_tray_activate as extern "C" fn(&mut Object, Sel, id),
        );
        decl.add_method(
            sel!(handleGPUITrayMenuItem:),
            handle_tray_menu_item as extern "C" fn(&mut Object, Sel, id),
        );
        TRAY_TARGET_CLASS = decl.register();
    }
}

pub(crate) struct MacTray {
    status_item: id,
    target: id,
    menu: id,
    actions: Vec<Box<dyn Action>>,
    activate: Option<Box<dyn Action>>,
}

impl MacTray {
    pub(crate) fn new(
        platform: *const MacPlatform,
        id: TrayId,
        options: TrayOptions,
    ) -> Result<Self> {
        unsafe {
            let image = create_image(&options)?;
            let status_bar = NSStatusBar::systemStatusBar(nil);
            let status_item = status_bar.statusItemWithLength_(-1.0);
            if status_item.is_null() {
                let _: () = msg_send![image, release];
                anyhow::bail!("creating NSStatusItem failed");
            }

            let target: id = msg_send![TRAY_TARGET_CLASS, new];
            (*target).set_ivar(PLATFORM_IVAR, platform as *mut c_void);
            (*target).set_ivar(TRAY_ID_IVAR, id.as_u32());

            let button = status_item.button();
            let _: () = msg_send![button, setImage: image];
            let _: () = msg_send![image, release];
            if let Some(tooltip) = &options.tooltip {
                let _: () = msg_send![button, setToolTip: ns_string(tooltip)];
            }
            let _: () = msg_send![button, setTarget: target];
            let _: () = msg_send![button, setAction: sel!(handleGPUITrayActivate:)];
            let mouse_up_mask: NSUInteger = (1 << 2) | (1 << 4);
            let _: NSUInteger = msg_send![button, sendActionOn: mouse_up_mask];

            let mut actions = Vec::new();
            let menu = create_menu(&options.menu, target, &mut actions);
            Ok(Self {
                status_item,
                target,
                menu,
                actions,
                activate: options.activate,
            })
        }
    }

    pub(crate) fn activation_action(&self) -> Option<Box<dyn Action>> {
        self.activate.as_ref().map(|action| action.boxed_clone())
    }

    pub(crate) fn menu_action(&self, index: usize) -> Option<Box<dyn Action>> {
        self.actions.get(index).map(|action| action.boxed_clone())
    }

    pub(crate) fn menu_handles(&self) -> (id, id) {
        (self.status_item, self.menu)
    }
}

impl Drop for MacTray {
    fn drop(&mut self) {
        unsafe {
            let status_bar = NSStatusBar::systemStatusBar(nil);
            status_bar.removeStatusItem_(self.status_item);
            let _: () = msg_send![self.menu, release];
            let _: () = msg_send![self.target, release];
        }
    }
}

unsafe fn create_image(options: &TrayOptions) -> Result<id> {
    let image = options
        .icon
        .images()
        .iter()
        .max_by_key(|image| image.width.saturating_mul(image.height))
        .ok_or_else(|| anyhow!("tray icon has no image representations"))?;
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            &image.rgba,
            image.width,
            image.height,
            ExtendedColorType::Rgba8,
        )
        .context("encoding tray icon")?;
    unsafe {
        let data = NSData::dataWithBytes_length_(nil, png.as_ptr().cast(), png.len() as NSUInteger);
        let native_image: id = msg_send![class!(NSImage), alloc];
        let native_image: id = msg_send![native_image, initWithData: data];
        anyhow::ensure!(
            !native_image.is_null(),
            "decoding tray icon as NSImage failed"
        );
        let _: () =
            msg_send![native_image, setTemplate: if options.icon.is_template() { YES } else { NO }];
        Ok(native_image)
    }
}

unsafe fn create_menu(items: &[MenuItem], target: id, actions: &mut Vec<Box<dyn Action>>) -> id {
    unsafe {
        let menu = NSMenu::new(nil);
        for item in items {
            match item {
                MenuItem::Separator => menu.addItem_(NSMenuItem::separatorItem(nil)),
                MenuItem::Label(label) => {
                    let item = NSMenuItem::new(nil).autorelease();
                    let _: () = msg_send![item, setTitle: ns_string(label)];
                    item.setEnabled_(NO);
                    menu.addItem_(item);
                }
                MenuItem::Action {
                    name,
                    action,
                    checked,
                    disabled,
                    ..
                } => {
                    let item: id = msg_send![class!(NSMenuItem), alloc];
                    let item: id = msg_send![item,
                        initWithTitle: ns_string(name)
                        action: sel!(handleGPUITrayMenuItem:)
                        keyEquivalent: ns_string("")
                    ];
                    let _: () = msg_send![item, setTarget: target];
                    let tag = actions.len() as NSInteger;
                    let _: () = msg_send![item, setTag: tag];
                    item.setEnabled_(if *disabled { NO } else { YES });
                    if *checked {
                        item.setState_(NSVisualEffectState::Active);
                    }
                    actions.push(action.boxed_clone());
                    menu.addItem_(item.autorelease());
                }
                MenuItem::Submenu(Menu {
                    name,
                    items,
                    disabled,
                }) => {
                    let item = NSMenuItem::new(nil).autorelease();
                    let _: () = msg_send![item, setTitle: ns_string(name)];
                    item.setEnabled_(if *disabled { NO } else { YES });
                    let submenu = create_menu(items, target, actions).autorelease();
                    item.setSubmenu_(submenu);
                    menu.addItem_(item);
                }
                MenuItem::SystemMenu(_) => {}
            }
        }
        menu
    }
}

extern "C" fn handle_tray_activate(this: &mut Object, _: Sel, _: id) {
    unsafe {
        let platform = platform_from_target(this);
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let event: id = msg_send![app, currentEvent];
        let event_type: NSEventType = msg_send![event, type];
        platform.handle_tray_activation(
            tray_id_from_target(this),
            event_type == NSEventType::NSRightMouseUp,
        );
    }
}

extern "C" fn handle_tray_menu_item(this: &mut Object, _: Sel, item: id) {
    unsafe {
        let tag: NSInteger = msg_send![item, tag];
        platform_from_target(this).handle_tray_menu_action(tray_id_from_target(this), tag as usize);
    }
}

unsafe fn platform_from_target(target: &Object) -> &MacPlatform {
    unsafe {
        let pointer: *mut c_void = *target.get_ivar(PLATFORM_IVAR);
        assert!(!pointer.is_null());
        &*(pointer as *const MacPlatform)
    }
}

unsafe fn tray_id_from_target(target: &Object) -> TrayId {
    unsafe { TrayId::from_u32(*target.get_ivar::<u32>(TRAY_ID_IVAR)) }
}
