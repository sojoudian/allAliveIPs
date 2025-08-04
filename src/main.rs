// Cargo.toml dependencies:
// [dependencies]
// tokio = { version = "1.0", features = ["full"] }
// futures = "0.3"
// clap = { version = "4.0", features = ["derive"] }

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::timeout;
use futures::future::join_all;

/// Result of a host scan
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub ip: Ipv4Addr,
    pub alive: bool,
    pub rtt: Option<Duration>,
    pub open_port: Option<u16>,
}

/// Scanner configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub subnet: String,
    pub timeout: Duration,
    pub max_concurrent: usize,
    pub start_ip: u8,
    pub end_ip: u8,
    pub ports: Vec<u16>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            subnet: "10.0.0".to_string(),
            timeout: Duration::from_millis(500),
            max_concurrent: num_cpus::get() * 8,
            start_ip: 1,
            end_ip: 254,
            ports: vec![22, 23, 53, 80, 135, 139, 443, 445, 993, 995],
        }
    }
}

/// High-performance network scanner
pub struct NetworkScanner {
    config: Config,
}

impl NetworkScanner {
    /// Create a new scanner with default configuration
    pub fn new(subnet: &str) -> Self {
        let mut config = Config::default();
        config.subnet = subnet.to_string();
        Self { config }
    }

    /// Create scanner with custom configuration
    pub fn with_config(config: Config) -> Self {
        Self { config }
    }

    /// Set maximum concurrent connections
    pub fn max_concurrent(mut self, max: usize) -> Self {
        self.config.max_concurrent = max;
        self
    }

    /// Set connection timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Set IP range to scan
    pub fn ip_range(mut self, start: u8, end: u8) -> Self {
        self.config.start_ip = start;
        self.config.end_ip = end;
        self
    }

    /// Set ports to test
    pub fn ports(mut self, ports: Vec<u16>) -> Self {
        self.config.ports = ports;
        self
    }

    /// Test if a host is alive by attempting TCP connections to common ports
    async fn test_host(&self, ip: Ipv4Addr) -> ScanResult {
        let start_time = Instant::now();
        
        // Try connecting to each port with a shorter timeout per port
        let per_port_timeout = self.config.timeout / self.config.ports.len() as u32;
        
        for &port in &self.config.ports {
            let addr = SocketAddr::new(IpAddr::V4(ip), port);
            
            match timeout(per_port_timeout, TcpStream::connect(addr)).await {
                Ok(Ok(_)) => {
                    let rtt = start_time.elapsed();
                    return ScanResult {
                        ip,
                        alive: true,
                        rtt: Some(rtt),
                        open_port: Some(port),
                    };
                }
                Ok(Err(_)) | Err(_) => continue,
            }
        }

        ScanResult {
            ip,
            alive: false,
            rtt: None,
            open_port: None,
        }
    }

