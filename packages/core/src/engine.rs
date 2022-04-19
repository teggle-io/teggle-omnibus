extern crate zip_module_resolver;

use std::cell::RefCell;
use std::io::Read;
use std::rc::Rc;

use cosmwasm_std::{Api, debug_print, Env, Extern, HandleResponse, Querier, StdError, StdResult, Storage};
use flate2::read::GzDecoder;
use rhai::{AST, Engine, Module, Scope, ScriptFnDef, Shared};
use rhai::packages::Package;
use zip_module_resolver::{RHAI_SCRIPT_EXTENSION, ZipModuleResolver};
use crate::CortexConfig;

use crate::rhai::packages::pkg_std::StandardPackage;

pub const YAML_EXTENSION: &'static str = "yaml";

pub const MAIN_FILE: &'static str = "main";
pub const CFG_FILE: &'static str = "config";

pub const ENDPOINT_FN_DEPLOY: &'static str = "deploy";
pub const ENDPOINT_FN_HANDLE: &'static str = "handle";
pub const ENDPOINT_FN_QUERY: &'static str = "query";

pub const ENDPOINT_METHODS: &'static [&'static str] = &[ENDPOINT_FN_DEPLOY, ENDPOINT_FN_HANDLE, ENDPOINT_FN_QUERY];

pub struct OmnibusEngine<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier> {
    rh_engine: Engine,
    rh_resolver: RefCell<Option<ZipModuleResolver>>,
    rh_ast: Option<AST>,
    deps: Rc<RefCell<Extern<S, A, Q>>>,
    cfg: Option<CortexConfig>,
    #[cfg(any(feature = "debug-print", feature = "test-print"))]
    debug_label: String,
}

impl<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier> OmnibusEngine<S, A, Q> {
    #[inline(always)]
    pub fn new(
        deps: Rc<RefCell<Extern<S, A, Q>>>,
    ) -> Self {
        let mut engine = Self::new_raw(deps);

        engine.register_modules();
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
            cfg: None,
            #[cfg(any(feature = "debug-print", feature = "test-print"))]
            debug_label: "None".to_string(),
        }
    }

    #[inline(always)]
    pub fn register_components(&mut self) {
        self.register_handlers()
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


    pub fn register_handlers(&mut self) -> &mut Self {
        #[cfg(any(feature = "debug-print", feature = "test-print"))]
        {
            let label = self.debug_label.clone();
            self.rh_engine.on_print(move |text| {
                #[cfg(feature = "debug-print")]
                debug_print!("CORTEX[{}][info ]: {}", label, text);

                #[cfg(feature = "test-print")]
                println!("CORTEX[{}][info ]: {}", label, text);
            });

            let label = self.debug_label.clone();
            self.rh_engine.on_debug(move |text, source, _pos| {
                if let Some(source) = source {
                    #[cfg(feature = "debug-print")]
                    debug_print!("CORTEX[{}][debug]: {} | {}", label, source, text);

                    #[cfg(feature = "test-print")]
                    println!("CORTEX[{}][debug]: {} | {}", label, source, text);
                } else {
                    #[cfg(feature = "debug-print")]
                    debug_print!("CORTEX[{}][debug]: {}", label, text);

                    #[cfg(feature = "test-print")]
                    println!("CORTEX[{}][debug]: {}", label, text);
                }
            });
        }

        self
    }

    pub fn register_functions(&mut self) -> &mut Self {
        // TODO:
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

        self.load_config()?;
        self.register_components();
        self.load_main()?;

        Ok(())
    }

    pub fn get_file(&mut self, path: &str, custom_extension: Option<String>) -> Result<String, StdError> {
        if !self.loaded_core() {
            return Err(StdError::GenericErr {
                msg: format!("can not 'get_file' without a core loaded (attempting to load '{}.{}')",
                             path, custom_extension
                                 .unwrap_or(RHAI_SCRIPT_EXTENSION.to_string())),
                backtrace: None,
            });
        }

        let mut rc_resolver = RefCell::borrow_mut(&self.rh_resolver);
        let resolver = rc_resolver.as_mut().unwrap();

        let full_path = resolver.get_file_path(path, None,
                                               custom_extension.to_owned());
        let source = resolver.get_file(full_path)
            .map_err(|err| {
                return StdError::GenericErr {
                    msg: format!("failed to load file '{}.{}': {err}",
                                 path, custom_extension
                                     .unwrap_or(RHAI_SCRIPT_EXTENSION.to_string())),
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

    pub fn load_script_raw(&mut self, script: &str) -> Result<(), StdError> {
        let ast: AST = self.rh_engine.compile(script).map_err(|err| {
            return StdError::GenericErr {
                msg: format!("failed to compile rhai script: {err}"),
                backtrace: None,
            };
        })?;

        if self.rh_ast.is_some() {
            // TODO: This should be changed to 'combine'
            self.rh_ast = Some(self.rh_ast.as_mut().unwrap().merge(&ast));
        } else {
            self.rh_ast = Some(ast);
        }

        Ok(())
    }

    #[inline(always)]
    pub fn load_config_source(&mut self) -> Result<String, StdError> {
        if !self.loaded_core() {
            return Err(StdError::GenericErr {
                msg: format!("can not 'load_config_source' without a core loaded."),
                backtrace: None,
            });
        }

        self.get_file(CFG_FILE, Some(String::from(YAML_EXTENSION)))
    }

    pub fn load_config(&mut self) -> Result<(), StdError> {
        let cfg_source = self.load_config_source()?;

        let mut cfg = CortexConfig::new();
        cfg.load_source(cfg_source.as_str())?;

        #[cfg(any(feature = "debug-print", feature = "test-print"))]
        {
            let name = cfg.cortex_name();
            let version = cfg.cortex_version();

            self.debug_label = format!("{}:{}", name, version);
        }

        self.cfg = Some(cfg);

        Ok(())
    }

    pub fn validate(&mut self) -> Result<(), StdError> {
        if !self.loaded_ast() {
            return Err(StdError::GenericErr {
                msg: format!("cannot call 'validate' without a compiled script or core"),
                backtrace: None,
            });
        }

        let ast = self.rh_ast.as_ref().unwrap();
        let lib = ast.shared_lib();

        for endpoint_fname in ENDPOINT_METHODS {
            validate_endpoint_method(lib, endpoint_fname)?;
        }

        Ok(())
    }

    pub fn run_deploy(&mut self, _env: Env) -> StdResult<HandleResponse> {
        // TODO:
        Ok(HandleResponse::default())
    }

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

//// Utils

// Validate

fn validate_endpoint_method(lib: &Shared<Module>,
                            name: &str) -> Result<(), StdError> {
    let res: Option<&Shared<ScriptFnDef>> = lib.get_script_fn(name, 0);
    return match res {
        None => {
            Err(StdError::GenericErr {
                msg: format!("core or script is invalid, missing 'fn {name}()' endpoint"),
                backtrace: None,
            })
        }
        Some(_m) => {
            Ok(())
        }
    };
}


// Compression

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
