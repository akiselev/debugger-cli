// Multithreaded test program for debugger integration tests
#include <stdio.h>
#include <pthread.h>
#include <unistd.h>

#define NUM_THREADS 2

// Shared state
pthread_barrier_t barrier;
pthread_mutex_t counter_mutex = PTHREAD_MUTEX_INITIALIZER;
int shared_counter = 0;

// Helper function called AFTER barrier - safe to break here
// BREAKPOINT_MARKER: worker_body
void worker_body(int thread_id) {
    // BREAKPOINT_MARKER: worker_start
    pthread_mutex_lock(&counter_mutex);
    shared_counter++;
    int local_count = shared_counter;
    pthread_mutex_unlock(&counter_mutex);

    printf("Thread %d incremented counter to %d\n", thread_id, local_count);
    // BREAKPOINT_MARKER: worker_end
}

void* thread_func(void* arg) {
    int thread_id = *(int*)arg;

    // BREAKPOINT_MARKER: thread_entry (BEFORE barrier - do NOT break here)
    // Breaking here causes deadlock: debugger stops this thread while other threads
    // wait for all NUM_THREADS+1 threads (including stopped one) to reach barrier
    pthread_barrier_wait(&barrier);

    // BREAKPOINT_MARKER: after_barrier (SAFE to break here - all threads synchronized)
    worker_body(thread_id);
    return NULL;
}

int main(int argc, char *argv[]) {
    pthread_t threads[NUM_THREADS];
    int thread_ids[NUM_THREADS];

    // Initialize barrier for main thread + worker threads
    if (pthread_barrier_init(&barrier, NULL, NUM_THREADS + 1) != 0) {
        fprintf(stderr, "Failed to initialize barrier\n");
        return 1;
    }

    // BREAKPOINT_MARKER: main_start
    printf("Starting %d worker threads\n", NUM_THREADS);

    // Create worker threads
    for (int i = 0; i < NUM_THREADS; i++) {
        thread_ids[i] = i;
        if (pthread_create(&threads[i], NULL, thread_func, &thread_ids[i]) != 0) {
            fprintf(stderr, "Failed to create thread %d\n", i);
            return 1;
        }
    }

    // BREAKPOINT_MARKER: main_wait
    pthread_barrier_wait(&barrier);

    // Join all threads
    for (int i = 0; i < NUM_THREADS; i++) {
        pthread_join(threads[i], NULL);
    }

    printf("Final counter value: %d\n", shared_counter);
    pthread_barrier_destroy(&barrier);
    return 0;
}