    /// Perform network scan with progress reporting
    pub async fn scan_with_progress(&self) -> Result<Vec<ScanResult>, Box<dyn std::error::Error>> {
        let total_hosts = (self.config.end_ip - self.config.start_ip + 1) as u64;
        let completed = Arc::new(AtomicU64::new(0));
        let alive_count = Arc::new(AtomicU64::new(0));
        
        // Channel for collecting results
        let (tx, mut rx) = mpsc::unbounded_channel::<ScanResult>();
        
        // Semaphore to limit concurrent connections
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));
        
        println!("🚀 Scanning subnet {}.{}-{} with {} max concurrent connections...", 
                 self.config.subnet, self.config.start_ip, self.config.end_ip, self.config.max_concurrent);
        println!("📊 Testing ports: {:?}", self.config.ports);
        println!("⏱️  Timeout per host: {:?}\n", self.config.timeout);

        let start_time = Instant::now();

        // Spawn scanning tasks
        let mut tasks = Vec::new();
        
        for i in self.config.start_ip..=self.config.end_ip {
            let ip_str = format!("{}.{}", self.config.subnet, i);
            let ip: Ipv4Addr = ip_str.parse()?;
            
            let semaphore = semaphore.clone();
            let tx = tx.clone();
            let scanner = self.clone();
            let completed = completed.clone();
            let alive_count = alive_count.clone();
            let total = total_hosts;

            let task = tokio::spawn(async move {
                // Acquire semaphore permit
                let _permit = semaphore.acquire().await.unwrap();
                
                // Perform the scan
                let result = scanner.test_host(ip).await;
                
                // Update counters
                let current_completed = completed.fetch_add(1, Ordering::Relaxed) + 1;
                if result.alive {
                    alive_count.fetch_add(1, Ordering::Relaxed);
                }
                
                // Send result
                let _ = tx.send(result.clone());
                
                // Progress reporting
                if result.alive {
                    if let Some(port) = result.open_port {
                        println!("✅ Found: {} (port {}, RTT: {:?})", 
                                result.ip, port, result.rtt.unwrap_or_default());
                    }
                }
                
                // Progress indicator
                if current_completed % 25 == 0 || current_completed == total {
                    let alive = alive_count.load(Ordering::Relaxed);
                    let elapsed = start_time.elapsed();
                    let rate = current_completed as f64 / elapsed.as_secs_f64();
                    
                    println!("📈 Progress: {}/{} ({:.1}%) | Alive: {} | Rate: {:.0} hosts/sec", 
                             current_completed, total, 
                             (current_completed as f64 / total as f64) * 100.0,
                             alive, rate);
                }
            });
            
            tasks.push(task);
        }
        
        // Drop the original sender so the channel closes when all tasks complete
        drop(tx);
        
        // Collect results
        let mut results = Vec::new();
        while let Some(result) = rx.recv().await {
            results.push(result);
        }
        
        // Wait for all tasks to complete
        let _ = join_all(tasks).await;
        
        // Sort results by IP address
        results.sort_by_key(|r| r.ip);
        
        let elapsed = start_time.elapsed();
        let alive_hosts: Vec<_> = results.iter().filter(|r| r.alive).collect();
        
        println!("\n🎯 === SCAN COMPLETE ===");
        println!("⏰ Total time: {:?}", elapsed);
        println!("📊 Scanned {} hosts", total_hosts);
        println!("✅ Found {} alive hosts", alive_hosts.len());
        println!("🚀 Average rate: {:.0} hosts/second", total_hosts as f64 / elapsed.as_secs_f64());
        println!("📈 Success rate: {:.1}%\n", (alive_hosts.len() as f64 / total_hosts as f64) * 100.0);
        
        Ok(results)
    }

    /// Perform fast scan without progress reporting
    pub async fn scan(&self) -> Result<Vec<ScanResult>, Box<dyn std::error::Error>> {
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));
        let mut tasks = Vec::new();
        
        for i in self.config.start_ip..=self.config.end_ip {
            let ip_str = format!("{}.{}", self.config.subnet, i);
            let ip: Ipv4Addr = ip_str.parse()?;
            
            let semaphore = semaphore.clone();
            let scanner = self.clone();

            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                scanner.test_host(ip).await
            });
            
            tasks.push(task);
        }
        
        // Wait for all tasks and collect results
        let results: Result<Vec<_>, _> = join_all(tasks).await.into_iter().collect();
        let mut results = results?;
        
        // Sort by IP address
        results.sort_by_key(|r| r.ip);
        
        Ok(results)
    }

    /// Scan specific hosts
    pub async fn scan_hosts(&self, hosts: Vec<Ipv4Addr>) -> Result<Vec<ScanResult>, Box<dyn std::error::Error>> {
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));
        let mut tasks = Vec::new();
        
        for ip in hosts {
            let semaphore = semaphore.clone();
            let scanner = self.clone();

            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                scanner.test_host(ip).await
            });
            
            tasks.push(task);
        }
        
        let results: Result<Vec<_>, _> = join_all(tasks).await.into_iter().collect();
        let mut results = results?;
        results.sort_by_key(|r| r.ip);
        
        Ok(results)
    }
}

