// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! Embeds the icon and version information in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../packaging/icon.ico");
        res.set("ProductName", "X-COPY Modern");
        res.set("FileDescription", "X-COPY Modern - Retro Computing Club");
        if let Err(e) = res.compile() {
            println!("cargo:warning=Windows icon not embedded: {e}");
        }
    }
}
