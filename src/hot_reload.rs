// src/hot_reload.rs
//! Hot reload infrastructure (requires 'hot-reload' feature)

#[cfg(feature = "hot-reload")]
use libloading::{Library, Symbol};

#[cfg(feature = "hot-reload")]
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

#[cfg(feature = "hot-reload")]
use std::path::PathBuf;
#[cfg(feature = "hot-reload")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "hot-reload")]
use std::sync::Arc;

#[cfg(feature = "hot-reload")]
pub type ResonatorFactoryFn = unsafe extern "C" fn(&crate::resonator::FieldMap) -> Box<dyn crate::resonator::Resonator>;

#[cfg(feature = "hot-reload")]
pub struct HotReloadSystem {
    lib_path: PathBuf,
    lib: Option<Library>,
    watcher: RecommendedWatcher,
    reload_pending: Arc<AtomicBool>,
}

#[cfg(feature = "hot-reload")]
impl HotReloadSystem {
    pub fn new(lib_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let reload_pending = Arc::new(AtomicBool::new(false));
        let reload_flag = reload_pending.clone();
        
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                if event.kind.is_modify() {
                    reload_flag.store(true, Ordering::Relaxed);
                }
            }
        })?;
        
        watcher.watch(&lib_path, RecursiveMode::NonRecursive)?;
        
        Ok(Self {
            lib_path,
            lib: None,
            watcher,
            reload_pending,
        })
    }
    
    pub fn load(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            self.lib = Some(Library::new(&self.lib_path)?);
        }
        Ok(())
    }
    
    pub fn should_reload(&self) -> bool {
        self.reload_pending.load(Ordering::Relaxed)
    }
    
    pub fn reload(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.lib = None;
        std::thread::sleep(std::time::Duration::from_millis(100));
        
        unsafe {
            self.lib = Some(Library::new(&self.lib_path)?);
        }
        
        self.reload_pending.store(false, Ordering::Relaxed);
        println!("✅ Hot reload completed");
        Ok(())
    }
    
    pub fn get_factory(&self, symbol_name: &[u8]) -> Result<Symbol<ResonatorFactoryFn>, Box<dyn std::error::Error>> {
        let lib = self.lib.as_ref().ok_or("Library not loaded")?;
        unsafe {
            let symbol = lib.get(symbol_name)?;
            Ok(symbol)
        }
    }
}

#[cfg(not(feature = "hot-reload"))]
pub struct HotReloadSystem;

#[cfg(not(feature = "hot-reload"))]
impl HotReloadSystem {
    pub fn new(_path: std::path::PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        Err("Hot reload feature not enabled".into())
    }
}