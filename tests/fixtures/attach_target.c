// Long-running target for attach mode tests
// Runs for 30 seconds with 1-second sleep intervals, allowing time for debugger attach
#include <stdio.h>
#include <unistd.h>

int main() {
    // Print PID for test harness to capture
    printf("PID: %d\n", getpid());
    fflush(stdout);

    // Run for 30 seconds - provides margin for attach operation
    // 30s chosen: attach completes <2s locally, 15x safety margin for slow CI
    for (int i = 0; i < 30; i++) {
        // BREAKPOINT_MARKER: loop_body
        int counter = i;
        (void)counter;  // Prevent optimization
        sleep(1);
    }

    return 0;
}
