#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <fcntl.h>
#include <string.h>
#include <errno.h>

int main(int argc, char *argv[]) {
    const char *filename = "fsync_test.txt";
    const char *test_data = "Testing fsync system call\n";
    int fd, ret;
    
    // 打开文件（如果不存在则创建）
    fd = open(filename, O_CREAT | O_RDWR | O_TRUNC, 0644);
    if (fd < 0) {
        perror("open failed");
        return EXIT_FAILURE;
    }
    
    printf("File '%s' opened successfully, fd = %d\n", filename, fd);
    
    // 写入数据到文件
    ret = write(fd, test_data, strlen(test_data));
    if (ret < 0) {
        perror("write failed");
        close(fd);
        return EXIT_FAILURE;
    }
    
    printf("Written %d bytes to file\n", ret);
    
    // 调用 fsync 确保数据写入磁盘
    printf("Calling fsync()...\n");
    ret = fsync(fd);
    if (ret < 0) {
        printf("fsync failed: %s (errno = %d)\n", strerror(errno), errno);
    } else {
        printf("fsync succeeded\n");
    }
    
    // 关闭文件
    close(fd);
    
    // 重新打开文件并验证内容
    fd = open(filename, O_RDONLY);
    if (fd < 0) {
        perror("Failed to reopen file");
        return EXIT_FAILURE;
    }
    
    char buffer[100];
    memset(buffer, 0, sizeof(buffer));
    
    ret = read(fd, buffer, sizeof(buffer) - 1);
    if (ret < 0) {
        perror("read failed");
        close(fd);
        return EXIT_FAILURE;
    }
    
    printf("Read %d bytes from file:\n%s\n", ret, buffer);
    
    // 验证读取的内容与写入的内容是否一致
    if (strcmp(buffer, test_data) == 0) {
        printf("Content verification: SUCCESS\n");
    } else {
        printf("Content verification: FAILED\n");
    }
    
    close(fd);
    
    printf("fsync test completed\n");
    return EXIT_SUCCESS;
}