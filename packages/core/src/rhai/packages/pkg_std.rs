use std::prelude::v1::*;

use rhai::def_package;
use rhai::packages::{BasicArrayPackage, BasicBlobPackage, BasicMapPackage, BasicMathPackage, BitFieldPackage, CorePackage, LogicPackage, MoreStringPackage};

def_package! {
    /// Standard package containing all built-in features.
    ///
    /// # Contents
    ///
    /// * [`CorePackage`][super::CorePackage]
    /// * [`BitFieldPackage`][super::BitFieldPackage]
    /// * [`LogicPackage`][super::LogicPackage]
    /// * [`BasicMathPackage`][super::BasicMathPackage]
    /// * [`BasicArrayPackage`][super::BasicArrayPackage]
    /// * [`BasicBlobPackage`][super::BasicBlobPackage]
    /// * [`BasicMapPackage`][super::BasicMapPackage]
    /// * [`BasicTimePackage`][super::BasicTimePackage]
    /// * [`MoreStringPackage`][super::MoreStringPackage]
    pub StandardPackage(lib) {
        CorePackage::init(lib);
        BitFieldPackage::init(lib);
        LogicPackage::init(lib);
        BasicMathPackage::init(lib);
        BasicArrayPackage::init(lib);
        BasicBlobPackage::init(lib);
        BasicMapPackage::init(lib);
        MoreStringPackage::init(lib);
    }
}
