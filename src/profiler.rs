// src/profiler.rs
//! Built-in profiling for performance analysis

use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Default, Clone, Debug)]
pub struct ScopeStats {
    pub count: u64,
    pub total: Duration,
    pub min: Duration,
    pub max: Duration,
}

impl ScopeStats {
    pub fn avg(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            self.total / self.count as u32
        }
    }
}

pub struct Profiler {
    scopes: HashMap<String, ScopeStats>,
    stack: Vec<(String, Instant)>,
    enabled: bool,
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            scopes: HashMap::new(),
            stack: Vec::new(),
            enabled: true,
        }
    }
    
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    
    pub fn begin(&mut self, name: &str) {
        if !self.enabled {
            return;
        }
        self.stack.push((name.to_string(), Instant::now()));
    }
    
    pub fn end(&mut self) {
        if !self.enabled {
            return;
        }
        
        if let Some((name, start)) = self.stack.pop() {
            let elapsed = start.elapsed();
            
            let stats = self.scopes.entry(name).or_default();
            stats.count += 1;
            stats.total += elapsed;
            
            if stats.count == 1 {
                stats.min = elapsed;
                stats.max = elapsed;
            } else {
                if elapsed < stats.min {
                    stats.min = elapsed;
                }
                if elapsed > stats.max {
                    stats.max = elapsed;
                }
            }
        }
    }
    
    pub fn get(&self, name: &str) -> Option<&ScopeStats> {
        self.scopes.get(name)
    }
    
    pub fn reset(&mut self) {
        self.scopes.clear();
        self.stack.clear();
    }
    
    pub fn report(&self) -> String {
        let mut lines = vec![
            "╔═══════════════════════════════════════════════════════════════╗".to_string(),
            "║                    PROFILER REPORT                            ║".to_string(),
            "╚═══════════════════════════════════════════════════════════════╝".to_string(),
        ];
        
        if self.scopes.is_empty() {
            lines.push("  No data collected.".to_string());
            return lines.join("\n");
        }
        
        let mut scopes: Vec<_> = self.scopes.iter().collect();
        scopes.sort_by_key(|(_, stats)| std::cmp::Reverse(stats.total));
        
        lines.push(format!(
            "  {:<25} {:>10} {:>12} {:>12} {:>12}",
            "Scope", "Calls", "Avg", "Min", "Max"
        ));
        lines.push("  ".to_string() + &"─".repeat(73));
        
        for (name, stats) in scopes {
            lines.push(format!(
                "  {:<25} {:>10} {:>12} {:>12} {:>12}",
                truncate(name, 25),
                stats.count,
                format_duration(stats.avg()),
                format_duration(stats.min),
                format_duration(stats.max),
            ));
        }
        
        lines.join("\n")
    }
    
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

fn format_duration(d: Duration) -> String {
    if d.as_millis() > 0 {
        format!("{:.2}ms", d.as_secs_f64() * 1000.0)
    } else if d.as_micros() > 0 {
        format!("{:.1}µs", d.as_secs_f64() * 1_000_000.0)
    } else {
        format!("{}ns", d.as_nanos())
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[macro_export]
macro_rules! profile {
    ($profiler:expr, $name:expr, $block:block) => {{
        $profiler.begin($name);
        let result = $block;
        $profiler.end();
        result
    }};
}