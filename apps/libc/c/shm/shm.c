#include <sys/ipc.h>
#include <sys/shm.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>
#include <errno.h>

#define SHM_SIZE 1024
#define TEST_KEY 1234

void test_shmget_shmat() {
    printf("=== Testing shmget and shmat ===\n");
    
    // 测试 shmget - 创建共享内存段
    int shmid = shmget(TEST_KEY, SHM_SIZE, IPC_CREAT | 0666);
    if (shmid == -1) {
        perror("shmget failed");
        return;
    }
    printf("✓ shmget success: shmid = %d\n", shmid);
    
    // 测试 shmat - 连接共享内存
    void *shm_ptr = shmat(shmid, NULL, 0);
    if (shm_ptr == (void *)-1) {
        perror("shmat failed");
        shmctl(shmid, IPC_RMID, NULL);
        return;
    }
    printf("✓ shmat success: addr = %p\n", shm_ptr);
    
    // 测试写入数据
    const char *test_data = "Hello, shared memory!";
    strcpy((char *)shm_ptr, test_data);
    printf("✓ Write to shared memory: %s\n", test_data);
    
    // 测试读取数据
    char *read_data = (char *)shm_ptr;
    printf("✓ Read from shared memory: %s\n", read_data);
    
    // 验证数据一致性
    if (strcmp(test_data, read_data) == 0) {
        printf("✓ Data consistency check passed\n");
    } else {
        printf("✗ Data consistency check failed\n");
    }
    
    // 测试 shmdt - 分离共享内存
    if (shmdt(shm_ptr) == -1) {
        perror("shmdt failed");
    } else {
        printf("✓ shmdt success\n");
    }
    
    // 清理：删除共享内存段
    if (shmctl(shmid, IPC_RMID, NULL) == -1) {
        perror("shmctl IPC_RMID failed");
    } else {
        printf("✓ Shared memory segment removed\n");
    }
}

void test_multiprocess_shm() {
    printf("\n=== Testing multi-process shared memory ===\n");
    
    int shmid = shmget(TEST_KEY + 1, SHM_SIZE, IPC_CREAT | 0666);
    if (shmid == -1) {
        perror("shmget failed");
        return;
    }
    
    pid_t pid = fork();
    if (pid == -1) {
        perror("fork failed");
        shmctl(shmid, IPC_RMID, NULL);
        return;
    }
    
    if (pid == 0) {
        // 子进程
        void *shm_ptr = shmat(shmid, NULL, 0);
        if (shm_ptr == (void *)-1) {
            perror("child: shmat failed");
            exit(1);
        }
        
        // 等待父进程写入数据
        sleep(1);
        
        // 读取父进程写入的数据
        char *data = (char *)shm_ptr;
        printf("Child process read: %s\n", data);
        
        // 子进程写入响应
        strcat(data, " - Response from child");
        
        shmdt(shm_ptr);
        exit(0);
    } else {
        // 父进程
        void *shm_ptr = shmat(shmid, NULL, 0);
        if (shm_ptr == (void *)-1) {
            perror("parent: shmat failed");
            shmctl(shmid, IPC_RMID, NULL);
            return;
        }
        
        // 父进程写入数据
        const char *parent_msg = "Message from parent";
        strcpy((char *)shm_ptr, parent_msg);
        printf("Parent process wrote: %s\n", parent_msg);
        
        // 等待子进程完成
        wait(NULL);
        
        // 读取子进程的响应
        char *final_data = (char *)shm_ptr;
        printf("Final data: %s\n", final_data);
        
        shmdt(shm_ptr);
        shmctl(shmid, IPC_RMID, NULL);
        printf("✓ Multi-process test completed\n");
    }
}

void test_error_conditions() {
    printf("\n=== Testing error conditions ===\n");
    
    // 测试无效的大小
    int shmid = shmget(TEST_KEY + 2, -1, IPC_CREAT | 0666);
    if (shmid == -1) {
        printf("✓ shmget correctly failed with invalid size\n");
    } else {
        printf("✗ shmget should have failed with invalid size\n");
        shmctl(shmid, IPC_RMID, NULL);
    }
    
    // 测试连接不存在的共享内存段
    void *ptr = shmat(99999, NULL, 0);
    if (ptr == (void *)-1) {
        printf("✓ shmat correctly failed with invalid shmid\n");
    } else {
        printf("✗ shmat should have failed with invalid shmid\n");
        shmdt(ptr);
    }
    
    // 测试分离未连接的共享内存
    if (shmdt((void *)0x12345678) == -1) {
        printf("✓ shmdt correctly failed with invalid address\n");
    } else {
        printf("✗ shmdt should have failed with invalid address\n");
    }
}

