# uic-macros

`uic-macros` provides general-purpose procedural macros that can be used
independently of GPUI.

## `file_enum`

`file_enum` scans one directory at compile time and generates a unit variant for
each file with the requested extension:

```rust
use uic_macros::file_enum;

#[file_enum(
    path = "assets/icons",
    ext = "svg",
    rename_all = "PascalCase",
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {}
```

For files named `arrow-left.svg` and `volume-2.svg`, this creates
`Icon::ArrowLeft` and `Icon::Volume2`. The generated `Display` implementation
returns `arrow-left.svg` and `volume-2.svg`, including the extension.

`path` is resolved relative to the consuming crate's `CARGO_MANIFEST_DIR`.
`rename_all` is optional and defaults to `PascalCase`. Supported rules are:

- `PascalCase` or `UpperCamelCase`
- `camelCase` or `lowerCamelCase`
- `snake_case`
- `SCREAMING_SNAKE_CASE`

The enum must be empty and cannot have generic parameters. The macro reports
invalid Rust identifiers and collisions caused by name conversion as compile
errors.

Existing matching files are registered as compiler inputs. Stable Rust does not
currently let a procedural macro track additions to a directory, so adding a
new file may require touching the enum's source file or rebuilding the consuming
crate.
