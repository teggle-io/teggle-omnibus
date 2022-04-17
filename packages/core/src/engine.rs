use cosmwasm_std::{Extern, Api, Querier, Storage, make_dependencies, debug_print};
use rhai::Engine;

pub struct OmnibusEngine<S: Storage, A: Api, Q: Querier> {
    pub rh_engine: Engine,
    pub deps: Extern<S, A, Q>,
}

impl <S: Storage, A: Api, Q: Querier> OmnibusEngine<S, A, Q> {
    pub fn new() -> Self {
        let deps = make_dependencies();
        let mut engine = Self {
            rh_engine: Engine::new(),
            deps: make_dependencies()
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
        self.rh_engine.register_fn("do_store", move |key: &str, val: &str| {
            //controller.borrow_mut().deps.storage.set(key.as_bytes(), val.as_bytes());
        });
    }
}
