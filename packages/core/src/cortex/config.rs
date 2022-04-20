use std::cell::RefCell;
use std::collections::BTreeMap;

use cosmwasm_std::StdError;
use rhai::{Dynamic, Map};

pub const CFG_KEY_CORTEX_NAME: &'static str = "cortex.name";
pub const CFG_KEY_CORTEX_VERSION: &'static str = "cortex.version";

pub const REQ_STR_KEYS: &'static [&'static str] = &[CFG_KEY_CORTEX_NAME, CFG_KEY_CORTEX_VERSION];

pub struct CortexConfig {
    map: Map,
    cache: RefCell<BTreeMap<String, Dynamic>>,
}

impl CortexConfig {
    pub fn new(map: Map) -> Self {
        Self {
            map,
            cache: BTreeMap::new().into(),
        }
    }

    pub(crate) fn validate(&mut self) -> Result<(), StdError> {
        for vk in REQ_STR_KEYS {
            let val = self.get_str(*vk);
            if val.is_none() {
                return Err(StdError::GenericErr {
                    msg: format!("cortex config missing key '{}'", *vk),
                    backtrace: None,
                });
            }
        }

        Ok(())
    }

    pub fn get(&self, key: &str) -> Dynamic {
        let cache_key = key.to_string();

        if let Some(value) = self.cache.borrow().get(&cache_key) {
            return value.clone();
        }

        let val = self._get(key);

        self.cache.borrow_mut().insert(cache_key, val.clone());

        val
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        let val = self.get(key);
        if val.is::<String>() != true {
            return None;
        }

        Some(val.into_string().unwrap())
    }

    fn _get(&self, key: &str) -> Dynamic {
        if key.is_empty() {
            return Dynamic::UNIT;
        }

        let keys = key.split(".").collect::<Vec<_>>();

        let cur_key_opt = keys.get(0);
        if cur_key_opt.is_none() {
            return Dynamic::UNIT;
        }

        let cur_key = cur_key_opt.unwrap();

        let mut cur = match self.map.get(*cur_key) {
            None => {
                return Dynamic::UNIT;
            }
            Some(cur) => cur.clone()
        };

        if keys.len() >= 2 {
            for ki in 1..keys.len() {
                // Is cur a Map?
                if cur.is::<Map>() != true {
                    return Dynamic::UNIT;
                }

                let cur_map = cur.read_lock::<Map>().unwrap();

                // Get next key
                let cur_key_opt = keys.get(ki);
                if cur_key_opt.is_none() {
                    return Dynamic::UNIT;
                }

                // Get value
                let cur_key = cur_key_opt.unwrap();

                cur = match cur_map.get(*cur_key) {
                    None => {
                        return Dynamic::UNIT;
                    }
                    Some(cur) => cur.clone()
                };
            }
        }

        return cur;
    }

    pub fn cortex_name(&self) -> String {
        return self.get_str(CFG_KEY_CORTEX_NAME).unwrap();
    }

    pub fn cortex_version(&self) -> String {
        return self.get_str(CFG_KEY_CORTEX_VERSION).unwrap();
    }
}
