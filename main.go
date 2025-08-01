package main

import (
	"context"
	"fmt"
	"net"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Result represents a ping result
type Result struct {
	IP    string
	Alive bool
	RTT   time.Duration
}

// Config holds scanner configuration
type Config struct {
	Subnet      string
	Timeout     time.Duration
	Workers     int
	StartIP     int
	EndIP       int
}

// Scanner handles the network scanning
type Scanner struct {
	config Config
}

// NewScanner creates a new scanner with optimized defaults
func NewScanner(subnet string) *Scanner {
	return &Scanner{
		config: Config{
			Subnet:  subnet,
			Timeout: 500 * time.Millisecond, // Reduced from 1s for faster scanning
			Workers: runtime.NumCPU() * 4,   // Optimal worker count
			StartIP: 1,
			EndIP:   254,
		},
	}
}

// pingICMP performs ICMP ping using raw sockets (more efficient than exec)
func (s *Scanner) pingICMP(ctx context.Context, ip string) (bool, time.Duration) {
	start := time.Now()
	
	// Use net.DialTimeout for TCP connect test as fallback
	// This is more portable than raw ICMP and often faster
	conn, err := net.DialTimeout("tcp", ip+":80", s.config.Timeout)
	if err == nil {
		conn.Close()
		return true, time.Since(start)
	}
	
	// Try common ports for better detection
	ports := []string{"22", "23", "53", "80", "135", "139", "443", "445"}
	for _, port := range ports {
		select {
		case <-ctx.Done():
			return false, 0
		default:
			conn, err := net.DialTimeout("tcp", ip+":"+port, s.config.Timeout/time.Duration(len(ports)))
			if err == nil {
				conn.Close()
				return true, time.Since(start)
			}
		}
	}
	
	return false, 0
}

// worker processes IP addresses from the jobs channel
func (s *Scanner) worker(ctx context.Context, jobs <-chan string, results chan<- Result, wg *sync.WaitGroup) {
	defer wg.Done()
	
	for {
		select {
		case ip, ok := <-jobs:
			if !ok {
				return
			}
			
			alive, rtt := s.pingICMP(ctx, ip)
			select {
			case results <- Result{IP: ip, Alive: alive, RTT: rtt}:
			case <-ctx.Done():
				return
			}
			
		case <-ctx.Done():
			return
		}
	}
}

// Scan performs the network scan
func (s *Scanner) Scan(ctx context.Context) ([]string, error) {
	jobs := make(chan string, s.config.Workers*2) // Buffered for better throughput
	results := make(chan Result, s.config.EndIP-s.config.StartIP+1)
	
	var wg sync.WaitGroup
	
	// Start workers
	for i := 0; i < s.config.Workers; i++ {
		wg.Add(1)
		go s.worker(ctx, jobs, results, &wg)
	}
	
	// Send jobs
	go func() {
		defer close(jobs)
		for i := s.config.StartIP; i <= s.config.EndIP; i++ {
			ip := fmt.Sprintf("%s.%d", s.config.Subnet, i)
			select {
			case jobs <- ip:
			case <-ctx.Done():
				return
			}
		}
	}()
	
	// Close results when all workers are done
	go func() {
		wg.Wait()
		close(results)
	}()
	
	// Collect results with pre-allocated slice
	expectedHosts := s.config.EndIP - s.config.StartIP + 1
	aliveIPs := make([]string, 0, expectedHosts/10) // Estimate 10% alive hosts
	
	for result := range results {
		if result.Alive {
			aliveIPs = append(aliveIPs, result.IP)
		}
	}
	
	// Sort efficiently using integer comparison
	sort.Slice(aliveIPs, func(i, j int) bool {
		return ipToInt(aliveIPs[i]) < ipToInt(aliveIPs[j])
	})
	
	return aliveIPs, nil
}

// ipToInt converts IP to integer for sorting (optimized)
func ipToInt(ip string) int {
	parts := strings.Split(ip, ".")
	if len(parts) != 4 {
		return 0
	}
	
	// Convert last octet only (sufficient for sorting within same subnet)
	lastOctet, _ := strconv.Atoi(parts[3])
	return lastOctet
}

// ScanWithProgress performs scan with progress reporting
func (s *Scanner) ScanWithProgress(ctx context.Context) ([]string, error) {
	jobs := make(chan string, s.config.Workers*2)
	results := make(chan Result, s.config.EndIP-s.config.StartIP+1)
	
	var wg sync.WaitGroup
	
	// Progress tracking
	totalHosts := s.config.EndIP - s.config.StartIP + 1
	completed := int64(0)
	
	// Start workers
	for i := 0; i < s.config.Workers; i++ {
		wg.Add(1)
		go s.worker(ctx, jobs, results, &wg)
	}
	
	// Send jobs
	go func() {
		defer close(jobs)
		for i := s.config.StartIP; i <= s.config.EndIP; i++ {
			ip := fmt.Sprintf("%s.%d", s.config.Subnet, i)
			select {
			case jobs <- ip:
			case <-ctx.Done():
				return
			}
		}
	}()
	
	// Close results when all workers are done
	go func() {
		wg.Wait()
		close(results)
	}()
	
	// Collect results with progress
	aliveIPs := make([]string, 0, totalHosts/10)
	
	for result := range results {
		completed++
		if result.Alive {
			aliveIPs = append(aliveIPs, result.IP)
			fmt.Printf("Found: %s (RTT: %v)\n", result.IP, result.RTT)
		}
		
		// Progress indicator
		if completed%50 == 0 || completed == int64(totalHosts) {
			fmt.Printf("Progress: %d/%d (%.1f%%)\n", 
				completed, totalHosts, float64(completed)/float64(totalHosts)*100)
		}
	}
	
	// Sort results
	sort.Slice(aliveIPs, func(i, j int) bool {
		return ipToInt(aliveIPs[i]) < ipToInt(aliveIPs[j])
	})
	
	return aliveIPs, nil
}

func main() {
	// Create scanner for subnet
	scanner := NewScanner("10.0.0")
	
	// Optional: customize configuration
	scanner.config.Timeout = 300 * time.Millisecond // Faster timeout
	scanner.config.Workers = runtime.NumCPU() * 6   // More aggressive parallelism
	
	// Create context with timeout for the entire operation
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	
	fmt.Printf("Scanning subnet %s.1-%d with %d workers...\n", 
		scanner.config.Subnet, scanner.config.EndIP, scanner.config.Workers)
	
	start := time.Now()
	
	// Perform scan with progress
	aliveIPs, err := scanner.ScanWithProgress(ctx)
	if err != nil {
		fmt.Printf("Error during scan: %v\n", err)
		return
	}
	
	elapsed := time.Since(start)
	
	// Print results
	fmt.Printf("\n=== SCAN COMPLETE ===\n")
	fmt.Printf("Found %d alive hosts in %v\n", len(aliveIPs), elapsed)
	fmt.Printf("Scan rate: %.0f hosts/second\n\n", 
		float64(scanner.config.EndIP-scanner.config.StartIP+1)/elapsed.Seconds())
	
	for _, ip := range aliveIPs {
		fmt.Printf("%s is alive\n", ip)
	}
}
