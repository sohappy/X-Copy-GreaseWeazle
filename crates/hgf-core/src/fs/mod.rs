// SPDX-License-Identifier: GPL-3.0-or-later

//! Floppy filesystems: blank disk images for every supported machine, and AmigaDOS analysis.
//!
//! Every formatter implements [`BlankDisk`], so front-ends can validate, name and build any
//! blank disk the same way (`Box<dyn BlankDisk>`), whatever the machine.

pub mod acorndfs;
pub mod amigados;
pub mod apple2;
pub mod bytes;
pub mod cbmdos;
pub mod fat12;

/// A blank disk that can be built in memory and written with `gw write --format …`.
pub trait BlankDisk {
    /// Check the user's input (volume name, label, ID…) before anything is written.
    fn validate(&self) -> Result<(), String>;
    /// Format passed to `gw --format`.
    fn gw_format(&self) -> &'static str;
    /// Usual extension of the image file.
    fn extension(&self) -> &'static str;
    /// Short human description (machine, size…).
    fn describe(&self) -> String;
    /// The image, with date stamps for `unix_secs` (fixed in tests).
    fn image_at(&self, unix_secs: u64) -> Result<Vec<u8>, String>;
    /// The image, dated now.
    fn image(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        self.image_at(bytes::now_secs())
    }
}

impl BlankDisk for amigados::FormatOpts {
    fn validate(&self) -> Result<(), String> {
        amigados::validate_name(&self.name)
    }
    fn gw_format(&self) -> &'static str {
        amigados::FormatOpts::gw_format(self)
    }
    fn extension(&self) -> &'static str {
        "adf"
    }
    fn describe(&self) -> String {
        format!("Amiga {} {}", if self.hd { "HD 1.76M" } else { "DD 880K" }, self.filesystem.label())
    }
    fn image_at(&self, unix_secs: u64) -> Result<Vec<u8>, String> {
        amigados::blank_image_at(self, unix_secs)
    }
}

impl BlankDisk for fat12::FatOpts {
    fn validate(&self) -> Result<(), String> {
        fat12::validate_label(&self.label)
    }
    fn gw_format(&self) -> &'static str {
        self.kind.layout().gw_format
    }
    fn extension(&self) -> &'static str {
        self.kind.layout().extension
    }
    fn describe(&self) -> String {
        format!("{} {}", self.system.label(), self.kind.layout().label)
    }
    fn image_at(&self, unix_secs: u64) -> Result<Vec<u8>, String> {
        fat12::blank_image_at(self, unix_secs)
    }
}

impl BlankDisk for cbmdos::CbmOpts {
    fn validate(&self) -> Result<(), String> {
        cbmdos::validate_name(&self.name)?;
        cbmdos::validate_id(&self.id)
    }
    fn gw_format(&self) -> &'static str {
        self.drive.gw_format()
    }
    fn extension(&self) -> &'static str {
        self.drive.extension()
    }
    fn describe(&self) -> String {
        format!("Commodore {}", self.drive.label())
    }
    fn image_at(&self, _unix_secs: u64) -> Result<Vec<u8>, String> {
        cbmdos::blank_image(self)
    }
}

impl BlankDisk for acorndfs::DfsOpts {
    fn validate(&self) -> Result<(), String> {
        acorndfs::validate_title(&self.title)
    }
    fn gw_format(&self) -> &'static str {
        acorndfs::DfsOpts::gw_format(self)
    }
    fn extension(&self) -> &'static str {
        acorndfs::DfsOpts::extension(self)
    }
    fn describe(&self) -> String {
        format!("Acorn DFS {}", self.label())
    }
    fn image_at(&self, _unix_secs: u64) -> Result<Vec<u8>, String> {
        acorndfs::blank_image(self)
    }
}

impl BlankDisk for apple2::AppleOpts {
    fn validate(&self) -> Result<(), String> {
        apple2::validate(self)
    }
    fn gw_format(&self) -> &'static str {
        self.os.gw_format()
    }
    fn extension(&self) -> &'static str {
        self.os.extension()
    }
    fn describe(&self) -> String {
        format!("Apple II {} 140K", self.os.label())
    }
    fn image_at(&self, unix_secs: u64) -> Result<Vec<u8>, String> {
        apple2::blank_image(self, unix_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every formatter goes through the same interface.
    #[test]
    fn every_blank_disk_builds_through_the_trait() {
        let disks: Vec<Box<dyn BlankDisk>> = vec![
            Box::new(amigados::FormatOpts::default()),
            Box::new(fat12::FatOpts::default()),
            Box::new(cbmdos::CbmOpts::default()),
            Box::new(acorndfs::DfsOpts { title: "BLANK".into(), ..Default::default() }),
            Box::new(apple2::AppleOpts::default()),
        ];
        for d in &disks {
            d.validate().unwrap();
            let img = d.image_at(0).unwrap();
            assert!(!img.is_empty(), "{}", d.describe());
            assert!(!d.gw_format().is_empty() && !d.extension().is_empty());
        }
        assert_eq!(disks[0].image_at(0).unwrap().len(), 901_120);
    }

    #[test]
    fn invalid_input_is_refused_before_building() {
        let bad = amigados::FormatOpts { name: "a:b".into(), ..Default::default() };
        assert!(BlankDisk::image(&bad).is_err());
    }
}
