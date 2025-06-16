use core::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};

use alloc::{collections::btree_map::BTreeMap, sync::Arc, vec::Vec};
use axerrno::LinuxResult;
use axprocess::Pid;
use axtask::TaskExtRef;
use memory_addr::{PhysAddr, VirtAddr};
use spin::RwLock;

/// 全局共享内存段表，按shmid索引
static SHM_TABLE: RwLock<BTreeMap<i32, Arc<SharedMemorySegment>>> = RwLock::new(BTreeMap::new());
/// 全局共享内存键值表，用于key到shmid的映射
static SHM_KEY_TABLE: RwLock<BTreeMap<i32, i32>> = RwLock::new(BTreeMap::new()); // key -> shmid映射  
/// 下一个可用的共享内存段ID
static NEXT_SHMID: AtomicI32 = AtomicI32::new(1);

/// 共享内存段结构体
///
/// 表示一个共享内存段的所有信息，包括标识符、大小、权限、物理页面等。
/// 多个进程可以通过相同的key或shmid访问同一个共享内存段。
pub struct SharedMemorySegment {
    /// 共享内存段ID  
    pub shmid: i32,
    /// 共享内存键值  
    pub key: i32,
    /// 内存段大小  
    pub size: usize,
    /// 物理内存页面  
    pub pages: Vec<PhysAddr>,
    /// 权限标志  
    pub perm: u16,
    /// 创建者进程ID  
    pub creator_pid: Pid,
    /// 引用计数  
    pub attach_count: AtomicUsize,
    /// 创建时间  
    pub ctime: u64,
    /// 最后访问时间  
    pub atime: AtomicU64,
    /// 最后修改时间  
    pub dtime: AtomicU64,
}

// 移除 Drop trait 实现，改为显式清理
impl SharedMemorySegment {
    /// 显式清理共享内存段
    pub fn cleanup(&self) {
        // 释放物理内存页面
        for _page in &self.pages {
            // TODO: 调用内存管理器释放页面
        }
    }
}

/// 共享内存映射信息
///
/// 表示进程地址空间中的一个共享内存映射，包含虚拟地址、大小等信息。
pub struct ShmMapping {
    /// 共享内存段ID
    pub shmid: i32,
    /// 映射的虚拟地址
    pub vaddr: VirtAddr,
    /// 映射大小
    pub size: usize,
    /// 映射标志
    pub flags: u32,
}

/// 根据共享内存段ID获取共享内存段
///
/// # 参数
///
/// * `shmid` - 共享内存段ID
///
/// # 返回值
///
/// 如果找到对应的共享内存段，返回`Some(Arc<SharedMemorySegment>)`，否则返回`None`
pub fn get_shm_by_id(shmid: i32) -> Option<Arc<SharedMemorySegment>> {
    SHM_TABLE.read().get(&shmid).cloned()
}

/// 根据键值获取共享内存段
///
/// # 参数
///
/// * `key` - 共享内存键值
///
/// # 返回值
///
/// 如果找到对应的共享内存段，返回`Some(Arc<SharedMemorySegment>)`，否则返回`None`
pub fn get_shm_by_key(key: i32) -> Option<Arc<SharedMemorySegment>> {
    let key_table = SHM_KEY_TABLE.read();
    let shmid = key_table.get(&key)?;
    SHM_TABLE.read().get(shmid).cloned()
}

/// 系统调用：获取共享内存段
///
/// 该函数实现了POSIX `shmget`系统调用，用于获取或创建共享内存段。
///
/// # 参数
///
/// * `key` - 共享内存键值，如果为-1则表示IPC_PRIVATE
/// * `size` - 请求的内存段大小（字节）
/// * `shmflg` - 标志位，包含权限和创建标志
///   - 权限位：低9位表示文件权限（如0o644）
///   - IPC_CREAT (0o1000)：如果不存在则创建
///   - IPC_EXCL (0o2000)：与IPC_CREAT一起使用，如果已存在则失败
///
/// # 返回值
///
/// 成功时返回共享内存段ID，失败时返回相应的Linux错误码：
/// * `EINVAL` - 参数无效（size为0或过大）
/// * `EEXIST` - 设置了IPC_CREAT|IPC_EXCL但共享内存段已存在
/// * `ENOENT` - 未设置IPC_CREAT且共享内存段不存在
/// * `EACCES` - 权限不足
///
/// # 示例
///
/// ```rust
/// // 创建新的共享内存段
/// let shmid = sys_shmget(-1, 4096, 0o644 | 0o1000)?;
///
/// // 获取已存在的共享内存段
/// let shmid = sys_shmget(1234, 0, 0o644)?;
/// ```
pub fn sys_shmget(key: i32, size: usize, shmflg: i32) -> LinuxResult<isize> {
    use axerrno::LinuxError;
    use axtask::current;

    // 1. 验证参数
    if size == 0 {
        return Err(LinuxError::EINVAL);
    }

    // 2. 检查key是否已存在
    if key != -1 {
        // IPC_PRIVATE = -1
        if let Some(existing_segment) = get_shm_by_key(key) {
            // 如果设置了IPC_CREAT | IPC_EXCL，且段已存在，返回错误
            if (shmflg & 0o2000) != 0 && (shmflg & 0o1000) != 0 {
                // IPC_CREAT | IPC_EXCL
                return Err(LinuxError::EEXIST);
            }

            // 检查大小
            if size > existing_segment.size {
                return Err(LinuxError::EINVAL);
            }

            return Ok(existing_segment.shmid as isize);
        } else if (shmflg & 0o1000) == 0 {
            // 没有IPC_CREAT标志
            return Err(LinuxError::ENOENT);
        }
    }

    // 3. 创建新的共享内存段
    let curr = current();
    let creator_pid = curr.task_ext().thread.process().pid();
    let aspace = curr.task_ext().process_data().aspace.lock();
    let perm = (shmflg & 0o777) as u16;

    // 分配物理页面
    let page_count = (size + 4095) / 4096;
    let mut pages = Vec::new();

    for _ in 0..page_count { 
        // TODO: 分配物理页面
        // aspace.map_shared(start, size, name, flags, align)
    }

    // 先分配 shmid
    let shmid = NEXT_SHMID.fetch_add(1, Ordering::SeqCst);

    let segment = Arc::new(SharedMemorySegment {
        shmid, // 直接使用分配的 shmid
        key,
        size,
        pages,
        perm,
        creator_pid,
        attach_count: AtomicUsize::new(0),
        ctime: axhal::time::monotonic_time_nanos() / 1_000_000_000,
        atime: AtomicU64::new(0),
        dtime: AtomicU64::new(0),
    });

    // 4. 添加到全局表
    {
        let mut shm_table = SHM_TABLE.write();
        let mut key_table = SHM_KEY_TABLE.write();
        shm_table.insert(shmid, segment.clone());
        if key != -1 {
            key_table.insert(key, shmid);
        }
    }
    Ok(shmid as isize)
}
