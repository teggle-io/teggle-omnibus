use std::cell::RefCell;
use std::rc::Rc;

use cosmwasm_std::{Api, Env, Extern, HandleResponse, Querier, StdResult, Storage};
use crate::OmnibusEngine;

pub fn deploy<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier>(
    deps: Rc<RefCell<Extern<S, A, Q>>>,
    env: Env,
    data: Vec<u8>,
) -> StdResult<HandleResponse> {
    let mut engine = OmnibusEngine::new(deps);
    engine.load_core(data)?;
    engine.validate()?;
    engine.run_deploy(env)
}

pub fn handle<S: 'static + Storage, A: 'static + Api, Q: 'static + Querier>(
    deps: Rc<RefCell<Extern<S, A, Q>>>,
    env: Env,
    data: Vec<u8>,
) -> StdResult<HandleResponse> {
    let mut engine = OmnibusEngine::new(deps);
    engine.load_core(data)?;
    engine.run_handle(env)
}