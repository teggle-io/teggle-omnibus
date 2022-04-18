use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str;

use rhai::{AST, Engine, EvalAltResult, GlobalRuntimeState, Identifier, Locked, Module, ModuleResolver, Position, Scope, Shared};
use zip::ZipArchive;

use crate::result::{ResolverError, ResolverResult};

pub const RHAI_SCRIPT_EXTENSION: &str = "rhai";

// Define a custom module resolver.
pub struct ZipModuleResolver {
    zip: RefCell<Option<ZipArchive<Cursor<Vec<u8>>>>>,
    base_path: Option<PathBuf>,
    extension: Identifier,
    cache_enabled: bool,

    #[cfg(not(feature = "sync"))]
    cache: RefCell<BTreeMap<PathBuf, Shared<Module>>>,
    #[cfg(feature = "sync")]
    cache: std::sync::RwLock<BTreeMap<PathBuf, Shared<Module>>>,
}

impl ZipModuleResolver {
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_extension(RHAI_SCRIPT_EXTENSION)
    }

    #[inline(always)]
    #[must_use]
    pub fn new_with_extension(extension: impl Into<Identifier>) -> Self {
        Self {
            zip: RefCell::new(None),
            base_path: None,
            extension: extension.into(),
            cache_enabled: true,
            cache: BTreeMap::new().into(),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn new_with_path_and_extension(
        path: impl Into<PathBuf>,
        extension: impl Into<Identifier>,
    ) -> Self {
        Self {
            zip: RefCell::new(None),
            base_path: Some(path.into()),
            extension: extension.into(),
            cache_enabled: true,
            cache: BTreeMap::new().into(),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn loaded(&self) -> bool {
        return self.zip.borrow().is_some();
    }

    #[inline]
    pub fn load(&mut self, reader: Cursor<Vec<u8>>) -> ResolverResult<()> {
        match ZipArchive::new(reader) {
            Ok(z) => {
                self.zip = RefCell::new(Some(z));

                Ok(())
            }
            Err(err) => {
                Err(ResolverError::InvalidZip(err))
            }
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn load_from_bytes(&mut self, bytes: Vec<u8>) -> ResolverResult<()> {
        return self.load(Cursor::new(bytes));
    }

    /// Get the base path for script files.
    #[inline(always)]
    #[must_use]
    pub fn base_path(&self) -> Option<&Path> {
        self.base_path.as_ref().map(PathBuf::as_ref)
    }

    /// Set the base path for script files.
    #[inline(always)]
    pub fn set_base_path(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.base_path = Some(path.into());
        self
    }

    /// Get the script file extension.
    #[inline(always)]
    #[must_use]
    pub fn extension(&self) -> &str {
        &self.extension
    }

    /// Set the script file extension.
    #[inline(always)]
    pub fn set_extension(&mut self, extension: impl Into<Identifier>) -> &mut Self {
        self.extension = extension.into();
        self
    }

    /// Enable/disable the cache.
    #[inline(always)]
    pub fn enable_cache(&mut self, enable: bool) -> &mut Self {
        self.cache_enabled = enable;
        self
    }
    /// Is the cache enabled?
    #[inline(always)]
    #[must_use]
    pub fn is_cache_enabled(&self) -> bool {
        self.cache_enabled
    }

    /// Is a particular path cached?
    #[inline]
    #[must_use]
    pub fn is_cached(&self, path: impl AsRef<str>, source_path: Option<&str>) -> bool {
        if !self.cache_enabled {
            return false;
        }

        let file_path = self.get_file_path(path.as_ref(), source_path);

        let cache = locked_read(&self.cache);

        if !cache.is_empty() {
            cache.contains_key(&file_path)
        } else {
            false
        }
    }

    /// Empty the internal cache.
    #[inline]
    pub fn clear_cache(&mut self) -> &mut Self {
        locked_write(&self.cache).clear();
        self
    }

    #[inline]
    #[must_use]
    pub fn clear_cache_for_path(
        &mut self,
        path: impl AsRef<str>,
        source_path: Option<impl AsRef<str>>,
    ) -> Option<Shared<Module>> {
        let file_path = self.get_file_path(path.as_ref(), source_path.as_ref().map(<_>::as_ref));

        locked_write(&self.cache)
            .remove_entry(&file_path)
            .map(|(.., v)| v)
    }

    #[must_use]
    pub fn get_file_path(&self, path: &str, source_path: Option<&str>) -> PathBuf {
        let path = Path::new(path);

        let mut file_path;

        if path.is_relative() {
            file_path = self
                .base_path
                .clone()
                .or_else(|| source_path.map(|p| p.into()))
                .unwrap_or_default();
            file_path.push(path);
        } else {
            file_path = path.into();
        }

        file_path.set_extension(self.extension.as_str()); // Force extension
        file_path
    }

    #[inline]
    pub fn get_file(&self, file_path: PathBuf) -> ResolverResult<String> {
        if !self.loaded() {
            return Err(ResolverError::NotReady);
        }

        let mut zip_rc = RefCell::borrow_mut(&self.zip);
        let zip = zip_rc.as_mut().unwrap();
        let mut file = zip.by_name(file_path.to_str().unwrap())
            .map_err(|_err| {
                ResolverError::FileNotFound
            })?;

        let mut string = String::new();

        match file.read_to_string(&mut string) {
            Ok(_) => {
                Ok(string)
            }
            Err(err) => {
                Err(ResolverError::FileReadFailed(err))
            }
        }
    }

    /// Resolve a module based on a path.
    fn impl_resolve(
        &self,
        engine: &Engine,
        global: Option<&mut GlobalRuntimeState>,
        source: Option<&str>,
        path: &str,
        pos: Position,
    ) -> Result<Rc<Module>, Box<EvalAltResult>> {
        // Load relative paths from source if there is no base path specified
        let source_path = global
            .as_ref()
            .and_then(|g| g.source())
            .or(source)
            .and_then(|p| Path::new(p).parent().map(|p| p.to_string_lossy()));

        let file_path = self.get_file_path(path, source_path.as_ref()
            .map(|p| p.as_ref()));

        if self.is_cache_enabled() {
            #[cfg(not(feature = "sync"))]
                let c = self.cache.borrow();
            #[cfg(feature = "sync")]
                let c = self.cache.read().unwrap();

            if let Some(module) = c.get(&file_path) {
                return Ok(module.clone());
            }
        }

        let scope = Scope::new();

        let script = self.get_file(file_path.clone())
            .map_err(|_err| {
                Box::new(EvalAltResult::ErrorModuleNotFound(path.to_string(), pos))
            })?;

        let mut ast = engine
            .compile(script)
            .map_err(|err| match err {
                _ => Box::new(
                    EvalAltResult::ErrorInModule(path.to_string(),
                                                 Box::new(EvalAltResult::from(err)), pos)
                ),
            })?;

        ast.set_source(path);

        let m: Shared<Module> = if let Some(global) = global {
            Module::eval_ast_as_new_raw(engine, scope, global, &ast)
        } else {
            Module::eval_ast_as_new(scope, &ast, engine)
        }
            .map_err(|err| Box::new(
                EvalAltResult::ErrorInModule(path.to_string(), err, pos)
            ))?
            .into();

        if self.is_cache_enabled() {
            locked_write(&self.cache).insert(file_path, m.clone());
        }

        Ok(m)
    }
}

// Implement the 'ModuleResolver' trait.
impl ModuleResolver for ZipModuleResolver {
    #[inline(always)]
    fn resolve(
        &self,
        engine: &Engine,
        source: Option<&str>,
        path: &str,
        pos: Position,
    ) -> Result<Rc<Module>, Box<EvalAltResult>> {
        self.impl_resolve(engine, None, source, path, pos)
    }

    fn resolve_raw(
        &self,
        engine: &Engine,
        global: &mut GlobalRuntimeState,
        path: &str,
        pos: Position,
    ) -> Result<Rc<Module>, Box<EvalAltResult>> {
        self.impl_resolve(engine, Some(global), None, path, pos)
    }

    /// Resolve an `AST` based on a path string.
    ///
    /// The file system is accessed during each call; the internal cache is by-passed.
    fn resolve_ast(
        &self,
        engine: &Engine,
        source_path: Option<&str>,
        path: &str,
        pos: Position,
    ) -> Option<Result<AST, Box<EvalAltResult>>> {
        // Construct the script file path
        let file_path = self.get_file_path(path, source_path);

        let script = self.get_file(file_path.clone())
            .map_err(|_err| {
                Box::new(EvalAltResult::ErrorModuleNotFound(path.to_string(), pos))
            }).ok().unwrap();

        // Load the script file and compile it
        Some(
            engine
                .compile(script)
                .map(|mut ast| {
                    ast.set_source(path);
                    ast
                })
                .map_err(|err| match err {
                    _ => EvalAltResult::ErrorInModule(path.to_string(),
                                                      Box::new(EvalAltResult::from(err)), pos).into(),
                }),
        )
    }
}

// Util
// borrowed
/// Read-only lock guard for synchronized shared object.
#[cfg(not(feature = "sync"))]
#[allow(dead_code)]
pub type LockGuard<'a, T> = std::cell::Ref<'a, T>;

/// Mutable lock guard for synchronized shared object.
#[cfg(not(feature = "sync"))]
#[allow(dead_code)]
pub type LockGuardMut<'a, T> = std::cell::RefMut<'a, T>;

#[inline(always)]
#[must_use]
#[allow(dead_code)]
pub fn locked_read<T>(value: &Locked<T>) -> LockGuard<T> {
    #[cfg(not(feature = "sync"))]
    return value.borrow();

    #[cfg(feature = "sync")]
    return value.read().unwrap();
}

/// Lock a [`Locked`] resource for mutable access.
#[inline(always)]
#[must_use]
#[allow(dead_code)]
pub fn locked_write<T>(value: &Locked<T>) -> LockGuardMut<T> {
    #[cfg(not(feature = "sync"))]
    return value.borrow_mut();

    #[cfg(feature = "sync")]
    return value.write().unwrap();
}
