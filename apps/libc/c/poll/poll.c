#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <poll.h>
#include <sys/select.h>
#include <sys/wait.h>
#include <signal.h>
#include <errno.h>
#include <string.h>
#include <time.h>
#include <fcntl.h>

// 信号处理函数
void signal_handler(int sig) {
    printf("Received signal %d\n", sig);
}

void test_ppoll() {
    printf("=== Testing ppoll ===\n");
    
    // 创建一个管道用于测试
    int pipefd[2];
    if (pipe(pipefd) == -1) {
        perror("pipe");
        return;
    }
    
    // 设置 pollfd 结构
    struct pollfd fds[1];
    fds[0].fd = pipefd[0];  // 读端
    fds[0].events = POLLIN; // 等待可读事件
    fds[0].revents = 0;
    
    // 设置超时时间 (2秒)
    struct timespec timeout;
    timeout.tv_sec = 2;
    timeout.tv_nsec = 0;
    
    // 设置信号掩码 (阻塞 SIGINT)
    sigset_t sigmask;
    sigemptyset(&sigmask);
    sigaddset(&sigmask, SIGINT);
    
    printf("Calling ppoll with 2 second timeout...\n");
    printf("(Child process will write data after 1 second)\n");
    
    // 在另一个进程中写入数据以触发事件
    if (fork() == 0) {
        // 子进程: 1秒后写入数据
        sleep(1);
        write(pipefd[1], "test data", 9);
        close(pipefd[1]);
        exit(0);
    }
    
    // 父进程: 调用 ppoll
    int result = ppoll(fds, 1, &timeout, &sigmask);
    
    if (result == -1) {
        perror("ppoll");
    } else if (result == 0) {
        printf("ppoll timeout\n");
    } else {
        printf("ppoll returned %d\n", result);
        if (fds[0].revents & POLLIN) {
            printf("Data available for reading\n");
            char buffer[100];
            ssize_t bytes = read(pipefd[0], buffer, sizeof(buffer)-1);
            if (bytes > 0) {
                buffer[bytes] = '\0';
                printf("Read: %s\n", buffer);
            }
        }
    }
    
    close(pipefd[0]);
    close(pipefd[1]);
    wait(NULL); // 等待子进程结束
}

void test_pselect6() {
    printf("\n=== Testing pselect6 ===\n");
    
    // 创建一个管道用于测试
    int pipefd[2];
    if (pipe(pipefd) == -1) {
        perror("pipe");
        return;
    }
    
    // 设置文件描述符集合
    fd_set readfds;
    FD_ZERO(&readfds);
    FD_SET(pipefd[0], &readfds);
    
    // 设置超时时间 (3秒)
    struct timespec timeout;
    timeout.tv_sec = 3;
    timeout.tv_nsec = 0;
    
    // 设置信号掩码 (阻塞 SIGUSR1)
    sigset_t sigmask;
    sigemptyset(&sigmask);
    sigaddset(&sigmask, SIGUSR1);
    
    printf("Calling pselect6 with 3 second timeout...\n");
    printf("(Child process will write data after 1.5 seconds)\n");
    
    // 在另一个进程中写入数据以触发事件
    if (fork() == 0) {
        // 子进程: 1.5秒后写入数据
        usleep(1500000); // 1.5秒
        write(pipefd[1], "pselect test", 12);
        close(pipefd[1]);
        exit(0);
    }
    
    // 父进程: 调用 pselect6
    int result = pselect(pipefd[0] + 1, &readfds, NULL, NULL, &timeout, &sigmask);
    
    if (result == -1) {
        perror("pselect");
    } else if (result == 0) {
        printf("pselect timeout\n");
    } else {
        printf("pselect returned %d\n", result);
        if (FD_ISSET(pipefd[0], &readfds)) {
            printf("Data available for reading\n");
            char buffer[100];
            ssize_t bytes = read(pipefd[0], buffer, sizeof(buffer)-1);
            if (bytes > 0) {
                buffer[bytes] = '\0';
                printf("Read: %s\n", buffer);
            }
        }
    }
    
    close(pipefd[0]);
    close(pipefd[1]);
    wait(NULL); // 等待子进程结束
}

