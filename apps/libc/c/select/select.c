#include <sys/select.h>
#include <sys/time.h>
#include <signal.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <string.h>

static volatile int signal_received = 0;

void signal_handler(int sig) {
    signal_received = 1;
    printf("Signal %d received during pselect6\n", sig);
}

int test_pselect6_basic() {
    printf("=== Testing basic pselect6 functionality ===\n");
    
    fd_set readfds;
    struct timespec timeout;
    int ret;
    
    // 测试标准输入的可读性，超时设置为2秒
    FD_ZERO(&readfds);
    FD_SET(STDIN_FILENO, &readfds);
    
    timeout.tv_sec = 2;
    timeout.tv_nsec = 0;
    
    printf("Waiting for input on stdin (timeout: 2 seconds)...\n");
    printf("Type something or wait for timeout: ");
    fflush(stdout);
    
    ret = pselect(STDIN_FILENO + 1, &readfds, NULL, NULL, &timeout, NULL);
    
    if (ret == -1) {
        perror("pselect6 failed");
        return -1;
    } else if (ret == 0) {
        printf("\nTimeout occurred\n");
    } else {
        if (FD_ISSET(STDIN_FILENO, &readfds)) {
            printf("\nStdin is ready for reading\n");
            // 清空输入缓冲区
            char buffer[256];
            read(STDIN_FILENO, buffer, sizeof(buffer));
        }
    }
    
    return 0;
}

int test_pselect6_with_signal() {
    printf("\n=== Testing pselect6 with signal handling ===\n");
    
    fd_set readfds;
    struct timespec timeout;
    sigset_t sigmask, oldmask;
    int ret;
    
    // 设置信号处理器
    signal(SIGALRM, signal_handler);
    
    // 创建信号掩码，阻塞SIGALRM
    sigemptyset(&sigmask);
    sigaddset(&sigmask, SIGALRM);
    sigprocmask(SIG_BLOCK, &sigmask, &oldmask);
    
    // 设置定时器，3秒后发送SIGALRM
    alarm(3);
    
    FD_ZERO(&readfds);
    FD_SET(STDIN_FILENO, &readfds);
    
    timeout.tv_sec = 10;  // 长超时
    timeout.tv_nsec = 0;
    
    printf("Waiting for input (will be interrupted by SIGALRM in 3 seconds)...\n");
    printf("Type something: ");
    fflush(stdout);
    
    // pselect6 会临时使用oldmask，允许SIGALRM中断
    ret = pselect(STDIN_FILENO + 1, &readfds, NULL, NULL, &timeout, &oldmask);
    
    if (ret == -1) {
        if (errno == EINTR) {
            printf("\npselect6 was interrupted by signal\n");
        } else {
            perror("pselect6 failed");
        }
    } else if (ret == 0) {
        printf("\nTimeout occurred\n");
    } else {
        printf("\nStdin is ready for reading\n");
    }
    
    // 恢复原来的信号掩码
    sigprocmask(SIG_SETMASK, &oldmask, NULL);
    alarm(0);  // 取消定时器
    
    return 0;
}

int test_pselect6_multiple_fds() {
    printf("\n=== Testing pselect6 with multiple file descriptors ===\n");
    
    int pipefd[2];
    fd_set readfds, writefds;
    struct timespec timeout;
    int ret;
    
    // 创建管道
    if (pipe(pipefd) == -1) {
        perror("pipe failed");
        return -1;
    }
    
    FD_ZERO(&readfds);
    FD_ZERO(&writefds);
    FD_SET(pipefd[0], &readfds);    // 读端
    FD_SET(pipefd[1], &writefds);   // 写端
    FD_SET(STDIN_FILENO, &readfds); // 标准输入
    
    timeout.tv_sec = 5;
    timeout.tv_nsec = 0;
    
    printf("Testing multiple file descriptors (pipe + stdin)...\n");
    printf("Pipe write end should be ready immediately\n");
    
    int maxfd = (pipefd[1] > STDIN_FILENO) ? pipefd[1] : STDIN_FILENO;
    ret = pselect(maxfd + 1, &readfds, &writefds, NULL, &timeout, NULL);
    
    if (ret == -1) {
        perror("pselect6 failed");
        close(pipefd[0]);
        close(pipefd[1]);
        return -1;
    } else if (ret == 0) {
        printf("Timeout occurred\n");
    } else {
        printf("Ready file descriptors: %d\n", ret);
        
        if (FD_ISSET(pipefd[0], &readfds)) {
            printf("Pipe read end is ready\n");
        }
        if (FD_ISSET(pipefd[1], &writefds)) {
            printf("Pipe write end is ready\n");
        }
        if (FD_ISSET(STDIN_FILENO, &readfds)) {
            printf("Stdin is ready\n");
        }
    }
    
    close(pipefd[0]);
    close(pipefd[1]);
    return 0;
}

int test_pselect6_error_cases() {
    printf("\n=== Testing pselect6 error cases ===\n");
    
    fd_set readfds;
    struct timespec timeout;
    int ret;
    
    // 测试无效的文件描述符
    FD_ZERO(&readfds);
    FD_SET(999, &readfds);  // 假设999是无效的fd
    
    timeout.tv_sec = 1;
    timeout.tv_nsec = 0;
    
    printf("Testing with invalid file descriptor...\n");
    ret = pselect(1000, &readfds, NULL, NULL, &timeout, NULL);
    
    if (ret == -1) {
        printf("Expected error occurred: %s\n", strerror(errno));
    } else {
        printf("Unexpected success with invalid fd\n");
    }
    
    // 测试无效的超时值
    printf("Testing with invalid timeout...\n");
    timeout.tv_sec = -1;
    timeout.tv_nsec = 0;
    
    FD_ZERO(&readfds);
    FD_SET(STDIN_FILENO, &readfds);
    
    ret = pselect(STDIN_FILENO + 1, &readfds, NULL, NULL, &timeout, NULL);
    
    if (ret == -1) {
        printf("Expected error with invalid timeout: %s\n", strerror(errno));
    } else {
        printf("Unexpected success with invalid timeout\n");
    }
    
    return 0;
}

int main() {
    printf("pselect6 System Call Test Program\n");
    printf("==================================\n\n");
    
    // 基本功能测试
    if (test_pselect6_basic() != 0) {
        return 1;
    }
    
    // 信号处理测试
    if (test_pselect6_with_signal() != 0) {
        return 1;
    }
    
    // 多文件描述符测试
    if (test_pselect6_multiple_fds() != 0) {
        return 1;
    }
    
    // 错误情况测试
    test_pselect6_error_cases();
    
    printf("\n=== All tests completed ===\n");
    return 0;
}