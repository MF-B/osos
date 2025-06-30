# 系统调用实现简述
## shm系列
+ shmget: 返回与参数键的值关联的 System V 共享内存段的标识符。
    1. 参数检查：
    * 检查size是否为负数，如果是，返回EINVAL。
    2. 标志解析：
    * 解析shmflg，提取IPC_CREAT、IPC_EXCL标志和权限位。
    3. IPC_PRIVATE处理：
    * 如果key是IPC_PRIVATE，直接创建新的共享内存段，并返回其shmid。
    4. 查找现有段：
    * 遍历现有共享内存段，查找是否已存在与key匹配且未被标记为删除的段。
        - 若找到现有段：
    <br>a. 检查IPC_CREAT | IPC_EXCL标志是否同时设置，如果是且段已存在，返回EEXIST。
    <br>b. 检查请求的size是否大于现有段的大小，如果是，返回EINVAL。
    <br>c. 检查权限（简化版，实际应从当前进程获取uid和gid）。
    <br>d. 返回现有段的shmid。
        - 若未找到现有段：
    <br>a. 检查是否设置了IPC_CREAT标志，如果没有，返回ENOENT。
    <br>b. 检查size是否为0，如果是，返回EINVAL。
    <br>c. 创建新的共享内存段，并返回其shmid。

+ shmat: 将 shmid 标识的共享内存段附加到调用进程的地址空间。
    1. 参数检查和日志记录：
    * 记录调试信息，包括shmid、shmaddr和shmflg。
    * 获取当前进程的地址空间（aspace）。
    2. 获取共享内存段：
    * 加锁共享内存管理器（SHM_MANAGER），获取指定shmid的共享内存段。
    * 如果未找到段，返回EINVAL。
    3. 权限检查：
    * 检查当前进程是否有权限访问该共享内存段（基于uid/gid和权限标志）。
    * 如果权限不足，返回EACCES。
    4. 确定附加地址：
    * 如果shmaddr为0，系统选择地址：
        - 从用户空间起始地址开始寻找合适的空闲区域。
        - 如果找不到足够的空间，返回ENOMEM。
    * 如果shmaddr不为0：
        - 如果设置了SHM_RND，对齐到SHMLBA边界（这里SHMLBA设为4096）。
        - 检查地址是否页面对齐，否则返回EINVAL。
    5. 设置映射权限：
    * 根据shmflg设置映射标志（用户空间、读、写、执行）。
    6. 分配物理内存：
    * 如果共享内存段尚未分配物理内存，调用aspace.alloc_shared分配。
    * 如果分配失败，返回ENOMEM。
    7. 映射共享内存段：
    * 调用aspace.map_linear将共享内存段映射到指定的虚拟地址。
    * 如果映射失败，返回ENOMEM。
    8. 更新连接信息：
    * 增加共享内存段的连接计数（attach_count）。
    * 更新附加时间（attach_time）。
    * 在共享内存管理器中记录连接信息（shmid、附加地址和进程ID）。

+ shmdt: 将位于 shmaddr 指定地址的共享内存段从调用进程的地址空间中分离出来。
    1. 参数检查：
    * 检查shmaddr是否为0，如果是，返回EINVAL。
    * 检查地址是否页面对齐（4KB），否则返回EINVAL。
    2. 查找共享内存段：
    * 获取当前进程的PID。
    * 加锁共享内存管理器（SHM_MANAGER），遍历attachments查找匹配的shmid和PID。
    * 如果未找到匹配项，返回EINVAL。
    3. 获取共享内存段信息：
    * 通过shmid获取共享内存段的可变引用。
    * 获取段的大小（segment_size）。
    4. 更新连接记录：
    * 从attachments中移除当前进程的连接记录。
    * 如果该shmid的连接记录为空，则从attachments中移除该shmid。
    5. 取消地址空间映射：
    * 加锁进程的地址空间（aspace），调用unmap取消对共享内存段的映射。
    * 如果unmap失败，返回EINVAL。
    6. 更新共享内存段状态：
    * 减少段的连接计数（attach_count）。
    * 更新分离时间（detach_time）。
    * 检查是否满足清理条件（marked_for_removal且attach_count == 0）。
    7. 清理共享内存段：
    如果满足清理条件，加锁管理器，从segments和attachments中移除该段。