void test_signal_handling() {
    printf("\n=== Testing signal handling with ppoll/pselect6 ===\n");
    
    // 创建管道
    int pipefd[2];
    if (pipe(pipefd) == -1) {
        perror("pipe");
        return;
    }
    
    // 设置信号处理器
    struct sigaction sa;
    sa.sa_handler = signal_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    sigaction(SIGUSR1, &sa, NULL);
    
    // 测试 ppoll 被信号中断
    printf("Testing ppoll with signal interruption...\n");
    
    if (fork() == 0) {
        // 子进程: 1秒后发送信号
        sleep(1);
        kill(getppid(), SIGUSR1);
        exit(0);
    }
    
    struct pollfd fds[1];
    fds[0].fd = pipefd[0];
    fds[0].events = POLLIN;
    fds[0].revents = 0;
    
    struct timespec timeout = {5, 0}; // 5秒超时
    sigset_t empty_mask;
    sigemptyset(&empty_mask);
    
    int result = ppoll(fds, 1, &timeout, &empty_mask);
    
    if (result == -1) {
        if (errno == EINTR) {
            printf("ppoll was interrupted by signal (expected)\n");
        } else {
            perror("ppoll");
        }
    } else {
        printf("ppoll returned %d (unexpected)\n", result);
    }
    
    wait(NULL); // 等待子进程结束
    
    // 测试 pselect6 被信号中断
    printf("\nTesting pselect6 with signal interruption...\n");
    
    if (fork() == 0) {
        // 子进程: 1秒后发送信号
        sleep(1);
        kill(getppid(), SIGUSR1);
        exit(0);
    }
    
    // 设置文件描述符集合
    fd_set readfds;
    FD_ZERO(&readfds);
    FD_SET(pipefd[0], &readfds);
    
    struct timespec pselect_timeout = {5, 0}; // 5秒超时
    
    result = pselect(pipefd[0] + 1, &readfds, NULL, NULL, &pselect_timeout, &empty_mask);
    
    if (result == -1) {
        if (errno == EINTR) {
            printf("pselect6 was interrupted by signal (expected)\n");
        } else {
            perror("pselect");
        }
    } else {
        printf("pselect returned %d (unexpected)\n", result);
    }
    
    close(pipefd[0]);
    close(pipefd[1]);
    wait(NULL); // 等待子进程结束
}

void print_usage(const char* prog_name) {
    printf("Usage: %s [options]\n", prog_name);
    printf("Options:\n");
    printf("  -p    Test ppoll only\n");
    printf("  -s    Test pselect6 only\n");
    printf("  -i    Test signal interruption\n");
    printf("  -a    Test all (default)\n");
    printf("  -h    Show this help\n");
}

int main(int argc, char* argv[]) {
    printf("Testing ppoll and pselect6 system calls\n");
    printf("========================================\n");
    
    int test_ppoll_flag = 0;
    int test_pselect_flag = 0;
    int test_signal_flag = 0;
    int test_all = 1;
    
    // 解析命令行参数
    int opt;
    while ((opt = getopt(argc, argv, "psiah")) != -1) {
        switch (opt) {
            case 'p':
                test_ppoll_flag = 1;
                test_all = 0;
                break;
            case 's':
                test_pselect_flag = 1;
                test_all = 0;
                break;
            case 'i':
                test_signal_flag = 1;
                test_all = 0;
                break;
            case 'a':
                test_all = 1;
                break;
            case 'h':
                print_usage(argv[0]);
                return 0;
            default:
                print_usage(argv[0]);
                return 1;
        }
    }
    
    if (test_all || test_ppoll_flag) {
        test_ppoll();
    }
    
    if (test_all || test_pselect_flag) {
        test_pselect6();
    }
    
    if (test_all || test_signal_flag) {
        test_signal_handling();
    }
    
    printf("\nTest completed.\n");
    return 0;
}