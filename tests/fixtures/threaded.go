// Multithreaded test program for debugger integration tests
package main

import (
	"fmt"
	"sync"
)

const numWorkers = 2

var sharedCounter int
var counterMutex sync.Mutex

func worker(id int, start chan bool, done *sync.WaitGroup) {
	defer done.Done()

	// BREAKPOINT_MARKER: thread_entry
	<-start

	// BREAKPOINT_MARKER: worker_start
	counterMutex.Lock()
	sharedCounter++
	localCount := sharedCounter
	counterMutex.Unlock()

	fmt.Printf("Worker %d incremented counter to %d\n", id, localCount)

	// BREAKPOINT_MARKER: worker_end
}

func main() {
	// BREAKPOINT_MARKER: main_start
	fmt.Printf("Starting %d workers\n", numWorkers)

	// Go lacks pthread_barrier equivalent in stdlib; buffered channel provides
	// deterministic start ordering without requiring all goroutines to synchronize.
	// Workers proceed independently after receiving start signal (differs from C
	// barrier which requires all threads to reach barrier before any proceed).
	startChan := make(chan bool, numWorkers)
	var wg sync.WaitGroup

	// Spawn workers
	for i := 0; i < numWorkers; i++ {
		wg.Add(1)
		go worker(i, startChan, &wg)
	}

	// BREAKPOINT_MARKER: main_wait
	// Signal all workers to start (deterministic execution)
	for i := 0; i < numWorkers; i++ {
		startChan <- true
	}

	wg.Wait()
	fmt.Printf("Final counter value: %d\n", sharedCounter)
}
