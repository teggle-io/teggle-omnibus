extern crate zip_module_resolver;

use std::cell::RefCell;
use std::io::{Read};
use std::rc::Rc;

use cosmwasm_std::{Api, debug_print, Env, Extern, HandleResponse, Querier, StdError, StdResult, Storage};
use flate2::read::GzDecoder;
use rhai::{AST, Engine, Scope};
use zip_module_resolver::ZipModuleResolver;

pub struct OmnibusEngine<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier> {
    rh_engine: Engine,
    deps: Rc<RefCell<Extern<S, A, Q>>>,
    ast: Option<AST>,
    label: String,
}

impl<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier> OmnibusEngine<S, A, Q> {
    pub fn new(
        deps: Rc<RefCell<Extern<S, A, Q>>>,
    ) -> Self {
        let mut engine = Self {
            rh_engine: Engine::new(),
            deps,
            ast: None,
            label: "cortex.core:v1".to_string(), // TODO:
        };

        engine.init();
        engine
    }

    pub fn init(&mut self) {
        self.register_handlers();
        self.register_functions();
    }

    pub fn register_handlers(&mut self) {
        let label = self.label.clone();
        self.rh_engine.on_print(move |text| {
            debug_print!("RHAI[info ][{}]: {}", label, text);
        });

        let label = self.label.clone();
        self.rh_engine.on_debug(move |text, source, pos| {
            if let Some(source) = source {
                debug_print!("RHAI[debug][{}]: {} @ {:?} | {}", label, source, pos, text);
            } else if pos.is_none() {
                debug_print!("RHAI[debug][{}]: {}", label, text);
            } else {
                debug_print!("RHAI[debug][{}]: {:?} | {}", label, pos, text);
            }
        });
    }

    pub fn register_functions(&mut self) {
        let deps = self.deps.clone();
        self.rh_engine.register_fn("storage_set", move |key: &str, val: &str| {
            RefCell::borrow_mut(&*deps).storage.set(key.as_bytes(), val.as_bytes());
        });

        let deps = self.deps.clone();
        self.rh_engine.register_fn("storage_set", move |key: &str, val: &[u8]| {
            RefCell::borrow_mut(&*deps).storage.set(key.as_bytes(), val);
        });
    }

    pub fn load_script_compressed(&mut self, compressed_bytes: &[u8]) -> Result<(), StdError> {
        let b = decompress_bytes(compressed_bytes)?;
        let s = String::from_utf8(b).map_err(|err| {
            return StdError::GenericErr {
                msg: format!("failed convert rhai script binary to utf8 string: {err}"),
                backtrace: None,
            };
        })?;

        self.load_script(String::as_str(&s))
    }

    pub fn load_script(&mut self, script: &str) -> Result<(), StdError> {
        // TODO: https://rhai.rs/book/rust/modules/self-contained.html
        // Switch to the above, possibly zip or gzip a directory of files resolve from that.
        let ast: AST = self.rh_engine.compile(script).map_err(|err| {
            return StdError::GenericErr {
                msg: format!("failed to compile rhai script: {err}"),
                backtrace: None,
            };
        })?;

        self.ast = Some(ast);

        Ok(())
    }

    pub fn load_core(&mut self, bytes: Vec<u8>) -> Result<(), StdError> {
        let mut resolver = ZipModuleResolver::new();
        resolver.load_from_bytes(bytes).map_err(|err| {
            return StdError::GenericErr {
                msg: format!("failed to load core: {err}"),
                backtrace: None,
            };
        })?;

        // TODO: Enhance, use a collection.
        self.rh_engine.set_module_resolver(resolver);

        Ok(())
    }

    pub fn run_handle(&mut self, env: Env) -> StdResult<HandleResponse> {
        if self.ast.is_none() {
            return Err(StdError::GenericErr {
                msg: format!("cannot call 'run_handle' without a compiled script"),
                backtrace: None,
            })
        }

        let ast = self.ast.clone().unwrap();
        let mut scope = Scope::new();

        scope.push("env", env);
        //scope.push("my_string", "hello, world!");
        //scope.push_constant("MY_CONST", true);

        let _res = self.rh_engine.call_fn(&mut scope, &ast,
                                                 "handle", ( ) ).map_err(|err| {
            return StdError::GenericErr {
                msg: format!("failed to run 'handle' on rhai script: {err}"),
                backtrace: None,
            };
        })?;

        Ok(HandleResponse::default())
    }
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
