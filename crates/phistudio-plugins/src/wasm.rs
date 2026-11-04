use phistudio_sdk::{API_VERSION, MAX_MESSAGE_BYTES, Plugin, Request, Response};
use wasmi::{
    Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc,
};

#[derive(Clone, Copy, Debug)]
pub struct WasmLimits {
    pub memory_bytes: usize,
    pub fuel_per_call: u64,
    pub fuel_per_action: u64,
    pub module_bytes: usize,
}
impl Default for WasmLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 128 * 1024 * 1024,
            fuel_per_call: 8_000_000,
            fuel_per_action: 4_000_000_000,
            module_bytes: 16 * 1024 * 1024,
        }
    }
}

/// No WASI or host imports. All access to the editor uses the checked protocol.
pub struct WasmPlugin {
    store: Store<StoreLimits>,
    memory: Memory,
    alloc: TypedFunc<u32, u32>,
    free: TypedFunc<(u32, u32), ()>,
    dispatch: TypedFunc<(u32, u32), u64>,
    limits: WasmLimits,
    poisoned: bool,
    call_budget: u64,
}

impl WasmPlugin {
    /// Translation/instantiation belongs on a loading worker, not in a UI frame.
    pub fn load(bytes: &[u8], limits: WasmLimits) -> Result<Self, String> {
        if bytes.len() > limits.module_bytes
            || limits.fuel_per_call == 0
            || limits.fuel_per_action == 0
            || limits.memory_bytes == 0
        {
            return Err("Wasm module or execution limits are invalid".into());
        }
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, bytes).map_err(|e| e.to_string())?;
        if module.imports().next().is_some() {
            return Err("Portable plugins must not import OS or host functions".into());
        }
        let limiter = StoreLimitsBuilder::new()
            .memory_size(limits.memory_bytes)
            .memories(1)
            .instances(1)
            .tables(4)
            .table_elements(16_384)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(&engine, limiter);
        store.limiter(|limits| limits);
        store
            .set_fuel(limits.fuel_per_call)
            .map_err(|e| e.to_string())?;
        let linker = Linker::new(&engine);
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|e| e.to_string())?;
        let version = instance
            .get_typed_func::<(), u32>(&store, "phistudio_api_version")
            .map_err(|e| e.to_string())?
            .call(&mut store, ())
            .map_err(|e| e.to_string())?;
        if version != API_VERSION {
            return Err(format!("Wasm plugin API {version} is unsupported"));
        }
        Ok(Self {
            memory: instance
                .get_memory(&store, "memory")
                .ok_or("Wasm plugin must export memory")?,
            alloc: instance
                .get_typed_func(&store, "phistudio_alloc")
                .map_err(|e| e.to_string())?,
            free: instance
                .get_typed_func(&store, "phistudio_free")
                .map_err(|e| e.to_string())?,
            dispatch: instance
                .get_typed_func(&store, "phistudio_call")
                .map_err(|e| e.to_string())?,
            store,
            limits,
            poisoned: false,
            call_budget: limits.fuel_per_call,
        })
    }

    fn exchange(&mut self, request: &Request) -> Result<Response, String> {
        let input = serde_json::to_vec(request).map_err(|e| e.to_string())?;
        if input.len() > MAX_MESSAGE_BYTES {
            return Err("Wasm request exceeds message limit".into());
        }
        self.call_budget = if matches!(request,Request::Input {input,..} if input.phase()==Some(phistudio_sdk::InputPhase::End))
            || matches!(
                request,
                Request::Command { .. }
                    | Request::Ui { .. }
                    | Request::Service { .. }
                    | Request::ServiceResult { .. }
                    | Request::Format { .. }
                    | Request::Resource { .. }
            ) {
            self.limits.fuel_per_action
        } else {
            self.limits.fuel_per_call
        };
        self.store
            .set_fuel(self.call_budget)
            .map_err(|e| e.to_string())?;
        let input_len = input.len() as u32;
        let input_ptr = self
            .alloc
            .call(&mut self.store, input_len)
            .map_err(|e| e.to_string())?;
        self.memory
            .write(&mut self.store, input_ptr as usize, &input)
            .map_err(|e| e.to_string())?;
        let output = self
            .dispatch
            .call(&mut self.store, (input_ptr, input_len))
            .map_err(|e| e.to_string())?;
        let output_ptr = output as u32;
        let output_len = (output >> 32) as u32;
        if output_len as usize > MAX_MESSAGE_BYTES || output_len == 0 {
            return Err("Wasm response exceeds message limit or is empty".into());
        }
        let input_range = u64::from(input_ptr)..u64::from(input_ptr) + u64::from(input_len);
        let output_range = u64::from(output_ptr)..u64::from(output_ptr) + u64::from(output_len);
        if input_range.start < output_range.end && output_range.start < input_range.end {
            return Err("Wasm response must own a separate allocation".into());
        }
        // Check before allocating on the host, including overflowing guest ranges.
        if output_range.end > self.memory.data_size(&self.store) as u64 {
            return Err("Wasm response points outside linear memory".into());
        }
        let mut bytes = vec![0; output_len as usize];
        self.memory
            .read(&self.store, output_ptr as usize, &mut bytes)
            .map_err(|e| e.to_string())?;
        self.free
            .call(&mut self.store, (output_ptr, output_len))
            .map_err(|e| e.to_string())?;
        self.free
            .call(&mut self.store, (input_ptr, input_len))
            .map_err(|e| e.to_string())?;
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Wasm response: {e}"))
    }

    /// Instructions charged to the most recent callback (or module startup).
    pub fn fuel_consumed(&self) -> u64 {
        self.store
            .get_fuel()
            .map_or(0, |remaining| self.call_budget.saturating_sub(remaining))
    }
}
impl Plugin for WasmPlugin {
    fn execution_cost(&self) -> phistudio_sdk::ExecutionCost {
        phistudio_sdk::ExecutionCost {
            fuel: Some(self.fuel_consumed()),
            memory_bytes: Some(self.memory.data_size(&self.store) as u64),
        }
    }
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        if self.poisoned {
            return Err("Wasm instance failed; reload the plugin before using it again".into());
        }
        let result = self.exchange(request);
        self.poisoned = result.is_err();
        result
    }
}
