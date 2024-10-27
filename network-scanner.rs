use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Attempts to ping the given IP address and returns it if successful
fn ping_ip(ip: String) -> Option<String> {
    #[cfg(target_os = "windows")]
    let args = vec!["-n", "1", "-w", "1000", &ip];
    
    #[cfg(not(target_os = "windows"))]
    let args = vec!["-c", "1", "-W", "1", &ip];

    let output = Command::new("ping")
        .args(&args)
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                Some(ip)
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

fn main() {
    //let subnet = "10.0.0";
    let subnet = "172.20";
    let results = Arc::new(Mutex::new(Vec::new()));
    let mut handles = vec![];

    // Spawn threads for each IP in the range
    for i in 1..=254 {
        let ip = format!("{}.{}", subnet, i);
        let results = Arc::clone(&results);

        let handle = thread::spawn(move || {
            if let Some(alive_ip) = ping_ip(ip) {
                let mut results = results.lock().unwrap();
                results.push(alive_ip);
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Print results
    let results = results.lock().unwrap();
    for ip in results.iter() {
        println!("{} is alive", ip);
    }

    // Wait a moment before exiting
    thread::sleep(Duration::from_secs(1));
}
