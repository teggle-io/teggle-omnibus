use std::cell::RefCell;
use std::collections::BTreeMap;

use cosmwasm_std::StdError;
use rhai::{Dynamic, INT, Map};

pub const CFG_KEY_CORTEX_NAME: &'static str = "cortex.name";
pub const CFG_KEY_CORTEX_VERSION: &'static str = "cortex.version";

pub const REQ_STR_KEYS: &'static [&'static str] = &[CFG_KEY_CORTEX_NAME, CFG_KEY_CORTEX_VERSION];

pub struct CortexConfig {
    data: Map,
    cache: RefCell<BTreeMap<String, Dynamic>>,
}

impl CortexConfig {
    pub fn new(map: Map) -> Self {
        Self {
            data: map,
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

    pub fn get_int(&self, key: &str) -> Option<INT> {
        let val = self.get(key);
        if val.is::<INT>() != true {
            return None;
        }

        Some(val.as_int().unwrap())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        let val = self.get(key);
        if val.is::<bool>() != true {
            return None;
        }

        Some(val.as_bool().unwrap())
    }

    fn _get(&self, key: &str) -> Dynamic {
        if key.is_empty() {
            return Dynamic::UNIT;
        }

        let keys = key.split(".").collect::<Vec<_>>();

        // Get the first key (outside the loop as this comes from data).
        let cur_key = match keys.get(0) {
            None => {
                return Dynamic::UNIT;
            }
            Some(cur_key) => cur_key
        };

        let mut cur = match self.data.get(*cur_key) {
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

                // Get the next key value
                let cur_key = match keys.get(ki) {
                    None => {
                        return Dynamic::UNIT;
                    }
                    Some(cur_key) => cur_key
                };

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
