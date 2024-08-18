#!/bin/bash

# Network subnet
subnet="10.0.0"

# Loop through all possible hosts in the subnet
for i in $(seq 1 254); do
    ip="$subnet.$i"
    
    # Ping each IP with a timeout of 1 second, sending only 1 ping
    ping -c 1 -W 1 $ip > /dev/null 2>&1
    
    # If the ping was successful, print the IP
    if [ $? -eq 0 ]; then
        echo "$ip is alive"
    fi
done

