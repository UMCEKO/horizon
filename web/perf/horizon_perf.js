// Loader for horizon_perf.wasm produced by `cargo xtask perf-wasm`.
let wasmModule = null;

export async function loadWasm(url = "./horizon_perf.wasm") {
  const res = await fetch(url);
  const bytes = await res.arrayBuffer();
  const mod = await WebAssembly.instantiate(bytes, {});
  wasmModule = mod.instance;
  return wasmModule;
}

export function summary(ptr, len) {
  if (!wasmModule) throw new Error("wasm not loaded");
  return wasmModule.exports.horizon_perf_summary(ptr, len);
}
