// package main

// import (
// 	"fmt"
// 	"os/exec"
// 	"sync"
// 	"time"
// )

// // pingIP tries to ping the given IP address and sends the result to the channel
// func pingIP(ip string, wg *sync.WaitGroup, results chan<- string) {
// 	defer wg.Done()

// 	// Execute ping command with a timeout of 1 second
// 	cmd := exec.Command("ping", "-c", "1", "-W", "1", ip)
// 	err := cmd.Run()

// 	if err == nil {
// 		results <- ip // Send IP to results channel if ping is successful
// 	}
// }

// func main() {
// 	// subnet := "10.0.0"
// 	subnet := "10.0.0"
// 	results := make(chan string, 254) // Buffered channel to hold results
// 	var wg sync.WaitGroup

// 	// Loop through IP addresses in subnet
// 	for i := 1; i <= 254; i++ {
// 		ip := fmt.Sprintf("%s.%d", subnet, i)

// 		// Add to wait group and spawn goroutine
// 		wg.Add(1)
// 		go pingIP(ip, &wg, results)
// 	}

// 	// Wait for all goroutines to finish
// 	go func() {
// 		wg.Wait()
// 		close(results)
// 	}()

// 	// Collect results
// 	for ip := range results {
// 		fmt.Printf("%s is alive\n", ip)
// 	}

// 	// Wait for 1 second to allow time for any residual goroutines to finish
// 	time.Sleep(1 * time.Second)
// }
package main

import (
	"fmt"
	"os/exec"
	"sort"
	"strconv"
	"strings"
	"sync"
)

func pingIP(ip string, wg *sync.WaitGroup, results chan<- string) {
	defer wg.Done()
	cmd := exec.Command("ping", "-c", "1", "-W", "1", ip)
	err := cmd.Run()
	if err == nil {
		results <- ip
	}
}

func ipToInt(ip string) int {
	parts := strings.Split(ip, ".")
	lastOctet, _ := strconv.Atoi(parts[3])
	return lastOctet
}

func main() {
	subnet := "10.0.0"
	results := make(chan string, 254)
	var wg sync.WaitGroup

	for i := 1; i <= 254; i++ {
		ip := fmt.Sprintf("%s.%d", subnet, i)
		wg.Add(1)
		go pingIP(ip, &wg, results)
	}

	go func() {
		wg.Wait()
		close(results)
	}()

	// Collect IPs into a slice
	var aliveIPs []string
	for ip := range results {
		aliveIPs = append(aliveIPs, ip)
	}

	// Sort IPs numerically by last octet
	sort.Slice(aliveIPs, func(i, j int) bool {
		return ipToInt(aliveIPs[i]) < ipToInt(aliveIPs[j])
	})

	// Print sorted results
	for _, ip := range aliveIPs {
		fmt.Printf("%s is alive\n", ip)
	}
}
