#include <sys/select.h>
#include <sys/time.h>
#include <sys/wait.h>
#include <unistd.h>
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <pthread.h>
#include <sys/syscall.h>
#include <linux/futex.h>
#include <stdatomic.h>
#include <assert.h>
#include <limits.h>

// 全局变量用于信号处理
static volatile sig_atomic_t signal_received = 0;

// 信号处理函数
void signal_handler(int sig) {
    signal_received = 1;
}

// 测试 pselect6 的功能
void test_pselect6_basic() {
    printf("=== 测试 pselect6 基本功能 ===\n");
    
    int pipe_fds[2];
    if (pipe(pipe_fds) == -1) {
        perror("pipe");
        return;
    }
    
    fd_set readfds;
    FD_ZERO(&readfds);
    FD_SET(pipe_fds[0], &readfds);
    
    struct timespec timeout = {1, 0}; // 1秒超时
    sigset_t sigmask;
    sigemptyset(&sigmask);
    
    printf("测试超时机制...\n");
    struct timespec start, end;
    clock_gettime(CLOCK_MONOTONIC, &start);
    
    int result = pselect(pipe_fds[0] + 1, &readfds, NULL, NULL, &timeout, &sigmask);
    
    clock_gettime(CLOCK_MONOTONIC, &end);
    long elapsed_ms = (end.tv_sec - start.tv_sec) * 1000 + 
                      (end.tv_nsec - start.tv_nsec) / 1000000;
    
    printf("pselect 返回值: %d, 耗时: %ld ms\n", result, elapsed_ms);
    
    if (result == 0 && elapsed_ms >= 950 && elapsed_ms <= 1050) {
        printf("✓ 超时机制正常\n");
    } else {
        printf("✗ 超时机制异常\n");
    }
    
    close(pipe_fds[0]);
    close(pipe_fds[1]);
}

void test_pselect6_signal_mask() {
    printf("=== 测试 pselect6 信号掩码 ===\n");
    
    signal_received = 0;
    
    // 设置信号处理器
    struct sigaction sa;
    sa.sa_handler = signal_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    sigaction(SIGUSR1, &sa, NULL);
    
    int pipe_fds[2];
    if (pipe(pipe_fds) == -1) {
        perror("pipe");
        return;
    }
    
    // 阻塞 SIGUSR1
    sigset_t oldmask, newmask;
    sigemptyset(&newmask);
    sigaddset(&newmask, SIGUSR1);
    pthread_sigmask(SIG_BLOCK, &newmask, &oldmask);
    
    // 在 pselect 中解除阻塞
    sigset_t pselect_mask;
    sigemptyset(&pselect_mask);
    
    fd_set readfds;
    FD_ZERO(&readfds);
    FD_SET(pipe_fds[0], &readfds);
    
    struct timespec timeout = {3, 0}; // 3秒超时
    
    // 子进程发送信号
    pid_t pid = fork();
    if (pid == 0) {
        sleep(1);
        printf("子进程发送 SIGUSR1 信号...\n");
        kill(getppid(), SIGUSR1);
        exit(0);
    }
    
    printf("等待信号中断 pselect (当前信号被阻塞)...\n");
    errno = 0;
    int result = pselect(pipe_fds[0] + 1, &readfds, NULL, NULL, &timeout, &pselect_mask);
    int saved_errno = errno;
    
    printf("pselect 返回值: %d, errno: %d (%s)\n", result, saved_errno, strerror(saved_errno));
    printf("信号接收标志: %d\n", signal_received);
    
    if (result == -1 && saved_errno == EINTR && signal_received) {
        printf("✓ 信号正确中断了 pselect\n");
    } else {
        printf("✗ 信号未能正确中断 pselect\n");
    }
    
    // 恢复信号掩码
    pthread_sigmask(SIG_SETMASK, &oldmask, NULL);
    close(pipe_fds[0]);
    close(pipe_fds[1]);
    
    int status;
    waitpid(pid, &status, 0);
}

