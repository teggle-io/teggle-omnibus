use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str;

use cfg_if::cfg_if;
use rhai::{AST, Engine, EvalAltResult, GlobalRuntimeState, Locked, Map, Module, ModuleResolver, Position, Scope, Shared};
use zip::ZipArchive;

use crate::config::Config;
use crate::result::{map_resolver_err_to_eval_err, ResolverError, ResolverResult};

pub const RHAI_EXTENSION: &'static str = "rhai";
#[cfg(feature = "json_config")]
pub const JSON_EXTENSION: &'static str = "json";

#[cfg(feature = "json_config")]
pub const CFG_FILE: &'static str = "config";

#[cfg(feature = "json_config")]
pub const CFG_KEY_GLOBAL_ENTRYPOINTS: &'static str = "global.entrypoints";


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
    pub fn init(&mut self, engine: &Engine) -> ResolverResult<Option<AST>> {
        self.init_with_scope(engine, Scope::new())
    }

    #[inline(always)]
    #[must_use]
    pub fn init_with_scope(&mut self, engine: &Engine, scope: Scope<'static>) -> ResolverResult<Option<AST>> {
        self.set_scope(scope);

        #[cfg(feature = "json_config")]
        self.load_config(engine)?;

        cfg_if! {
            if #[cfg(feature = "json_config")] {
                self.compile_entrypoints(engine)
            } else {
                Ok(None)
            }
        }
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

    #[inline(always)]
    pub fn get_source_path(&self, path: &str, source_path: Option<&str>) -> PathBuf {
        self.get_file_path(path, source_path,
                              Some(RHAI_EXTENSION.to_string()))
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

    #[inline(always)]
    pub fn compile(&self, engine: &Engine, source: String) -> ResolverResult<AST> {
        let mut scope = Scope::new();

        self.compile_with_scope(&mut scope, engine, source)
    }

    pub fn compile_with_scope(&self, scope: &mut Scope, engine: &Engine,
                              source: String) -> ResolverResult<AST> {
        return match split_source_const(&source) {
            None => {
                engine.compile_with_scope(scope, &source).map_err(|err| {
                    ResolverError::ParseError(err)
                })
            }
            Some((consts, body)) => {
                // Load const into scope and discard AST.
                engine.compile_with_scope(scope, &consts).map_err(|err| {
                    ResolverError::ParseError(err)
                })?;

                // Compile main body as AST.
                engine.compile_with_scope(scope, &body).map_err(|err| {
                    ResolverError::ParseError(err)
                })
            }
        };
    }

    pub fn compile_path_with_scope(&self, path: String, scope: &mut Scope,
                                   engine: &Engine) -> ResolverResult<AST> {
        let source_path = self.get_source_path(path.as_str(), None);

        let source = self.get_file(source_path.clone())?;

        self.compile_with_scope(scope, engine, source)
            .map_err(|err| {
                ResolverError::SourceCompileFailed(source_path.to_str().unwrap().to_string(),
                                                   Box::new(err))
            })
    }

    #[cfg(feature = "json_config")]
    pub fn compile_entrypoints(&mut self, engine: &Engine) -> ResolverResult<Option<AST>> {
        if !self.loaded() {
            return Err(ResolverError::NotReady);
        }

        let mut scope = self.scope.to_owned();

        return match self.config.as_ref().unwrap().get_str_array(CFG_KEY_GLOBAL_ENTRYPOINTS) {
            Some(entrypoints) => {
                let mut ast: Option<AST> = None;
                for name in entrypoints {
                    let cur_ast = self.compile_path_with_scope(name, &mut scope,
                                                          engine)?;
                    if ast.is_some() {
                        ast = Some(ast.unwrap().merge(&cur_ast));
                    } else {
                        ast = Some(cur_ast);
                    }
                }

                self.set_scope(scope);

                Ok(ast)
            }
            None => Ok(None)
        };
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

        // Clone to avoid importing any module consts.
        let mut scope = self.scope.clone();

        let mut ast = self.compile_with_scope(&mut scope, engine, script)
            .map_err(|err| {
                EvalAltResult::ErrorInModule(path.to_string(),
                                             Box::new(map_resolver_err_to_eval_err(err)), pos)
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

// Source
fn split_source_const(source: &String) -> Option<(String, String)> {
    return match split_source(source, "fn ") {
        None => None,
        Some((preamble, body)) => {
            return match split_source(&preamble, "const ") {
                None => None,
                Some((before_const, consts)) => {
                    let mut full_body = before_const.clone();
                    full_body.push_str(body.as_str());

                    Some((consts, full_body))
                }
            };
        }
    };
}

#[inline(always)]
fn split_source(source: &String, pat: &str) -> Option<(String, String)> {
    return match source.find(pat) {
        None => None,
        Some(n) => {
            let (a, b) = source.split_at(n);

            Some((a.to_string(), b.to_string()))
        }
    };
}

