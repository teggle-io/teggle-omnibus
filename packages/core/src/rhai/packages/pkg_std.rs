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
        CorePackage::init(lib); // 18k extra in WASM
        BitFieldPackage::init(lib); // 1k extra
        LogicPackage::init(lib); // < 1k extra
        BasicMathPackage::init(lib); // 1k extra
        BasicArrayPackage::init(lib); // 15k extra
        BasicBlobPackage::init(lib); // 7k extra
        BasicMapPackage::init(lib); // 4k extra
        MoreStringPackage::init(lib); // 34k extra
    }
}