// 测试文件描述符集合的原子性
void test_pselect6_atomic_fdset() {
    printf("=== 测试 pselect6 文件描述符集合原子性 ===\n");
    
    int pipe_fds[2];
    if (pipe(pipe_fds) == -1) {
        perror("pipe");
        return;
    }
    
    fd_set readfds, writefds, exceptfds;
    FD_ZERO(&readfds);
    FD_ZERO(&writefds);
    FD_ZERO(&exceptfds);
    
    FD_SET(pipe_fds[0], &readfds);
    FD_SET(pipe_fds[1], &writefds);
    
    struct timespec timeout = {0, 100000000}; // 100ms
    
    // 管道应该立即可写
    int result = pselect(pipe_fds[1] + 1, &readfds, &writefds, &exceptfds, &timeout, NULL);
    
    printf("pselect 返回值: %d\n", result);
    printf("读端状态: %s\n", FD_ISSET(pipe_fds[0], &readfds) ? "就绪" : "未就绪");
    printf("写端状态: %s\n", FD_ISSET(pipe_fds[1], &writefds) ? "就绪" : "未就绪");
    
    if (result > 0 && FD_ISSET(pipe_fds[1], &writefds) && !FD_ISSET(pipe_fds[0], &readfds)) {
        printf("✓ 文件描述符集合操作正确\n");
    } else {
        printf("✗ 文件描述符集合操作异常\n");
    }
    
    close(pipe_fds[0]);
    close(pipe_fds[1]);
}

// 测试 pselect6 边界条件
void test_pselect6_edge_cases() {
    printf("=== 测试 pselect6 边界条件 ===\n");
    
    // 测试空的文件描述符集合
    struct timespec timeout = {0, 10000000}; // 10ms
    int result = pselect(0, NULL, NULL, NULL, &timeout, NULL);
    
    printf("空 fd 集合测试: 返回值 %d\n", result);
    if (result == 0) {
        printf("✓ 空 fd 集合处理正确\n");
    } else {
        printf("✗ 空 fd 集合处理异常\n");
    }
    
    // 测试无效的文件描述符
    fd_set readfds;
    FD_ZERO(&readfds);
    FD_SET(999, &readfds); // 假设这是无效的 fd
    
    timeout.tv_sec = 0;
    timeout.tv_nsec = 10000000; // 10ms
    result = pselect(1000, &readfds, NULL, NULL, &timeout, NULL);
    
    printf("无效 fd 测试: 返回值 %d, errno: %d\n", result, errno);
    if (result == -1 && errno == EBADF) {
        printf("✓ 无效 fd 处理正确\n");
    } else {
        printf("✗ 无效 fd 处理异常\n");
    }
}

// Futex 测试相关的全局变量
static atomic_int futex_var = 0;
static atomic_int wake_count = 0;
static volatile int test_running = 1;

// Futex 系统调用包装
long futex_syscall(int *uaddr, int futex_op, int val, 
                   const struct timespec *timeout, int *uaddr2, int val3) {
    return syscall(SYS_futex, uaddr, futex_op, val, timeout, uaddr2, val3);
}

void* futex_waiter_thread(void* arg) {
    int thread_id = *(int*)arg;
    
    while (test_running) {
        // 等待 futex_var 变为非零
        int current_val = atomic_load(&futex_var);
        if (current_val == 0) {
            printf("线程 %d 开始等待 futex\n", thread_id);
            long ret = futex_syscall((int*)&futex_var, FUTEX_WAIT, 0, NULL, NULL, 0);
            if (ret == 0) {
                printf("线程 %d 被唤醒\n", thread_id);
                atomic_fetch_add(&wake_count, 1);
            } else if (errno == EAGAIN) {
                printf("线程 %d EAGAIN (值已改变)\n", thread_id);
            } else {
                printf("线程 %d 等待失败: %s\n", thread_id, strerror(errno));
            }
        }
        usleep(10000); // 10ms
    }
    return NULL;
}

void test_futex_wake_wait() {
    printf("=== 测试 Futex WAIT/WAKE 基本操作 ===\n");
    
    atomic_store(&futex_var, 0);
    atomic_store(&wake_count, 0);
    test_running = 1;
    
    const int num_threads = 4;
    pthread_t threads[num_threads];
    int thread_ids[num_threads];
    
    // 创建等待线程
    for (int i = 0; i < num_threads; i++) {
        thread_ids[i] = i;
        if (pthread_create(&threads[i], NULL, futex_waiter_thread, &thread_ids[i]) != 0) {
            perror("pthread_create");
            return;
        }
    }
    
    sleep(1); // 确保所有线程都开始等待
    
    printf("唤醒 2 个等待线程...\n");
    atomic_store(&futex_var, 1);
    long wake_ret = futex_syscall((int*)&futex_var, FUTEX_WAKE, 2, NULL, NULL, 0);
    printf("FUTEX_WAKE 返回值: %ld (应该是唤醒的线程数)\n", wake_ret);
    
    sleep(1);
    
    printf("唤醒所有剩余线程...\n");
    long wake_all_ret = futex_syscall((int*)&futex_var, FUTEX_WAKE, INT_MAX, NULL, NULL, 0);
    printf("FUTEX_WAKE_ALL 返回值: %ld\n", wake_all_ret);
    
    test_running = 0;
    
    // 等待所有线程结束
    for (int i = 0; i < num_threads; i++) {
        pthread_join(threads[i], NULL);
    }
    
    int final_wake_count = atomic_load(&wake_count);
    printf("总共唤醒的线程数: %d\n", final_wake_count);
    
    if (final_wake_count >= 2) {
        printf("✓ Futex WAKE/WAIT 基本功能正常\n");
    } else {
        printf("✗ Futex WAKE/WAIT 功能异常\n");
    }
}

