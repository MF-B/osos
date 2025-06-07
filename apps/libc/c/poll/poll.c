#include <poll.h>
#include <stdio.h>
#include <unistd.h>
#include <string.h>
#include <errno.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <fcntl.h>

void test_poll_stdin() {
    printf("Testing poll with stdin (should timeout)...\n");
    
    struct pollfd fds[1];
    fds[0].fd = STDIN_FILENO;
    fds[0].events = POLLIN;
    fds[0].revents = 0;
    
    // Test with 1 second timeout
    int ret = poll(fds, 1, 1000);
    
    if (ret == 0) {
        printf("✓ Poll timeout worked correctly\n");
    } else if (ret > 0) {
        printf("✓ Poll detected input on stdin\n");
    } else {
        printf("✗ Poll failed: %s\n", strerror(errno));
    }
}

void test_poll_pipe() {
    printf("\nTesting poll with pipe...\n");
    
    int pipefd[2];
    if (pipe(pipefd) == -1) {
        printf("✗ Failed to create pipe: %s\n", strerror(errno));
        return;
    }
    
    struct pollfd fds[2];
    // Read end of pipe
    fds[0].fd = pipefd[0];
    fds[0].events = POLLIN;
    fds[0].revents = 0;
    
    // Write end of pipe
    fds[1].fd = pipefd[1];
    fds[1].events = POLLOUT;
    fds[1].revents = 0;
    
    // Poll should return immediately - write end should be ready
    int ret = poll(fds, 2, 0);
    
    if (ret > 0) {
        if (fds[1].revents & POLLOUT) {
            printf("✓ Poll correctly detected writable pipe\n");
        }
        if (fds[0].revents & POLLIN) {
            printf("! Unexpected: read end shows data available\n");
        }
    } else if (ret == 0) {
        printf("✗ Poll timed out unexpectedly\n");
    } else {
        printf("✗ Poll failed: %s\n", strerror(errno));
    }
    
    // Write some data and test read readiness
    const char *msg = "test";
    write(pipefd[1], msg, strlen(msg));
    
    ret = poll(fds, 1, 100); // Only poll read end
    
    if (ret > 0 && (fds[0].revents & POLLIN)) {
        printf("✓ Poll correctly detected readable pipe after write\n");
    } else {
        printf("✗ Poll failed to detect readable pipe\n");
    }
    
    close(pipefd[0]);
    close(pipefd[1]);
}

void test_poll_invalid_fd() {
    printf("\nTesting poll with invalid fd...\n");
    
    struct pollfd fds[1];
    fds[0].fd = -1;  // Invalid fd
    fds[0].events = POLLIN;
    fds[0].revents = 0;
    
    int ret = poll(fds, 1, 100);
    
    if (ret >= 0 && (fds[0].revents & POLLNVAL)) {
        printf("✓ Poll correctly detected invalid fd\n");
    } else {
        printf("✗ Poll did not handle invalid fd correctly\n");
    }
}

void test_poll_zero_timeout() {
    printf("\nTesting poll with zero timeout...\n");
    
    struct pollfd fds[1];
    fds[0].fd = STDOUT_FILENO;
    fds[0].events = POLLOUT;
    fds[0].revents = 0;
    
    int ret = poll(fds, 1, 0);
    
    if (ret > 0 && (fds[0].revents & POLLOUT)) {
        printf("✓ Poll with zero timeout worked (stdout writable)\n");
    } else if (ret == 0) {
        printf("! Poll with zero timeout returned immediately (no events)\n");
    } else {
        printf("✗ Poll with zero timeout failed: %s\n", strerror(errno));
    }
}

void test_poll_negative_timeout() {
    printf("\nTesting poll with negative timeout (infinite wait - will timeout in 2s)...\n");
    
    int pipefd[2];
    if (pipe(pipefd) == -1) {
        printf("✗ Failed to create pipe: %s\n", strerror(errno));
        return;
    }
    
    struct pollfd fds[1];
    fds[0].fd = pipefd[0];
    fds[0].events = POLLIN;
    fds[0].revents = 0;
    
    // Fork to create a timeout mechanism
    pid_t pid = fork();
    if (pid == 0) {
        // Child process - sleep and write to pipe
        sleep(2);
        const char *msg = "timeout";
        write(pipefd[1], msg, strlen(msg));
        close(pipefd[1]);
        _exit(0);
    } else if (pid > 0) {
        // Parent process - poll with infinite timeout
        close(pipefd[1]); // Close write end in parent
        
        int ret = poll(fds, 1, -1);
        
        if (ret > 0 && (fds[0].revents & POLLIN)) {
            printf("✓ Poll with negative timeout worked (infinite wait)\n");
        } else {
            printf("✗ Poll with negative timeout failed\n");
        }
        
        close(pipefd[0]);
    } else {
        printf("✗ Fork failed: %s\n", strerror(errno));
        close(pipefd[0]);
        close(pipefd[1]);
    }
}

int main() {
    printf("=== Poll System Call Test Suite ===\n");
    
    test_poll_stdin();
    test_poll_pipe();
//    test_poll_invalid_fd();
    test_poll_zero_timeout();
    test_poll_negative_timeout();
    
    printf("\n=== Test Suite Complete ===\n");
    return 0;
}