use cosmwasm_std::{debug_print, new_storage, Storage};
use rhai::Engine;

pub struct OmnibusEngine<S: Storage> {
    pub rh_engine: Engine,
    pub storage: S,
}

impl <S: Storage> OmnibusEngine<S> {
    pub fn new() -> Self {
        let storage: S = new_storage();
        let mut engine = Self {
            rh_engine: Engine::new(),
            storage: storage
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