void test_futex_timeout() {
    printf("=== 测试 Futex 超时机制 ===\n");
    
    atomic_store(&futex_var, 0);
    
    struct timespec timeout = {0, 500000000}; // 500ms
    struct timespec start, end;
    
    clock_gettime(CLOCK_MONOTONIC, &start);
    errno = 0;
    long ret = futex_syscall((int*)&futex_var, FUTEX_WAIT, 0, &timeout, NULL, 0);
    int saved_errno = errno;
    clock_gettime(CLOCK_MONOTONIC, &end);
    
    long elapsed_ms = (end.tv_sec - start.tv_sec) * 1000 + 
                      (end.tv_nsec - start.tv_nsec) / 1000000;
    
    printf("Futex 超时返回值: %ld, 耗时: %ld ms, errno: %d (%s)\n", 
           ret, elapsed_ms, saved_errno, strerror(saved_errno));
    
    if (ret == -1 && saved_errno == ETIMEDOUT && elapsed_ms >= 450 && elapsed_ms <= 550) {
        printf("✓ Futex 超时机制正常\n");
    } else {
        printf("✗ Futex 超时机制异常\n");
    }
}

// 测试竞争条件
static atomic_int race_counter = 0;
static atomic_int race_futex = 0;

void* race_test_thread(void* arg) {
    int thread_id = *(int*)arg;
    
    for (int i = 0; i < 100; i++) { // 减少循环次数以便观察
        // 模拟竞争条件
        while (1) {
            int expected = 0;
            if (atomic_compare_exchange_weak(&race_futex, &expected, 1)) {
                // 获取到锁，进行临界区操作
                atomic_fetch_add(&race_counter, 1);
                usleep(100); // 模拟工作
                atomic_store(&race_futex, 0);
                
                // 唤醒等待者
                futex_syscall((int*)&race_futex, FUTEX_WAKE, 1, NULL, NULL, 0);
                break;
            } else {
                // 未获取到锁，等待
                futex_syscall((int*)&race_futex, FUTEX_WAIT, 1, NULL, NULL, 0);
            }
        }
    }
    return NULL;
}

void test_futex_race_conditions() {
    printf("=== 测试 Futex 竞争条件处理 ===\n");
    
    atomic_store(&race_counter, 0);
    atomic_store(&race_futex, 0);
    
    const int num_threads = 4;
    pthread_t threads[num_threads];
    int thread_ids[num_threads];
    
    for (int i = 0; i < num_threads; i++) {
        thread_ids[i] = i;
        pthread_create(&threads[i], NULL, race_test_thread, &thread_ids[i]);
    }
    
    for (int i = 0; i < num_threads; i++) {
        pthread_join(threads[i], NULL);
    }
    
    int final_counter = atomic_load(&race_counter);
    int expected = num_threads * 100;
    printf("最终计数器值: %d (期望: %d)\n", final_counter, expected);
    
    if (final_counter == expected) {
        printf("✓ Futex 竞争条件处理正确\n");
    } else {
        printf("✗ Futex 存在竞争条件问题\n");
    }
}

int main() {
    printf("开始系统调用测试...\n\n");
    
    // 测试 pselect6
    test_pselect6_basic();
    printf("\n");
    
    test_pselect6_signal_mask();
    printf("\n");
    
    test_pselect6_atomic_fdset();
    printf("\n");
    
    test_pselect6_edge_cases();
    printf("\n");
    
    // 测试 futex
    test_futex_wake_wait();
    printf("\n");
    
    test_futex_timeout();
    printf("\n");
    
    test_futex_race_conditions();
    printf("\n");
    
    printf("所有测试完成!\n");
    return 0;
}