void test_shm_info() {
    printf("\n=== Testing shmctl IPC_STAT ===\n");
    
    int shmid = shmget(TEST_KEY + 3, SHM_SIZE, IPC_CREAT | 0666);
    if (shmid == -1) {
        perror("shmget failed");
        return;
    }
    
    struct shmid_ds shm_info;
    if (shmctl(shmid, IPC_STAT, &shm_info) == -1) {
        perror("shmctl IPC_STAT failed");
    } else {
        printf("✓ Shared memory info retrieved:\n");
        printf("  Size: %zu bytes\n", shm_info.shm_segsz);
        printf("  Attach count: %d\n", (int)shm_info.shm_nattch);
        printf("  Creator PID: %d\n", (int)shm_info.shm_cpid);
    }
    
    shmctl(shmid, IPC_RMID, NULL);
}

void test_shmdt_detailed() {
    printf("\n=== Detailed shmdt Testing ===\n");
    
    int shmid = shmget(TEST_KEY + 4, SHM_SIZE, IPC_CREAT | 0666);
    if (shmid == -1) {
        perror("shmget failed");
        return;
    }
    
    // 测试多次连接和分离
    void *ptr1 = shmat(shmid, NULL, 0);
    void *ptr2 = shmat(shmid, NULL, 0);
    void *ptr3 = shmat(shmid, NULL, 0);
    
    if (ptr1 == (void *)-1 || ptr2 == (void *)-1 || ptr3 == (void *)-1) {
        perror("shmat failed");
        shmctl(shmid, IPC_RMID, NULL);
        return;
    }
    
    printf("✓ Multiple attach successful: %p, %p, %p\n", ptr1, ptr2, ptr3);
    
    // 检查连接计数
    struct shmid_ds info;
    shmctl(shmid, IPC_STAT, &info);
    printf("✓ Attach count after 3 attaches: %d\n", (int)info.shm_nattch);
    
    // 逐一分离
    if (shmdt(ptr1) == 0) {
        printf("✓ First shmdt successful\n");
        shmctl(shmid, IPC_STAT, &info);
        printf("  Attach count: %d\n", (int)info.shm_nattch);
    } else {
        perror("First shmdt failed");
    }
    
    if (shmdt(ptr2) == 0) {
        printf("✓ Second shmdt successful\n");
        shmctl(shmid, IPC_STAT, &info);
        printf("  Attach count: %d\n", (int)info.shm_nattch);
    } else {
        perror("Second shmdt failed");
    }
    
    if (shmdt(ptr3) == 0) {
        printf("✓ Third shmdt successful\n");
        shmctl(shmid, IPC_STAT, &info);
        printf("  Attach count: %d\n", (int)info.shm_nattch);
    } else {
        perror("Third shmdt failed");
    }
    
    // 测试重复分离同一地址
    if (shmdt(ptr1) == -1) {
        printf("✓ shmdt correctly failed on already detached address (errno: %s)\n", strerror(errno));
    } else {
        printf("✗ shmdt should fail on already detached address\n");
    }
    
    // 测试分离无效地址
    if (shmdt((void *)0x1000) == -1) {
        printf("✓ shmdt correctly failed with invalid address (errno: %s)\n", strerror(errno));
    } else {
        printf("✗ shmdt should fail with invalid address\n");
    }
    
    shmctl(shmid, IPC_RMID, NULL);
}

