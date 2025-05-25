# tnef_docx2attr

Extracts enumerated values from the DOCX version of `[MS-OXTNEF]` and generates a compatible Rust
source file. This should make updates easier if a new version introduces new values.

Usage:

    cargo run -p tnef_docx2attr -- "[MS-OXTNEF]-220215.docx" > msox/src/tnef_enums.rs

This application mostly only serves the build process of this workspace.
