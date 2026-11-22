use anyhow::{bail, ensure, Context, Result};
use std::path::Path;
use wasmi::{Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

// SDK 0.9.1, API v2. No JS glue or WASI imports are needed.
pub fn build(wasm: &Path, width: u32, height: u32, rgba: &[u8], copies: u8, dither: bool) -> Result<Vec<u8>> {
    ensure!(width > 0 && height > 0, "image dimensions must be positive");
    ensure!(copies > 0, "copies must be 1..255");
    let expected = u64::from(width) * u64::from(height) * 4;
    ensure!(expected == rgba.len() as u64, "RGBA length mismatch");
    ensure!(expected <= 128 * 1024 * 1024, "image exceeds 128 MiB RGBA limit");
    let mut config = wasmi::Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    let module = Module::new(&engine, &std::fs::read(wasm)
        .with_context(|| format!("read WASM {}; run scripts/fetch_sdk.py", wasm.display()))?[..])?;
    ensure!(module.imports().next().is_none(), "unexpected WASM imports");
    let limits = StoreLimitsBuilder::new().memory_size(256 * 1024 * 1024).build();
    let mut store = Store::new(&engine, limits);
    store.limiter(|s: &mut StoreLimits| s);
    store.set_fuel(10_000_000_000)?;
    let instance = Linker::<StoreLimits>::new(&engine).instantiate(&mut store, &module)?.start(&mut store)?;
    let memory = instance.get_memory(&store, "memory").context("missing memory")?;
    let version = instance.get_typed_func::<(), i32>(&store, "get_wasm_api_version")?;
    ensure!(version.call(&mut store, ())? == 2, "expected WASM API version 2");
    let alloc = instance.get_typed_func::<i32, i32>(&store, "alloc")?;
    let free = instance.get_typed_func::<(i32, i32), ()>(&store, "dealloc")?;
    let build = instance.get_typed_func::<(i32, i32, i32, i32, i32, i32), i32>(&store, "build_print_command")?;
    let result_len = instance.get_typed_func::<(), i32>(&store, "get_last_result_len")?;
    let error_code = instance.get_typed_func::<(), i32>(&store, "get_last_error_code")?;
    let len = i32::try_from(rgba.len())?;
    let input = alloc.call(&mut store, len)?;
    ensure!(input != 0, "WASM allocation failed");
    memory.write(&mut store, input as u32 as usize, rgba)?;
    let output = build.call(&mut store, (width as i32, height as i32, input, len, i32::from(copies), i32::from(dither)))?;
    let n = result_len.call(&mut store, ())?;
    let error = error_code.call(&mut store, ())?;
    if error != 0 {
        // Store drop also releases all guest memory on error/trap.
        bail!("WASM build error {error} (1=dimensions, 2=RGBA, 3=copies, 4=dither)");
    }
    ensure!(output != 0 && n > 0 && n <= 64 * 1024 * 1024, "invalid WASM output length {n}");
    let mut bytes = vec![0; n as usize];
    memory.read(&store, output as u32 as usize, &mut bytes)?;
    free.call(&mut store, (input, len))?;
    free.call(&mut store, (output, n))?;
    Ok(bytes)
}