void test_shmctl_detailed() {
    printf("\n=== Detailed shmctl Testing ===\n");
    
    int shmid = shmget(TEST_KEY + 5, SHM_SIZE, IPC_CREAT | 0666);
    if (shmid == -1) {
        perror("shmget failed");
        return;
    }
    
    // 测试 IPC_STAT
    struct shmid_ds shm_stat;
    if (shmctl(shmid, IPC_STAT, &shm_stat) == 0) {
        printf("✓ IPC_STAT successful:\n");
        printf("  Segment size: %zu bytes\n", shm_stat.shm_segsz);
        printf("  Attach count: %d\n", (int)shm_stat.shm_nattch);
        printf("  Creator PID: %d\n", (int)shm_stat.shm_cpid);
        printf("  Last attach PID: %d\n", (int)shm_stat.shm_lpid);
        printf("  Permissions: %o\n", shm_stat.shm_perm.mode);
    } else {
        perror("IPC_STAT failed");
    }
    
    // 连接共享内存以改变状态
    void *ptr = shmat(shmid, NULL, 0);
    if (ptr != (void *)-1) {
        // 再次检查状态
        if (shmctl(shmid, IPC_STAT, &shm_stat) == 0) {
            printf("✓ IPC_STAT after attach:\n");
            printf("  Attach count: %d\n", (int)shm_stat.shm_nattch);
            printf("  Last attach PID: %d\n", (int)shm_stat.shm_lpid);
        }
        
        shmdt(ptr);
    }
    
    // 测试 IPC_SET (修改权限)
    struct shmid_ds new_stat = shm_stat;
    new_stat.shm_perm.mode = 0644;
    
    if (shmctl(shmid, IPC_SET, &new_stat) == 0) {
        printf("✓ IPC_SET successful - permissions changed\n");
        
        // 验证修改
        if (shmctl(shmid, IPC_STAT, &shm_stat) == 0) {
            printf("  New permissions: %o\n", shm_stat.shm_perm.mode & 0777);
        }
    } else {
        perror("IPC_SET failed");
    }
    
    // 测试无效的 shmid
    if (shmctl(99999, IPC_STAT, &shm_stat) == -1) {
        printf("✓ shmctl correctly failed with invalid shmid (errno: %s)\n", strerror(errno));
    } else {
        printf("✗ shmctl should fail with invalid shmid\n");
    }
    
    // 测试无效的命令
    if (shmctl(shmid, 999, &shm_stat) == -1) {
        printf("✓ shmctl correctly failed with invalid command (errno: %s)\n", strerror(errno));
    } else {
        printf("✗ shmctl should fail with invalid command\n");
    }
    
    // 测试 IPC_RMID
    if (shmctl(shmid, IPC_RMID, NULL) == 0) {
        printf("✓ IPC_RMID successful - segment marked for deletion\n");
        
        // 尝试再次访问已删除的段
        if (shmctl(shmid, IPC_STAT, &shm_stat) == -1) {
            printf("✓ Access to removed segment correctly failed (errno: %s)\n", strerror(errno));
        } else {
            printf("✗ Access to removed segment should fail\n");
        }
    } else {
        perror("IPC_RMID failed");
    }
}

void test_shmctl_with_attachments() {
    printf("\n=== Testing shmctl with active attachments ===\n");
    
    int shmid = shmget(TEST_KEY + 6, SHM_SIZE, IPC_CREAT | 0666);
    if (shmid == -1) {
        perror("shmget failed");
        return;
    }
    
    // 连接共享内存
    void *ptr = shmat(shmid, NULL, 0);
    if (ptr == (void *)-1) {
        perror("shmat failed");
        shmctl(shmid, IPC_RMID, NULL);
        return;
    }
    
    // 写入测试数据
    strcpy((char *)ptr, "Test data before removal");
    
    // 在有连接的情况下删除段
    if (shmctl(shmid, IPC_RMID, NULL) == 0) {
        printf("✓ IPC_RMID successful with active attachment\n");
        
        // 段应该仍然可访问，直到最后一个进程分离
        printf("✓ Data still accessible: %s\n", (char *)ptr);
        
        // 修改数据
        strcpy((char *)ptr, "Modified after IPC_RMID");
        printf("✓ Can still modify data: %s\n", (char *)ptr);
        
        // 分离后段应该被真正删除
        if (shmdt(ptr) == 0) {
            printf("✓ shmdt successful after IPC_RMID\n");
        } else {
            perror("shmdt failed");
        }
        
        // 现在段应该不存在了
        struct shmid_ds info;
        if (shmctl(shmid, IPC_STAT, &info) == -1) {
            printf("✓ Segment truly removed after last detach (errno: %s)\n", strerror(errno));
        } else {
            printf("✗ Segment should be removed after last detach\n");
        }
    } else {
        perror("IPC_RMID failed");
        shmdt(ptr);
    }
}

int main() {
    printf("Starting comprehensive shared memory system call tests...\n\n");
    
    test_shmget_shmat();
    test_multiprocess_shm();
    test_error_conditions();
    test_shm_info();
    test_shmdt_detailed();
    test_shmctl_detailed();
    test_shmctl_with_attachments();
    
    printf("\n=== All tests completed ===\n");
    return 0;
}