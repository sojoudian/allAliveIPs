use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::io::{self, Write};

/// Gets user input from stdin
fn get_input(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap(); // Ensure prompt is displayed before reading input
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

/// Validates IP subnet format
fn is_valid_subnet(subnet: &str) -> bool {
    let parts: Vec<&str> = subnet.split('.').collect();
    if parts.len() != 3 {
        return false;
    }

    parts.iter().all(|part| {
        if let Ok(num) = part.parse::<u8>() {
            num <= 255
        } else {
            false
        }
    })
}

/// Attempts to ping the given IP address and returns it if successful
fn ping_ip(ip: String) -> Option<String> {
    println!("Pinging: {}", ip);

    #[cfg(target_os = "windows")]
    let args = vec!["-n", "1", "-w", "1000", &ip];
    
    #[cfg(not(target_os = "windows"))]
    let args = vec!["-c", "1", "-W", "1", &ip];

    let output = Command::new("ping")
        .args(&args)
        .output();

    match output {
        Ok(result) => {
            let success = result.status.success();
            if success {
                println!("✓ {} is alive", ip);
                Some(ip)
            } else {
                None
            }
        }
        Err(e) => {
            println!("Error pinging {}: {}", ip, e);
            None
        }
    }
}

fn main() {
    println!("=== Network Scanner ===");
    
    // Get subnet from user
    let mut subnet = String::new();
    loop {
        subnet = get_input("Enter subnet to scan (e.g., 192.168.1): ");
        
        if is_valid_subnet(&subnet) {
            break;
        }
        println!("Invalid subnet format. Please use format like '192.168.1'");
    }

    println!("\nStarting scan of subnet: {}.1-254", subnet);
    println!("This may take a few minutes...\n");

    let results = Arc::new(Mutex::new(Vec::new()));
    let mut handles = vec![];

    // Show a simple progress indicator
    let total_ips = 254;
    let progress = Arc::new(Mutex::new(0));

    // Spawn threads for each IP in the range
    for i in 1..=254 {
        let ip = format!("{}.{}", subnet, i);
        let results = Arc::clone(&results);
        let progress = Arc::clone(&progress);

        let handle = thread::spawn(move || {
            if let Some(alive_ip) = ping_ip(ip) {
                let mut results = results.lock().unwrap();
                results.push(alive_ip);
            }
            
            // Update and show progress
            let mut progress = progress.lock().unwrap();
            *progress += 1;
            print!("\rProgress: {}/{}  ({:.1}%)", *progress, total_ips, (*progress as f32 / total_ips as f32) * 100.0);
            io::stdout().flush().unwrap();
        });

        handles.push(handle);
        
        // Add a small delay between spawns to prevent overwhelming the system
        thread::sleep(Duration::from_millis(10));
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Print final results
    println!("\n\nScan completed! Results:");
    println!("------------------------");
    
    let results = results.lock().unwrap();
    if results.is_empty() {
        println!("No responsive IPs found in subnet {}", subnet);
    } else {
        println!("Found {} active IPs:", results.len());
        for ip in results.iter() {
            println!("✓ {}", ip);
        }
    }
}
