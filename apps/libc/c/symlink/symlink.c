#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <sys/stat.h>
#include <errno.h>
#include <fcntl.h>

void test_basic_symlink()
{
    printf("=== 测试基本符号链接功能 ===\n");

    // 创建一个测试文件
    int fd = open("test_file.txt", O_CREAT | O_WRONLY, 0644);
    if (fd >= 0)
    {
        write(fd, "Hello World\n", 12);
        close(fd);
        printf("✓ 创建测试文件成功\n");
    }
    else
    {
        printf("✗ 创建测试文件失败: %s\n", strerror(errno));
        return;
    }

    // 创建符号链接
    if (symlink("test_file.txt", "test_symlink") == 0)
    {
        printf("✓ 创建符号链接成功\n");
    }
    else
    {
        printf("✗ 创建符号链接失败: %s\n", strerror(errno));
        return;
    }

    // 读取符号链接
    char buffer[256];
    ssize_t len = readlink("test_symlink", buffer, sizeof(buffer) - 1);
    if (len > 0)
    {
        buffer[len] = '\0';
        printf("✓ 读取符号链接成功: %s\n", buffer);

        if (strcmp(buffer, "test_file.txt") == 0)
        {
            printf("✓ 符号链接内容正确\n");
        }
        else
        {
            printf("✗ 符号链接内容错误，期望: test_file.txt, 实际: %s\n", buffer);
        }
    }
    else
    {
        printf("✗ 读取符号链接失败: %s\n", strerror(errno));
    }

    // 通过符号链接访问文件
    fd = open("test_symlink", O_RDONLY);
    if (fd >= 0)
    {
        char read_buffer[32];
        ssize_t read_len = read(fd, read_buffer, sizeof(read_buffer) - 1);
        if (read_len > 0)
        {
            read_buffer[read_len] = '\0';
            printf("✓ 通过符号链接读取文件成功: %s", read_buffer);
        }
        close(fd);
    }
    else
    {
        printf("✗ 通过符号链接打开文件失败: %s\n", strerror(errno));
    }

    // 清理
    unlink("test_symlink");
    unlink("test_file.txt");
    printf("\n");
}

void test_symlink_to_directory()
{
    printf("=== 测试指向目录的符号链接 ===\n");

    // 创建测试目录
    if (mkdir("test_dir", 0755) == 0)
    {
        printf("✓ 创建测试目录成功\n");
    }
    else
    {
        printf("✗ 创建测试目录失败: %s\n", strerror(errno));
        return;
    }

    // 创建指向目录的符号链接
    if (symlink("test_dir", "test_dir_symlink") == 0)
    {
        printf("✓ 创建指向目录的符号链接成功\n");
    }
    else
    {
        printf("✗ 创建指向目录的符号链接失败: %s\n", strerror(errno));
        rmdir("test_dir");
        return;
    }

    // 验证符号链接
    struct stat st;
    if (lstat("test_dir_symlink", &st) == 0 && S_ISLNK(st.st_mode))
    {
        printf("✓ 符号链接类型正确\n");
    }
    else
    {
        printf("✗ 符号链接类型错误\n");
    }

    // 通过符号链接访问目录
    if (stat("test_dir_symlink", &st) == 0 && S_ISDIR(st.st_mode))
    {
        printf("✓ 通过符号链接访问目录成功\n");
    }
    else
    {
        printf("✗ 通过符号链接访问目录失败\n");
    }

    // 清理
    unlink("test_dir_symlink");
    rmdir("test_dir");
    printf("\n");
}

void test_broken_symlink()
{
    printf("=== 测试断开的符号链接 ===\n");

    // 创建指向不存在文件的符号链接
    if (symlink("nonexistent_file", "broken_symlink") == 0)
    {
        printf("✓ 创建断开的符号链接成功\n");
    }
    else
    {
        printf("✗ 创建断开的符号链接失败: %s\n", strerror(errno));
        return;
    }

    // 使用lstat应该成功（不跟随链接）
    struct stat st;
    if (lstat("broken_symlink", &st) == 0 && S_ISLNK(st.st_mode))
    {
        printf("✓ lstat断开的符号链接成功\n");
    }
    else
    {
        printf("✗ lstat断开的符号链接失败\n");
    }

    // 使用stat应该失败（跟随链接）
    if (stat("broken_symlink", &st) == -1 && errno == ENOENT)
    {
        printf("✓ stat断开的符号链接正确失败\n");
    }
    else
    {
        printf("✗ stat断开的符号链接应该失败但没有失败\n");
    }

    // 尝试打开应该失败
    int fd = open("broken_symlink", O_RDONLY);
    if (fd == -1 && errno == ENOENT)
    {
        printf("✓ 打开断开的符号链接正确失败\n");
    }
    else
    {
        printf("✗ 打开断开的符号链接应该失败但没有失败\n");
        if (fd >= 0)
            close(fd);
    }

    // 清理
    unlink("broken_symlink");
    printf("\n");
}

