use mlua::{Lua, Table, Result as LuaResult, Function};
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashSet;

// Define base directories for mobile storage
const RMODS_ROOT: &str = "horizon/rmods";
const DISABLED_CONFIG: &str = "horizon/rmods/disabled.json";

pub struct ModLoader {
    lua: Lua,
    enabled_mods: HashSet<String>,
}

impl ModLoader {
    pub fn new() -> Self {
        let lua = Lua::new();
        let mut loader = Self {
            lua,
            enabled_mods: HashSet::new(),
        };
        
        loader.register_unrestricted_bindings();
        loader.load_disabled_states();
        loader
    }

    /// Loads disabled mod IDs from disabled.json
    pub fn load_disabled_states(&mut self) {
        self.enabled_mods.clear();
        let path = Path::new(DISABLED_CONFIG);
        
        let disabled_set: HashSet<String> = if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                serde_json::from_str(&content).unwrap_or_default()
            } else {
                HashSet::new()
            }
        } else {
            // Create empty config if missing
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(path, "[]");
            HashSet::new()
        };

        // Scan rmods root and populate enabled set
        if let Ok(entries) = fs::read_dir(RMODS_ROOT) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            let mod_name = name.to_string();
                            if !disabled_set.contains(&mod_name) {
                                self.enabled_mods.insert(mod_name);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Exposes unrestricted native Rust power to Lua scripts (Filesystem, Network, Device I/O)
    fn register_unrestricted_bindings(&mut self) {
        let globals = self.lua.globals();

        // 1. Unrestricted File System Read/Write (Direct device storage access)
        let fs_table = self.lua.create_table().unwrap();
        
        fs_table.set("read_text", self.lua.create_function(|_, path: String| {
            Ok(fs::read_to_string(path).unwrap_or_default())
        }).unwrap()).unwrap();

        fs_table.set("write_text", self.lua.create_function(|_, (path, content): (String, String)| {
            if let Some(parent) = Path::new(&path).parent() {
                let _ = fs::create_dir_all(parent);
            }
            Ok(fs::write(path, content).is_ok())
        }).unwrap()).unwrap();

        globals.set("device_fs", fs_table).unwrap();

        // 2. Unrestricted Network & Downloading capabilities (Like JER / Full .hnmod)
        let net_table = self.lua.create_table().unwrap();
        
        net_table.set("download_file", self.lua.create_function(|_, (url, save_path): (String, String)| {
            println!("[RML-Net] Downloading from: {} -> {}", url, save_path);
            let response = reqwest::blocking::get(&url);
            match response {
                Ok(resp) => {
                    if let Ok(bytes) = resp.bytes() {
                        if let Some(parent) = Path::new(&save_path).parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        let _ = fs::write(&save_path, bytes);
                        return Ok(true);
                    }
                    Ok(false)
                }
                Err(_) => Ok(false),
            }
        }).unwrap()).unwrap();

        globals.set("device_net", net_table).unwrap();
        
        // 3. Logger bridge
        globals.set("print_log", self.lua.create_function(|_, msg: String| {
            println!("[RML-Script] {}", msg);
            Ok(())
        }).unwrap()).unwrap();
    }

    /// Scans all enabled RMods, registers their Resource folders, and executes their Scripts/
    pub fn scan_and_run_mods(&self) {
        let root = Path::new(RMODS_ROOT);
        if !root.exists() {
            let _ = fs::create_dir_all(root);
            return;
        }

        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(mod_name_str) = path.file_name().and_then(|n| n.to_str()) {
                        let mod_name = mod_name_str.to_string();

                        // Skip if disabled by user toggle
                        if !self.enabled_mods.contains(&mod_name) {
                            continue;
                        }

                        println!("[RML] Initializing RMod: {}", mod_name);

                        // Mount Resource Folder (Textures & Models)
                        let resource_path = path.join("Resource");
                        if resource_path.exists() {
                            self.register_resources(&mod_name, &resource_path);
                        }

                        // Execute Script Folder (.lua files)
                        let scripts_path = path.join("Scripts");
                        if scripts_path.exists() {
                            self.execute_scripts(&scripts_path);
                        }
                    }
                }
            }
        }
    }

    fn register_resources(&self, mod_name: &str, resource_dir: &Path) {
        // Bridges raw assets into Horizon's resource manager backend
        println!("[RML-Assets] Registering assets for [{}] from path: {:?}", mod_name, resource_dir);
        // Horizon asset registration hook integration point
    }

    fn execute_scripts(&self, scripts_dir: &Path) {
        if let Ok(entries) = fs::read_dir(scripts_dir) {
            for entry in entries.flatten() {
                let script_path = entry.path();
                if script_path.is_file() {
                    if let Some(ext) = script_path.extension().and_then(|e| e.to_str()) {
                        if ext == "lua" {
                            if let Ok(code) = fs::read_to_string(&script_path) {
                                if let Err(e) = self.lua.load(&code).exec() {
                                    eprintln!("[RML-Error] Failed to execute script {:?}: {}", script_path, e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Hot-reload trigger for the Rmod Menu
    pub fn reload_mods(&mut self) {
        println!("[RML] Reloading all RMods...");
        self.load_disabled_states();
        self.scan_and_run_mods();
    }
}

// Global hook called by Horizon engine on startup
#[no_mangle]
pub extern "C" fn horizon_mod_init() {
    let loader = ModLoader::new();
    loader.scan_and_run_mods();
              }
                                                           
