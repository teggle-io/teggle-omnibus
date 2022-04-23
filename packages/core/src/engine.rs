extern crate zip_module_resolver;

use std::cell::RefCell;
use std::rc::Rc;

use cosmwasm_std::{Api, Env, Extern, HandleResponse, Querier, StdError, StdResult, Storage};
#[cfg(feature = "debug-print")]
use cosmwasm_std::{debug_print};
use rhai::{AST, Caches, Dynamic, Engine, EvalAltResult, GlobalRuntimeState, ImmutableString, Module, Scope, ScriptFnDef, Shared};
use rhai::packages::Package;
use zip_module_resolver::{ZipModuleResolver};

use crate::CortexConfig;
use crate::rhai::packages::pkg_std::StandardPackage;

pub const ENDPOINT_FN_DEPLOY: &'static str = "deploy";
pub const ENDPOINT_FN_HANDLE: &'static str = "handle";
pub const ENDPOINT_FN_QUERY: &'static str = "query";

pub const ENDPOINT_METHODS: &'static [&'static str] = &[ENDPOINT_FN_DEPLOY, ENDPOINT_FN_HANDLE, ENDPOINT_FN_QUERY];

pub struct OmnibusEngine<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier> {
    rh_engine: Engine,
    rh_caches: Option<Caches>,
    rh_global: Option<GlobalRuntimeState<'static>>,
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