void test_symlink_chain()
{
    printf("=== 测试符号链接链 ===\n");

    // 创建原始文件
    int fd = open("original.txt", O_CREAT | O_WRONLY, 0644);
    if (fd >= 0)
    {
        write(fd, "original content\n", 17);
        close(fd);
    }
    else
    {
        printf("✗ 创建原始文件失败: %s\n", strerror(errno));
        return;
    }

    // 创建符号链接链：link1 -> link2 -> original.txt
    if (symlink("original.txt", "link2") == 0)
    {
        printf("✓ 创建link2成功\n");
    }
    else
    {
        printf("✗ 创建link2失败: %s\n", strerror(errno));
        unlink("original.txt");
        return;
    }

    if (symlink("link2", "link1") == 0)
    {
        printf("✓ 创建link1成功\n");
    }
    else
    {
        printf("✗ 创建link1失败: %s\n", strerror(errno));
        unlink("link2");
        unlink("original.txt");
        return;
    }

    // 通过链式符号链接访问文件
    fd = open("link1", O_RDONLY);
    if (fd >= 0)
    {
        char buffer[32];
        ssize_t len = read(fd, buffer, sizeof(buffer) - 1);
        if (len > 0)
        {
            buffer[len] = '\0';
            printf("✓ 通过符号链接链读取文件成功: %s", buffer);
        }
        close(fd);
    }
    else
    {
        printf("✗ 通过符号链接链访问文件失败: %s\n", strerror(errno));
    }

    // 清理
    unlink("link1");
    unlink("link2");
    unlink("original.txt");
    printf("\n");
}

void test_error_conditions()
{
    printf("=== 测试错误条件 ===\n");

    // 测试创建已存在的符号链接
    int fd = open("existing_file", O_CREAT | O_WRONLY, 0644);
    if (fd >= 0)
    {
        close(fd);

        if (symlink("target", "existing_file") == -1 && errno == EEXIST)
        {
            printf("✓ 创建已存在文件的符号链接正确失败\n");
        }
        else
        {
            printf("✗ 创建已存在文件的符号链接应该失败\n");
        }

        unlink("existing_file");
    }

    // 测试读取不存在的符号链接
    char buffer[256];
    if (readlink("nonexistent_symlink", buffer, sizeof(buffer)) == -1 && errno == ENOENT)
    {
        printf("✓ 读取不存在的符号链接正确失败\n");
    }
    else
    {
        printf("✗ 读取不存在的符号链接应该失败\n");
    }

    // 测试读取普通文件（非符号链接）
    fd = open("regular_file", O_CREAT | O_WRONLY, 0644);
    if (fd >= 0)
    {
        close(fd);

        if (readlink("regular_file", buffer, sizeof(buffer)) == -1 && errno == EINVAL)
        {
            printf("✓ 读取普通文件作为符号链接正确失败\n");
        }
        else
        {
            printf("✗ 读取普通文件作为符号链接应该失败\n");
        }

        unlink("regular_file");
    }

    printf("\n");
}

void test_relative_absolute_paths()
{
    printf("=== 测试相对和绝对路径 ===\n");

    // 创建测试文件
    int fd = open("target_file", O_CREAT | O_WRONLY, 0644);
    if (fd >= 0)
    {
        write(fd, "test content\n", 13);
        close(fd);
    }

    // 测试相对路径符号链接
    if (symlink("target_file", "relative_link") == 0)
    {
        printf("✓ 创建相对路径符号链接成功\n");

        char buffer[256];
        ssize_t len = readlink("relative_link", buffer, sizeof(buffer) - 1);
        if (len > 0)
        {
            buffer[len] = '\0';
            printf("✓ 相对路径符号链接内容: %s\n", buffer);
        }
    }

    // 测试绝对路径符号链接
    char cwd[256];
    if (getcwd(cwd, sizeof(cwd)) != NULL)
    {
        char abs_path[512];
        snprintf(abs_path, sizeof(abs_path), "%s/target_file", cwd);

        if (symlink(abs_path, "absolute_link") == 0)
        {
            printf("✓ 创建绝对路径符号链接成功\n");

            char buffer[256];
            ssize_t len = readlink("absolute_link", buffer, sizeof(buffer) - 1);
            if (len > 0)
            {
                buffer[len] = '\0';
                printf("✓ 绝对路径符号链接内容: %s\n", buffer);
            }
        }
    }

    // 清理
    unlink("relative_link");
    unlink("absolute_link");
    unlink("target_file");
    printf("\n");
}

int main()
{
    printf("开始测试符号链接功能...\n\n");

    test_basic_symlink();
    test_symlink_to_directory();
    test_broken_symlink();
    test_symlink_chain();
    test_error_conditions();
    test_relative_absolute_paths();

    printf("符号链接功能测试完成！\n");
    return 0;
}