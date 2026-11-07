use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    name: String,
    labels: Vec<(String, String)>,
}

#[derive(Debug)]
#[allow(dead_code)]
struct Counter {
    key: Key,
    value_bits: AtomicU64,
}

#[derive(Clone)]
pub struct CounterHandle {
    inner: std::sync::Arc<Counter>,
}

impl CounterHandle {
    pub fn add(&self, v: f64) {
        let mut cur = self.inner.value_bits.load(Ordering::Relaxed);
        loop {
            let new = (f64::from_bits(cur) + v).to_bits();
            match self.inner.value_bits.compare_exchange(
                cur,
                new,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
    }
    pub fn set(&self, v: f64) {
        self.inner.value_bits.store(v.to_bits(), Ordering::Relaxed);
    }
    pub fn get(&self) -> f64 {
        f64::from_bits(self.inner.value_bits.load(Ordering::Relaxed))
    }
}

pub struct CounterRegistry {
    inner: Mutex<BTreeMap<Key, std::sync::Arc<Counter>>>,
}

impl CounterRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn counter(&self, name: &str, labels: &[(&str, &str)]) -> CounterHandle {
        let mut g = self.inner.lock().unwrap();
        let mut labels: Vec<(String, String)> = labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        labels.sort();
        let key = Key {
            name: name.to_string(),
            labels,
        };
        let c = g
            .entry(key.clone())
            .or_insert_with(|| {
                std::sync::Arc::new(Counter {
                    key,
                    value_bits: AtomicU64::new(0),
                })
            })
            .clone();
        CounterHandle { inner: c }
    }
}

impl Default for CounterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn counter(name: &str, labels: &[(&str, &str)]) -> CounterHandle {
    static REG: std::sync::OnceLock<CounterRegistry> = std::sync::OnceLock::new();
    let r = REG.get_or_init(CounterRegistry::new);
    r.counter(name, labels)
}
