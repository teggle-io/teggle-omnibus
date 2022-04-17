use std::cell::RefCell;
use std::rc::Rc;
use cosmwasm_std::{Api, debug_print, Extern, Querier, Storage};
use rhai::Engine;

pub struct OmnibusEngine<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier> {
    rh_engine: Engine,
    deps: Rc<RefCell<Extern<S, A, Q>>>,
}

impl<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier> OmnibusEngine<S, A, Q> {
    pub fn new(
        deps: Rc<RefCell<Extern<S, A, Q>>>,
    ) -> Self {
        let mut engine = Self {
            rh_engine: Engine::new(),
            deps
        };

        engine.init();
        engine
    }

    pub fn init(&mut self) {
        self.register_handlers();
        self.register_functions();
    }

    pub fn register_handlers(&mut self) {
        // TODO: Clean up.
        self.rh_engine.on_print(|text| {
            println!("CORTEX[]: {}", text);
            debug_print!("CORTEX[]: {}", text);
        });

        self.rh_engine.on_debug(|text, source, pos| {
            if let Some(source) = source {
                println!("{} @ {:?} | {}", source, pos, text);
                debug_print!("{} @ {:?} | {}", source, pos, text);
            } else if pos.is_none() {
                println!("{}", text);
                debug_print!("{}", text);
            } else {
                println!("{:?} | {}", pos, text);
                debug_print!("{:?} | {}", pos, text);
            }
        });
    }

    pub fn register_functions(&mut self) {
        let deps = self.deps.clone();

        self.rh_engine.register_fn("storage_set", move |key: &str, val: &str| {
            RefCell::borrow_mut(&*deps).storage.set(key.as_bytes(), val.as_bytes());
        });
    }
}
