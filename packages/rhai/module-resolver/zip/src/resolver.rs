use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str;

use rhai::{AST, Engine, EvalAltResult, GlobalRuntimeState, Locked, Map, Module, ModuleResolver, Position, Scope, Shared};
use zip::ZipArchive;

use crate::config::Config;
use crate::result::{ResolverError, ResolverResult};

pub const RHAI_EXTENSION: &'static str = "rhai";
#[cfg(feature = "json_config")]
pub const JSON_EXTENSION: &'static str = "json";

pub const MAIN_FILE: &'static str = "main";
#[cfg(feature = "json_config")]
pub const CFG_FILE: &'static str = "config";

// Define a custom module resolver.
#[derive(Debug, Clone)]
pub struct ZipModuleResolver {
    zip: RefCell<Option<ZipArchive<Cursor<Vec<u8>>>>>,
    scope: Scope<'static>,
    #[cfg(feature = "json_config")]
    config: Option<Config>,
    base_path: Option<PathBuf>,
    extension: String,
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
        Self::new_with_extension(RHAI_EXTENSION.to_string())
    }

    #[inline(always)]
    #[must_use]
    pub fn new_with_scope(scope: Scope<'static>) -> Self {
        Self {
            zip: RefCell::new(None),
            scope,
            #[cfg(feature = "json_config")]
            config: None,
            base_path: None,
            extension: RHAI_EXTENSION.to_string(),
            cache_enabled: true,
            cache: BTreeMap::new().into(),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn new_with_extension(extension: String) -> Self {
        Self {
            zip: RefCell::new(None),
            scope: Scope::new(),
            #[cfg(feature = "json_config")]
            config: None,
            base_path: None,
            extension: extension,
            cache_enabled: true,
            cache: BTreeMap::new().into(),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn new_with_path_and_extension(
        path: impl Into<PathBuf>,
        extension: String,
    ) -> Self {
        Self {
            zip: RefCell::new(None),
            scope: Scope::new(),
            #[cfg(feature = "json_config")]
            config: None,
            base_path: Some(path.into()),
            extension: extension,
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

    #[inline(always)]
    #[must_use]
    pub fn load_from_bytes_and_init(&mut self, bytes: Vec<u8>) -> ResolverResult<()> {
        self.load_from_bytes(bytes)?;
        self.init()
    }

    #[inline(always)]
    #[must_use]
    pub fn init(&mut self) -> ResolverResult<()> {
        let engine = Engine::new_raw();

        #[cfg(feature = "json_config")]
        self.load_config(&engine)?;

        Ok(())
    }

    /// Get the scope.
    #[inline(always)]
    #[must_use]
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Set the scope.
    #[inline(always)]
    pub fn set_scope(&mut self, scope: Scope<'static>) -> &mut Self {
        self.scope = scope;
        self
    }

    /// Get the scope.
    #[inline(always)]
    #[must_use]
    #[cfg(feature = "json_config")]
    pub fn config(&self) -> Config {
        self.config.as_ref().unwrap().clone()
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
    pub fn set_extension(&mut self, extension: String) -> &mut Self {
        self.extension = extension;
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

        let file_path = self.get_file_path(path.as_ref(), source_path, None);

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
        let file_path = self.get_file_path(path.as_ref(),
                                           source_path.as_ref().map(<_>::as_ref),
                                           None);

        locked_write(&self.cache)
            .remove_entry(&file_path)
            .map(|(.., v)| v)
    }

    #[inline(always)]
    #[cfg(feature = "json_config")]
    pub fn load_config_source(&mut self) -> ResolverResult<String> {
        if !self.loaded() {
            return Err(ResolverError::NotReady);
        }

        self.get_file(self.get_file_path(CFG_FILE, None,
                                         Some(JSON_EXTENSION.to_string())))
    }

    #[cfg(feature = "json_config")]
    pub fn load_config(&mut self, engine: &Engine) -> ResolverResult<()> {
        let cfg_map = self.load_json_with_engine(CFG_FILE, engine)?;

        self.config = Some(Config::new(cfg_map));

        Ok(())
    }

    #[inline(always)]
    pub fn load_json(&mut self, path: &str) -> ResolverResult<Map> {
        let engine = Engine::new_raw();

        return self.load_json_with_engine(path, &engine);
    }

    pub fn load_json_with_engine(&mut self, path: &str, engine: &Engine) -> ResolverResult<Map> {
        let json_source = self.get_file(
            self.get_file_path(path, None,
                               Some(JSON_EXTENSION.to_string())))?;
        let json_map = engine.parse_json(json_source, true).map_err(|err| {
            ResolverError::JsonParseFailed(err.to_string())
        })?;

        Ok(json_map)
    }

    #[must_use]
    pub fn get_file_path(&self, path: &str, source_path: Option<&str>,
                         custom_extension: Option<String>) -> PathBuf {
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

        if custom_extension.is_some() {
            file_path.set_extension(custom_extension.unwrap());
        } else {
            file_path.set_extension(self.extension.as_str());
        }
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
            .map(|p| p.as_ref()), None);

        if self.is_cache_enabled() {
            #[cfg(not(feature = "sync"))]
                let c = self.cache.borrow();
            #[cfg(feature = "sync")]
                let c = self.cache.read().unwrap();

            if let Some(module) = c.get(&file_path) {
                return Ok(module.clone());
            }
        }

        let script = self.get_file(file_path.clone())
            .map_err(|_err| {
                Box::new(EvalAltResult::ErrorModuleNotFound(path.to_string(), pos))
            })?;

        let mut ast = engine
            .compile_with_scope(&self.scope, script)
            .map_err(|err| match err {
                _ => Box::new(
                    EvalAltResult::ErrorInModule(path.to_string(),
                                                 Box::new(EvalAltResult::from(err)), pos)
                ),
            })?;

        ast.set_source(path);

        let scope = Scope::new();

        let m: Shared<Module> = if let Some(_global) = global {
            Module::eval_ast_as_new(scope, &ast, engine)
            // TODO: this needs to be made public.
            //Module::eval_ast_as_new_raw(engine, scope, global, &ast)
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
        let file_path = self.get_file_path(path, source_path, None);

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