+ shctl: 对shmid指定的共享内存区域执行一些控制操作。
    1. 参数检查和初始化
    * 记录调试信息。
    * 获取共享内存管理器（SHM_MANAGER）的锁。
    * 通过shmid获取共享内存段（segment_arc），如果未找到则返回EINVAL。
    * 获取当前进程的uid和gid（当前代码中硬编码为0，需要从当前进程获取真实值）。
    2. 根据cmd执行不同操作
    * IPC_STAT
        - 获取共享内存段的状态信息。
        - 检查当前进程是否有读权限。
        - 将shmid_ds结构体从共享内存段复制到用户空间缓冲区。
        - 返回成功。
    * IPC_SET
        - 设置共享内存段的属性。
        - 检查当前进程是否有修改权限（需要是所有者或root）。
        - 从用户空间缓冲区读取新的shmid_ds结构体。
        - 更新共享内存段的uid、gid、权限和时间戳。
        - 返回成功。
    * IPC_RMID
        - 标记共享内存段为删除。
        - 检查当前进程是否有删除权限（需要是所有者或root）。
        - 获取当前附加计数。
        - 标记段为删除并更新时间戳。
        - 如果没有进程附加，立即从管理器中移除段并释放相关资源。
        - 返回成功。
    * 其他命令
        - 返回EINVAL表示不支持的操作。

## I/O multiplexing系列
+ ppoll: 允许应用程序安全地等待，直到文件描述符准备就绪或捕获到信号。
    1. 参数验证
    * 检查nfds是否为0，如果是则直接返回0（无文件描述符需要轮询）。
    2. 获取用户空间数据
    * 使用get_as_mut_slice获取用户空间的Pollfd数组。
    * 将用户空间的Pollfd数组复制到本地Vec中，以便操作。
    3. 处理超时时间
    * 如果timeout为null，则设置timeout_ms为-1（无限等待）。
    * 否则，从用户空间读取timespec结构，转换为毫秒数。
    4. 处理信号屏蔽
    * 如果sigmask为null，则不修改信号屏蔽集。
    * 否则，从用户空间读取信号屏蔽集，并临时替换当前线程的信号屏蔽集，保存旧值以便恢复。
    5. 主要轮询逻辑
    * 调用poll_files函数执行实际的轮询操作，等待文件描述符变为就绪状态。
    * poll_files函数返回准备就绪的文件描述符数量。
    6. 恢复信号屏蔽
    * 如果之前修改了信号屏蔽集，则恢复原始信号屏蔽集。
    7. 写回结果
    * 将轮询结果（Pollfd数组）写回用户空间。

+ pselect: poll 的一个变体，用于在阻塞事件的同时增加对信号处理的支持。
    1. 参数验证
    * 检查nfds是否为负数，如果是则返回EINVAL错误。
    * 将nfds限制在FD_SETSIZE范围内，防止数组越界。
    2. 处理超时时间
    * 如果timeout不为null，则计算截止时间（当前时间加上timeval指定的时间）。
    3. 初始化文件描述符集合
    * 使用FdSets结构体初始化文件描述符集合。
    * 将readfds、writefds和exceptfds清零，确保初始状态为空。
    4. 主循环
    * 调用axnet::poll_interfaces()轮询网络接口，检查是否有文件描述符变为就绪状态。
    * 使用fd_sets.poll_all检查文件描述符集合，更新就绪状态。
    * 如果有文件描述符就绪，返回就绪数量。
    * 如果超时时间到达，返回0。
    * 否则，调用axtask::yield_now()让出CPU，避免忙等待。

## I/O 系列
+ pwrite64: 允许进程在文件的指定偏移量处写入数据，而不改变文件指针的位置。
    1. 参数验证
    * 检查offset是否为负数，如果是则返回EINVAL错误。
    2. 获取文件对象
    * 使用File::from_fd(fd)?从文件描述符获取文件对象。
    3. 写入数据
    * 调用file.get_inner().write_at(offset as u64, buf)?在指定偏移量处写入数据。
    * write_at方法将数据写入文件的指定位置，并返回实际写入的字节数。
    4. 返回结果
    * 将实际写入的字节数转换为isize并返回。
+ pread64: 允许进程从文件的指定偏移量处读取数据，并将数据写入用户空间的缓冲区，同时保持文件指针的位置不变。
    1. 参数验证
    * 检查offset是否为负数，如果是则返回EINVAL错误。
    2. 获取文件对象
    * 使用File::from_fd(fd)?从文件描述符获取文件对象。
    3. 读取数据
    * 调用file.get_inner().read_at(offset as u64, buf)?从指定偏移量处读取数据到用户空间的缓冲区。
    * read_at方法从文件的指定位置读取数据，并返回实际读取的字节数。
    4. 返回结果
    * 将实际读取的字节数转换为isize并返回。

+ ftruncate: 允许进程将文件的长度调整为指定的length，如果文件当前长度大于length，则截断文件；如果文件当前长度小于length，则扩展文件（可能填充零）。
    1. 参数验证
    * 检查length是否为负数，如果是则返回EINVAL错误。
    2. 获取文件对象
    * 使用File::from_fd(fd)?从文件描述符获取文件对象。
    3. 截断文件
    * 调用file.get_inner().truncate(length as _)将文件截断或扩展到指定长度。
    * truncate方法会修改文件的长度，可能涉及磁盘空间的分配或释放。
    4. 返回结果
    * 返回0表示操作成功。