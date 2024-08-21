#!/bin/bash

# Network subnet
subnet="172.20.10"  

# Function to ping a single IP
ping_ip() {
    ip="$1"
    ping -c 1 -W 1 $ip > /dev/null 2>&1
    if [ $? -eq 0 ]; then
        echo "$ip is alive"
    fi
}

# Loop through all possible hosts in the subnet
for i in $(seq 1 254); do
    ip="$subnet.$i"
    
    # Ping each IP in the background
    ping_ip $ip &
    
    # Limit the number of concurrent jobs to avoid overwhelming the system
    while [[ $(jobs -r | wc -l) -ge 100 ]]; do
        sleep 0.1  # Wait briefly before checking again
    done
done

# Wait for all background jobs to complete before exiting
wait