        engine.default_init();
        engine
    }

    #[inline(always)]
    pub fn new_raw(
        deps: Rc<RefCell<Extern<S, A, Q>>>,
    ) -> Self {
        Self {
            rh_engine: Engine::new_raw(),
            rh_caches: None,
            rh_global: None,
            rh_resolver: RefCell::new(None),
            rh_ast: None,
            deps,
            cfg: None,
            #[cfg(any(feature = "debug-print", feature = "test-print"))]
            debug_label: "None".to_string(),
        }
    }

    #[inline(always)]
    pub fn default_init(&mut self) -> &mut Self {
        self.register_modules();
        self.rh_engine.set_strict_variables(true);

        self
    }

    #[inline(always)]
    pub fn register_components(&mut self) -> &mut Self {
        self.register_handlers()
            .register_functions()
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
        // TODO: This is a mess and will change a lot (this is just for testing).
        let deps = self.deps.clone();
        self.rh_engine.register_fn("storage_set", move |key: &str, val: &str| {
            RefCell::borrow_mut(&*deps).storage.set(key.as_bytes(), val.as_bytes());
        });

        let deps = self.deps.clone();
        self.rh_engine.register_fn("storage_set", move |key: &str, val: &[u8]| {
            RefCell::borrow_mut(&*deps).storage.set(key.as_bytes(), val);
        });

        let deps = self.deps.clone();
        self.rh_engine.register_result_fn("storage_set", move |key_path: &mut Vec<Dynamic>, val: &str| -> Result<(), Box<EvalAltResult>> {
            let key = expand_key_path(key_path).map_err(|err| {
                return format!("error during storage set: {err}");
            })?;

            RefCell::borrow_mut(&*deps).storage.set(key.as_bytes(), val.as_bytes());

            Ok(())
        });

        let deps = self.deps.clone();
        self.rh_engine.register_result_fn("storage_set", move |key_path: &mut Vec<Dynamic>, val: &[u8]| -> Result<(), Box<EvalAltResult>> {
            let key = expand_key_path(key_path).map_err(|err| {
                return format!("error during storage set: {err}");
            })?;

            RefCell::borrow_mut(&*deps).storage.set(key.as_bytes(), val);

            Ok(())
        });

        self
    }

    #[inline(always)]
    pub fn loaded_core(&mut self) -> bool {
        self.rh_resolver.borrow().is_some()
            && self.rh_ast.is_some()
            && self.rh_caches.is_some()
            && self.rh_global.is_some()
    }

    pub fn load_core(&mut self, bytes: Vec<u8>, env: Env) -> Result<(), StdError> {
        let mut resolver = ZipModuleResolver::new();
        resolver.load_from_bytes(bytes)
            .map_err(|err| {
                return StdError::GenericErr {
                    msg: format!("failed to load core: {err}"),
                    backtrace: None,
                };
            })?;

        self.rh_resolver = RefCell::new(Some(resolver.clone()));
        self.rh_engine.set_module_resolver(resolver);

        self.init_core(env)?;

        Ok(())
    }

    pub fn init_core(&mut self, env: Env) -> Result<(), StdError> {
        {
            let mut rc_resolver = RefCell::borrow_mut(&self.rh_resolver);
            let resolver = rc_resolver.as_mut().unwrap();

            // TODO: Abstract (this is a mess, and will change a lot).
            let mut scope = Scope::new();
            scope.push_constant("ENV", env);

            let ast_res = resolver.init_with_scope(&self.rh_engine, scope)
                .map_err(|err| {
                    return StdError::GenericErr {
                        msg: format!("failed to init core: {err}"),
                        backtrace: None,
                    };
                })?;
            match ast_res {
                None => {
                    return Err(StdError::GenericErr {
                        msg: format!("failed to compile core, no AST returned."),
                        backtrace: None,
                    });
                }
                Some(ast) => {
                    if self.rh_ast.is_some() {
                        self.rh_ast = Some(self.rh_ast.as_mut().unwrap().merge(&ast));
                    } else {
                        self.rh_ast = Some(ast);
                    }
                }
            }
        }

        self.load_config()?;
        self.register_components(); // Must go after load_config to apply the label.
        self.warm_ast()?;

        Ok(())
    }

    pub fn warm_ast(&mut self) -> Result<(), StdError> {
        let mut rc_resolver = RefCell::borrow_mut(&self.rh_resolver);
        let resolver = rc_resolver.as_mut().unwrap();

        let ast = self.rh_ast.as_mut().unwrap();
        let mut scope = resolver.scope().clone();

        let mut caches = Caches::new();
        let mut global = GlobalRuntimeState::new(&self.rh_engine);

        let rewind_scope = true;
        let statements = ast.statements();
        let orig_scope_len = scope.len();

        if !statements.is_empty() {
            self.rh_engine.eval_statements_raw(&mut scope, &mut global, &mut caches, statements, &[ast.as_ref()], 0)
                .map_err(|err| {
                    return StdError::GenericErr {
                        msg: format!("failed to 'eval_statements_raw' during init of core: {err}"),
                        backtrace: None,
                    };
                })?;

            if rewind_scope {
                scope.rewind(orig_scope_len);
            }
        }

        self.rh_caches = Some(caches);
        self.rh_global = Some(global);

        Ok(())
    }

    pub fn load_config(&mut self) -> Result<(), StdError> {
        let mut rc_resolver = RefCell::borrow_mut(&self.rh_resolver);
        let resolver = rc_resolver.as_mut().unwrap();

        let mut cfg = CortexConfig::new(resolver.config());
        cfg.validate()?;

        #[cfg(any(feature = "debug-print", feature = "test-print"))]
        {
            self.debug_label = format!("{}:{}", cfg.cortex_name(), cfg.cortex_version());
        }

        self.cfg = Some(cfg);

        Ok(())
    }

    pub fn validate(&mut self) -> Result<(), StdError> {
        if !self.loaded_core() {
            return Err(StdError::GenericErr {
                msg: format!("cannot call 'validate' without a compiled core"),
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

    pub fn run_deploy(&mut self) -> StdResult<HandleResponse> {
        // TODO:
        Ok(HandleResponse::default())
    }

    pub fn run_handle(&mut self) -> StdResult<HandleResponse> {
        if !self.loaded_core() {
            return Err(StdError::GenericErr {
                msg: format!("cannot call 'run_handle' without a compiled core"),
                backtrace: None,
            });
        }

        let rc_resolver = RefCell::borrow_mut(&self.rh_resolver);
        let resolver = rc_resolver.as_ref().unwrap();

        let caches = self.rh_caches.as_mut().unwrap();
        let global = self.rh_global.as_mut().unwrap();
        let ast = self.rh_ast.as_mut().unwrap();
        let mut scope = resolver.scope().clone();

        // no affect, needs to be set before globals.
        //scope.push_constant("ENV", "dffs");

        let mut args: [Dynamic; 0] = [];

        for _ in 0..1000_i32 {
            self.rh_engine.call_fn_raw_raw(&mut scope, global, caches, &ast, false,
                                           true, "simple", None, &mut args, )
                .map_err(|err| {
                    return StdError::GenericErr {
                        msg: format!("failed to run 'handle' on rhai script: {err}"),
                        backtrace: None,
                    };
                })?;
        }

        Ok(HandleResponse::default())
    }
}

//// Utils

// Keys
// TODO: Move
fn expand_key_path(key_path: &mut Vec<Dynamic>) -> Result<String, String> {
    if key_path.is_empty() {
        return Err("key path is required.")?;
    }

    let mut buf = String::new();
    key_path.into_iter().try_for_each(|key| {
        if !buf.is_empty() { buf.push_str("."); }

        match key.read_lock::<ImmutableString>() {
            None => {
                return Err("keys must all be String.");
            }
            Some(v) => buf.push_str(v.as_str())
        };

        Ok(())
    })?;

    Ok(buf)
}

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
