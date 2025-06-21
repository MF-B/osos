#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>

#define TEST_FILE "fsync_test.txt"
#define BUFFER_SIZE 1024

int main() {
    int fd;
    ssize_t bytes_written, total_written = 0;
    char write_buffer[BUFFER_SIZE];
    char read_buffer[BUFFER_SIZE];
    int fsync_result;
    
    printf("开始测试伪实现的 fsync...\n");
    
    // 准备测试数据
    memset(write_buffer, 'A', BUFFER_SIZE - 1);
    write_buffer[BUFFER_SIZE - 1] = '\0';
    
    // 创建并写入测试文件
    printf("创建并写入测试文件...\n");
    fd = open(TEST_FILE, O_CREAT | O_WRONLY | O_TRUNC, 0644);
    if (fd == -1) {
        perror("open 失败");
        return EXIT_FAILURE;
    }
    
    // 处理部分写入的情况
    while (total_written < BUFFER_SIZE - 1) {
        bytes_written = write(fd, write_buffer + total_written, (BUFFER_SIZE - 1) - total_written);
        if (bytes_written <= 0) {
            if (errno == EINTR) continue; // 被信号中断，重试
            perror("write 失败");
            close(fd);
            return EXIT_FAILURE;
        }
        total_written += bytes_written;
        printf("写入进度: %zd/%d 字节\n", total_written, BUFFER_SIZE - 1);
    }
    
    // 调用 fsync
    printf("调用 fsync...\n");
    fsync_result = fsync(fd);
    printf("fsync 返回值: %d (errno: %d)\n", fsync_result, errno);
    
    // 关闭文件
    close(fd);
    
    // 重新打开文件并验证数据
    printf("重新打开文件并验证数据...\n");
    fd = open(TEST_FILE, O_RDONLY);
    if (fd == -1) {
        perror("重新打开失败");
        return EXIT_FAILURE;
    }
    
    memset(read_buffer, 0, BUFFER_SIZE);
    ssize_t total_read = 0;
    ssize_t bytes_read;
    
    // 处理部分读取的情况
    while (total_read < BUFFER_SIZE - 1) {
        bytes_read = read(fd, read_buffer + total_read, (BUFFER_SIZE - 1) - total_read);
        if (bytes_read < 0) {
            perror("read 失败");
            close(fd);
            return EXIT_FAILURE;
        }
        if (bytes_read == 0) break; // 文件结束
        total_read += bytes_read;
    }
    
    close(fd);
    
    if (total_read != BUFFER_SIZE - 1) {
        printf("读取失败: 期望读取 %d 字节，实际读取 %zd 字节\n", 
               BUFFER_SIZE - 1, total_read);
        return EXIT_FAILURE;
    }
    
    // 比较写入和读取的数据
    if (strcmp(write_buffer, read_buffer) != 0) {
        printf("数据验证失败: 数据不匹配\n");
        printf("写入: %.20s...\n", write_buffer);
        printf("读取: %.20s...\n", read_buffer);
        return EXIT_FAILURE;
    }
    
    printf("基本读写测试通过!\n");
    
    // 测试场景二：模拟崩溃恢复
    printf("\n测试场景二：模拟崩溃恢复...\n");
    
    // 创建新文件并只写入部分数据
    fd = open(TEST_FILE, O_CREAT | O_WRONLY | O_TRUNC, 0644);
    if (fd == -1) {
        perror("open 失败");
        return EXIT_FAILURE;
    }
    
    // 写入部分数据并调用 fsync
    bytes_written = write(fd, "PART1", 5);
    if (bytes_written != 5) {
        perror("write PART1 失败");
        close(fd);
        return EXIT_FAILURE;
    }
    
    printf("写入第一部分数据并调用 fsync...\n");
    fsync_result = fsync(fd);
    printf("fsync 返回值: %d (errno: %d)\n", fsync_result, errno);
    
    // 写入更多数据但不调用 fsync (模拟崩溃前未完成的写入)
    bytes_written = write(fd, "PART2", 5);
    if (bytes_written != 5) {
        perror("write PART2 失败");
        close(fd);
        return EXIT_FAILURE;
    }
    
    printf("写入第二部分数据但不调用 fsync (模拟崩溃)...\n");
    
    // 直接关闭文件 (模拟崩溃)
    close(fd);
    
    // 再次打开文件检查内容 (模拟崩溃后恢复)
    printf("模拟崩溃后恢复，检查文件内容...\n");
    fd = open(TEST_FILE, O_RDONLY);
    if (fd == -1) {
        perror("崩溃后重新打开文件失败");
        return EXIT_FAILURE;
    }
    
    memset(read_buffer, 0, BUFFER_SIZE);
    bytes_read = read(fd, read_buffer, BUFFER_SIZE - 1);
    close(fd);
    
    printf("崩溃后读取的数据: '%s' (长度: %zd)\n", read_buffer, bytes_read);
    printf("期望至少包含第一部分数据 'PART1'\n");
    
    // 在伪实现中，所有写入的数据都可能保存下来
    if (strstr(read_buffer, "PART1") == NULL) {
        printf("崩溃恢复测试失败: 无法找到应该已同步的数据 'PART1'\n");
        return EXIT_FAILURE;
    }
    
    printf("伪实现的 fsync 测试完成。\n");
    
    // 清理测试文件
    unlink(TEST_FILE);
    
    return EXIT_SUCCESS;
}