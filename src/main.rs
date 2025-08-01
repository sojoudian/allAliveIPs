use std::process::Command;
use std::thread;
use std::sync::mpsc;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let subnet = "10.0.0";
    let (tx, rx) = mpsc::channel();

    for i in 1..=254 {
        let ip = format!("{}.{}", subnet, i);
        let tx_clone = tx.clone();
        thread::spawn(move || {
            let status = Command::new("ping")
                .args(&["-c", "1", "-W", "1", &ip])
                .status();
            if let Ok(s) = status {
                if s.success() {
                    let _ = tx_clone.send(ip);
                }
            }
        });
    }

    drop(tx);

    let mut alive_ips: Vec<String> = rx.into_iter().collect();
    alive_ips.sort_by_key(|ip| {
        ip.rsplit('.')
          .next()
          .and_then(|octet| octet.parse::<u8>().ok())
          .unwrap_or(0)
    });

    for ip in alive_ips {
        println!("{} is alive", ip);
    }

    Ok(())
}

