# props_md2attr

Extracts enumerated values from the Office MAPI documentation and `[MS-OXPROPS].docx` and generates
a compatible Rust source file. This should make updates easier if a new version introduces new
values.

Usage:

    cargo run -p props_md2attr -- office-developer-client-docs/docs/outlook/mapi [MS-OXPROPS]-210817.docx > msox/src/prop_enums.rs

This application mostly only serves the build process of this workspace.
