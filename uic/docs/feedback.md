# Feedback

UIC separates brief operation feedback from detailed application notifications.

Toast messages appear at the top center, contain one line of text, and dismiss automatically. Mount the layer once near the end of the window root:

```rust,ignore
use uic::components::toast;

toast::success("Saved successfully", cx);

div()
    .child(application_content)
    .child(toast::layer(cx))
```

The built-in variants are `info`, `success`, `warn`, `error`, and `loading`. Each uses its matching semantic icon. Configure the shared message surface through `ToastAppearance`, which implements `Styled`:

```rust,ignore
toast::init_with_appearance(
    ToastAppearance::default()
        .bg(rgb(0x171717))
        .text_color(rgb(0xf5f5f5))
        .rounded(px(10.)),
    cx,
);
```

Notifications appear at a window corner and provide a title, description, status icon, and close control:

```rust,ignore
use uic::components::notification::{self, Notification, NotificationPlacement};

notification::show(
    Notification::new(
        "Export complete",
        "The archive was written to the selected folder.",
    )
    .success()
    .placement(NotificationPlacement::TopRight)
    .w(px(420.)),
    cx,
);

div()
    .child(application_content)
    .child(notification::layer(cx))
```

Notifications close automatically after 4.5 seconds. Use `.persistent()` when the user should dismiss the card explicitly. `Notification` implements `Styled`, so an individual card can override the shared `NotificationAppearance` surface without changing its semantic icon or interaction behavior.

Run the complete example with:

```sh
cargo run -p uic --example feedback
```
