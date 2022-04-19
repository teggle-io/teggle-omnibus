extern crate zip_module_resolver;

use std::cell::RefCell;
use std::io::Read;
use std::rc::Rc;

use cosmwasm_std::{Api, debug_print, Env, Extern, HandleResponse, Querier, StdError, StdResult, Storage};
use flate2::read::GzDecoder;
use rhai::{AST, Engine, Module, Scope, Shared};
use rhai::packages::Package;
use zip_module_resolver::{RHAI_SCRIPT_EXTENSION, ZipModuleResolver};

use crate::rhai::packages::pkg_std::StandardPackage;

pub const MAIN_FILE: &str = "main";

pub struct OmnibusEngine<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier> {
    rh_engine: Engine,
    rh_resolver: RefCell<Option<ZipModuleResolver>>,
    rh_ast: Option<AST>,
    deps: Rc<RefCell<Extern<S, A, Q>>>,
    label: String,
}

impl<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier> OmnibusEngine<S, A, Q> {
    #[inline(always)]
    pub fn new(
        deps: Rc<RefCell<Extern<S, A, Q>>>,
    ) -> Self {
        let mut engine = Self::new_raw(deps);

        engine.init();
        engine
    }

    #[inline(always)]
    pub fn new_raw(
        deps: Rc<RefCell<Extern<S, A, Q>>>,
    ) -> Self {
        Self {
            rh_engine: Engine::new_raw(),
            rh_resolver: RefCell::new(None),
            rh_ast: None,
            deps,
            label: "cortex.core:v1".to_string(), // TODO: load core config
        }
    }

    #[inline(always)]
    pub fn init(&mut self) {
        self.register_modules()
            .register_handlers()
            .register_functions();
    }

    #[inline(always)]
    pub fn register_modules(&mut self) -> &mut Self {
        self.register_global_module(StandardPackage::new().as_shared_module());

        self
    }

    #[inline(always)]
    pub fn register_global_module(&mut self, module: Shared<Module>) -> &mut Self {
        self.rh_engine.register_global_module(module);
        self
    }

    #[inline]
    pub fn register_handlers(&mut self) -> &mut Self {
        let label = self.label.clone();
        self.rh_engine.on_print(move |text| {
            debug_print!("CORTEX[{}][info ]: {}", label, text);
        });

        let label = self.label.clone();
        self.rh_engine.on_debug(move |text, source, pos| {
            if let Some(source) = source {
                debug_print!("CORTEX[{}][debug]: {} @ {:?} | {}", label, source, pos, text);
            } else if pos.is_none() {
                debug_print!("CORTEX[{}][debug]: {}", label, text);
            } else {
                debug_print!("CORTEX[{}][debug]: {:?} | {}", label, pos, text);
            }
        });

        self
    }

    #[inline]
    pub fn register_functions(&mut self) -> &mut Self {
        let deps = self.deps.clone();
        self.rh_engine.register_fn("storage_set", move |key: &str, val: &str| {
            RefCell::borrow_mut(&*deps).storage.set(key.as_bytes(), val.as_bytes());
        });

        let deps = self.deps.clone();
        self.rh_engine.register_fn("storage_set", move |key: &str, val: &[u8]| {
            RefCell::borrow_mut(&*deps).storage.set(key.as_bytes(), val);
        });

        self
    }

    #[inline(always)]
    pub fn loaded_core(&mut self) -> bool {
        self.rh_resolver.borrow().is_some()
    }

    #[inline(always)]
    pub fn loaded_ast(&mut self) -> bool {
        self.rh_ast.is_some()
    }

    #[inline]
    pub fn load_core(&mut self, bytes: Vec<u8>) -> Result<(), StdError> {
        let mut resolver = ZipModuleResolver::new();
        resolver.load_from_bytes(bytes).map_err(|err| {
            return StdError::GenericErr {
                msg: format!("failed to load core: {err}"),
                backtrace: None,
            };
        })?;

        self.rh_resolver = RefCell::new(Some(resolver.clone()));
        self.rh_engine.set_module_resolver(resolver);
        self.load_main()?;

        Ok(())
    }