// Manual Clone implementation since we don't want to derive it
impl Clone for NetworkScanner {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}

/// Utility functions
impl NetworkScanner {
    /// Convert results to alive IPs only
    pub fn alive_ips(results: &[ScanResult]) -> Vec<Ipv4Addr> {
        results.iter()
            .filter(|r| r.alive)
            .map(|r| r.ip)
            .collect()
    }

    /// Get statistics from scan results
    pub fn get_stats(results: &[ScanResult]) -> ScanStats {
        let total = results.len();
        let alive = results.iter().filter(|r| r.alive).count();
        let avg_rtt = if alive > 0 {
            let total_rtt: Duration = results.iter()
                .filter_map(|r| r.rtt)
                .sum();
            Some(total_rtt / alive as u32)
        } else {
            None
        };

        ScanStats {
            total_hosts: total,
            alive_hosts: alive,
            success_rate: (alive as f64 / total as f64) * 100.0,
            average_rtt: avg_rtt,
        }
    }
}

#[derive(Debug)]
pub struct ScanStats {
    pub total_hosts: usize,
    pub alive_hosts: usize,
    pub success_rate: f64,
    pub average_rtt: Option<Duration>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create scanner with custom configuration
    let scanner = NetworkScanner::new("10.0.0")
        .max_concurrent(64)  // Aggressive parallelism
        .timeout(Duration::from_millis(300))  // Fast timeout
        .ip_range(1, 254)    // Full range
        .ports(vec![22, 80, 443, 135, 139, 445, 993, 995, 8080, 8443]); // Common ports

    // Perform scan with progress
    let results = scanner.scan_with_progress().await?;
    
    // Filter and display alive hosts
    let alive_hosts: Vec<_> = results.iter().filter(|r| r.alive).collect();
    
    if !alive_hosts.is_empty() {
        println!("🌐 === ALIVE HOSTS ===");
        for result in &alive_hosts {
            if let (Some(rtt), Some(port)) = (result.rtt, result.open_port) {
                println!("🟢 {} is alive (port {}, RTT: {:?})", result.ip, port, rtt);
            } else {
                println!("🟢 {} is alive", result.ip);
            }
        }
    } else {
        println!("❌ No alive hosts found in the specified range");
    }

    // Print statistics
    let stats = NetworkScanner::get_stats(&results);
    println!("\n📊 === STATISTICS ===");
    println!("📈 Success rate: {:.1}%", stats.success_rate);
    if let Some(avg_rtt) = stats.average_rtt {
        println!("⚡ Average RTT: {:?}", avg_rtt);
    }

    Ok(())
}

// Example usage for different scenarios
#[allow(dead_code)]
async fn example_usage() -> Result<(), Box<dyn std::error::Error>> {
    // Quick scan without progress
    let scanner = NetworkScanner::new("192.168.1");
    let results = scanner.scan().await?;
    let alive_ips = NetworkScanner::alive_ips(&results);
    println!("Found {} alive hosts", alive_ips.len());

    // Custom configuration
    let config = Config {
        subnet: "172.16.0".to_string(),
        timeout: Duration::from_millis(200),
        max_concurrent: 100,
        start_ip: 50,
        end_ip: 100,
        ports: vec![80, 443],
    };
    
    let custom_scanner = NetworkScanner::with_config(config);
    let _results = custom_scanner.scan_with_progress().await?;
    
    // Scan specific hosts
    let specific_hosts = vec![
        "8.8.8.8".parse()?,
        "1.1.1.1".parse()?,
        "208.67.222.222".parse()?,
    ];
    let _results = scanner.scan_hosts(specific_hosts).await?;
    
    Ok(())
}