    #[inline]
    pub fn get_file(&mut self, path: &str, custom_extension: Option<String>) -> Result<String, StdError> {
        if !self.loaded_core() {
            return Err(StdError::GenericErr {
                msg: format!("can not 'get_file' without a core loaded."),
                backtrace: None,
            });
        }

        let mut rc_resolver = RefCell::borrow_mut(&self.rh_resolver);
        let resolver = rc_resolver.as_mut().unwrap();

        let full_path = resolver.get_file_path(path, None, custom_extension);
        let source = resolver.get_file(full_path)
            .map_err(|err| {
                return StdError::GenericErr {
                    msg: format!("failed to load {}.{} file source: {err}",
                                 MAIN_FILE, RHAI_SCRIPT_EXTENSION),
                    backtrace: None,
                };
            })?;

        Ok(source)
    }

    #[inline]
    pub fn load_script(&mut self, path: &str) -> Result<(), StdError> {
        if !self.loaded_core() {
            return Err(StdError::GenericErr {
                msg: format!("can not 'load_script' without a core loaded."),
                backtrace: None,
            });
        }

        let source = self.get_file(path, None)?;

        self.load_script_raw(source.as_str())
    }

    #[inline(always)]
    pub fn load_main(&mut self) -> Result<(), StdError> {
        self.load_script(MAIN_FILE)
    }

    #[inline]
    pub fn load_script_raw_compressed(&mut self, compressed_bytes: &[u8]) -> Result<(), StdError> {
        let b = decompress_bytes(compressed_bytes)?;
        let s = String::from_utf8(b).map_err(|err| {
            return StdError::GenericErr {
                msg: format!("failed convert rhai script binary to utf8 string: {err}"),
                backtrace: None,
            };
        })?;

        self.load_script_raw(String::as_str(&s))
    }

    #[inline]
    pub fn load_script_raw(&mut self, script: &str) -> Result<(), StdError> {
        let ast: AST = self.rh_engine.compile(script).map_err(|err| {
            return StdError::GenericErr {
                msg: format!("failed to compile rhai script: {err}"),
                backtrace: None,
            };
        })?;

        if self.rh_ast.is_some() {
            self.rh_ast = Some(self.rh_ast.as_mut().unwrap().merge(&ast));
        } else {
            self.rh_ast = Some(ast);
        }

        Ok(())
    }

    #[inline]
    pub fn validate(&mut self) -> Result<(), StdError> {
        if !self.loaded_ast() {
            return Err(StdError::GenericErr {
                msg: format!("cannot call 'validate' without a compiled script or core"),
                backtrace: None,
            });
        }

        //self.rh_ast.unwrap().

        Ok(())
    }

    #[inline]
    pub fn run_deploy(&mut self, _env: Env) -> StdResult<HandleResponse> {
        // TODO:
        Ok(HandleResponse::default())
    }

    #[inline]
    pub fn run_handle(&mut self, env: Env) -> StdResult<HandleResponse> {
        if !self.loaded_ast() {
            return Err(StdError::GenericErr {
                msg: format!("cannot call 'run_handle' without a compiled script or core"),
                backtrace: None,
            });
        }

        let ast = self.rh_ast.as_mut().unwrap();
        let mut scope = Scope::new();

        scope.push("env", env);
        //scope.push("my_string", "hello, world!");
        //scope.push_constant("MY_CONST", true);

        let _res = self.rh_engine.call_fn(&mut scope, ast,
                                          "handle", ()).map_err(|err| {
            return StdError::GenericErr {
                msg: format!("failed to run 'handle' on rhai script: {err}"),
                backtrace: None,
            };
        })?;

        Ok(HandleResponse::default())
    }
}

// Compression

#[inline]
fn decompress_bytes(compressed_bytes: &[u8]) -> Result<Vec<u8>, StdError> {
    let mut decoder = GzDecoder::new(compressed_bytes);
    let mut buf: Vec<u8> = Vec::new();

    let res = decoder.read_to_end(&mut buf).map_err(|err| {
        return StdError::GenericErr {
            msg: format!("failed to deflate rhai script: {err}"),
            backtrace: None,
        };
    })?;

    debug_print!("deflated rhai script ({} bytes)", res);

    return Ok(buf);
}
