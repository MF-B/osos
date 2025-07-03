use crate::ptr::UserPtr;
use alloc::{collections::BTreeMap, sync::Arc};
use axerrno::{LinuxError, LinuxResult};
use axhal::{
    mem::{PhysAddr, VirtAddr},
    paging::{MappingFlags, PageSize},
};
use axtask::TaskExtRef;
use axtask::current;
use memory_addr::{MemoryAddr, VirtAddrRange};
use spin::{Mutex, RwLock};

/// IPC flags
pub const IPC_CREAT: i32 = 0o1000;
pub const IPC_EXCL: i32 = 0o2000;
pub const IPC_NOWAIT: i32 = 0o4000;

/// IPC commands
pub const IPC_RMID: i32 = 0;
pub const IPC_SET: i32 = 1;
pub const IPC_STAT: i32 = 2;

/// SHM operations and flags
pub const SHM_RDONLY: i32 = 0o010000;
pub const SHM_RND: i32 = 0o020000;
pub const SHM_REMAP: i32 = 0o040000;
pub const SHM_EXEC: i32 = 0o100000;

/// Shared memory segment identifier
pub type ShmId = i32;

/// Shared memory key type
pub type Key = i32;

/// Shared memory segment data structure (shmid_ds)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ShmidDs {
    pub shm_perm: IpcPerm, // IPC permissions
    pub shm_segsz: usize,  // Size of segment in bytes
    pub shm_atime: i64,    // Last attach time
    pub shm_dtime: i64,    // Last detach time
    pub shm_ctime: i64,    // Last change time
    pub shm_cpid: i32,     // Creator PID
    pub shm_lpid: i32,     // Last operator PID
    pub shm_nattch: u64,   // Number of current attaches
    pub shm_unused: [u32; 4], // Unused fields for future expansion
}

/// IPC permission structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IpcPerm {
    pub key: i32,  // Key supplied to shmget()
    pub uid: u32,  // Effective UID of owner
    pub gid: u32,  // Effective GID of owner
    pub cuid: u32, // Effective UID of creator
    pub cgid: u32, // Effective GID of creator
    pub mode: u32, // Permissions
    pub seq: u32,  // Sequence number
    pub _unused1: [u32;5], // Unused
}

/// System call: shmget - get shared memory segment
///
/// # Arguments
/// * `key` - Shared memory key
/// * `size` - Size of the shared memory segment in bytes
/// * `shmflg` - Flags (IPC_CREAT, IPC_EXCL, permissions)
///
/// # Returns
/// * `Ok(shmid)` - Shared memory identifier on success
/// * `Err(LinuxError)` - Error code on failure
pub fn sys_shmget(key: Key, size: isize, shmflg: i32) -> LinuxResult<isize> {
    debug!(
        "sys_shmget: key={}, size={}, shmflg={:#x}",
        key, size, shmflg
    );

    // 检查大小参数的有效性
    if size < 0 {
        return Err(LinuxError::EINVAL);
    }

    let size = size as usize;

    const IPC_PRIVATE: Key = 0;
    let create_flag = (shmflg & IPC_CREAT) != 0;
    let excl_flag = (shmflg & IPC_EXCL) != 0;
    let permissions = (shmflg & 0o777) as u32;

    let mut manager = SHM_MANAGER.lock();

    // 如果key是IPC_PRIVATE，总是创建新的段
    if key == IPC_PRIVATE {
        if size <= 0 {
            return Err(LinuxError::EINVAL);
        }

        let shmid = manager.create_segment(key, size, permissions)?;
        return Ok(shmid as isize);
    }

    // 查找是否已存在相同key的段
    let existing_segment = manager
        .segments
        .iter()
        .find(|(_, segment)| {
            let seg = segment.read();
            seg.key == key && !seg.marked_for_removal
        })
        .map(|(shmid, segment)| (*shmid, segment.clone()));

    match existing_segment {
        Some((shmid, segment_arc)) => {
            // 找到了现有的段
            if excl_flag && create_flag {
                // IPC_CREAT | IPC_EXCL 但段已存在
                return Err(LinuxError::EEXIST);
            }

            // 检查大小要求
            let segment = segment_arc.read();
            if size > 0 && size > segment.size {
                return Err(LinuxError::EINVAL);
            }

            // 检查权限 - 简化版本，实际应该检查访问权限
            let uid = 0; // TODO: 从当前进程获取真实的uid
            let gid = 0; // TODO: 从当前进程获取真实的gid

            if !segment.check_permission(uid, gid, false) {
                return Err(LinuxError::EACCES);
            }

            debug!(
                "Found existing shared memory segment: id={}, key={}",
                shmid, key
            );
            Ok(shmid as isize)
        }
        None => {
            // 没有找到现有的段
            if !create_flag {
                // 没有IPC_CREAT标志，不能创建新段
                return Err(LinuxError::ENOENT);
            }

            if size == 0 {
                return Err(LinuxError::EINVAL);
            }

            // 创建新的共享内存段
            let shmid = manager.create_segment(key, size, permissions)?;
            Ok(shmid as isize)
        }
    }
}

/// System call: shmat - attach shared memory segment
///
/// # Arguments
/// * `shmid` - Shared memory identifier
/// * `shmaddr` - Desired attach address (0 for system choice)
/// * `shmflg` - Flags (SHM_RDONLY, SHM_RND, SHM_REMAP)
///
/// # Returns
/// * `Ok(addr)` - Virtual address where segment is attached
/// * `Err(LinuxError)` - Error code on failure
pub fn sys_shmat(shmid: ShmId, shmaddr: usize, shmflg: i32) -> LinuxResult<isize> {
    debug!(
        "sys_shmat: shmid={}, shmaddr={:#x}, shmflg={:#x}",
        shmid, shmaddr, shmflg
    );

    let current_task = current();
    let mut aspace = current_task.task_ext().process_data().aspace.lock();

    // 获取共享内存段
    let manager = SHM_MANAGER.lock();
    let segment_arc = manager.get_segment(shmid).ok_or(LinuxError::EINVAL)?;

    let segment = segment_arc.read();

    // 检查权限
    let want_write = (shmflg & SHM_RDONLY) == 0;
    let uid = 0; // TODO: 从当前进程获取真实的uid/gid
    let gid = 0;

    if !segment.check_permission(uid, gid, want_write) {
        return Err(LinuxError::EACCES);
    }

    let aligned_length = memory_addr::align_up_4k(segment.size);

    let attach_addr = if shmaddr == 0 {
        // 系统选择地址 - 在用户空间中寻找合适的地址
        let hint_addr = aspace.base(); // 用户空间起始地址
        let limit = VirtAddrRange::from_start_size(aspace.base(), aspace.size());
        let align = PageSize::Size4K;

        match aspace.find_free_area(hint_addr, aligned_length, limit, align) {
            Some(addr) => addr,
            _ => return Err(LinuxError::ENOMEM),
        }
    } else {
        let mut addr = VirtAddr::from(shmaddr);

        // 如果设置了 SHM_RND，需要对齐到 SHMLBA 边界
        if (shmflg & SHM_RND) != 0 {
            const SHMLBA: usize = 4096; // 页面大小对齐
            addr = VirtAddr::from(addr.as_usize() & !(SHMLBA - 1));
        }

        // 检查地址是否页面对齐
        if !addr.is_aligned(PageSize::Size4K) {
            return Err(LinuxError::EINVAL);
        }

        addr
    };

    // 设置映射权限
    let mut mapping_flags = MappingFlags::USER | MappingFlags::READ;
    if want_write {
        mapping_flags |= MappingFlags::WRITE;
    }
    if !(shmflg & SHM_EXEC == 0) {
        mapping_flags |= MappingFlags::EXECUTE;
    }

    drop(segment); // 释放读锁
    drop(manager); // 释放管理器锁

    let phys_addr = {
        let mut segment = segment_arc.write();
        if segment.phys_addr.as_usize() == 0 {
            // 第一次映射，分配物理内存
            if let Ok(pa) = aspace.alloc_shared(aligned_length, PageSize::Size4K) {
                segment.phys_addr = pa;
            } else {
                return Err(LinuxError::ENOMEM);
            }
        }
        segment.phys_addr
    };

    let result = aspace.map_linear(
        attach_addr,
        phys_addr,
        aligned_length,
        mapping_flags,
        PageSize::Size4K,
    );

    if result.is_err() {
        return Err(LinuxError::ENOMEM);
    }

    // 更新段的连接信息
    {
        let mut segment = segment_arc.write();
        segment.attach_count += 1;
        segment.attach_time = axhal::time::wall_time().as_secs();
    }

    // 记录连接信息到管理器
    {
        let mut manager = SHM_MANAGER.lock();
        manager
            .attachments
            .entry(shmid)
            .or_insert_with(BTreeMap::new)
            .insert(attach_addr, current_task.id().as_u64() as u32);
    }

    debug!(
        "Successfully attached shared memory segment {} at address {:#x}",
        shmid,
        attach_addr.as_usize()
    );

    Ok(attach_addr.as_usize() as isize)
}

/// System call: shmctl - shared memory control operations
///
/// # Arguments
/// * `shmid` - Shared memory identifier
/// * `cmd` - Control command (IPC_STAT, IPC_SET, IPC_RMID)
/// * `buf` - Buffer for shmid_ds structure
///
/// # Returns
/// * `Ok(0)` - Success
/// * `Err(LinuxError)` - Error code on failure
pub fn sys_shmctl(shmid: ShmId, cmd: i32, buf: UserPtr<ShmidDs>) -> LinuxResult<isize> {
    debug!("sys_shmctl: shmid={}, cmd={}", shmid, cmd);

    let manager = SHM_MANAGER.lock();
    let segment_arc = manager.get_segment(shmid).ok_or(LinuxError::EINVAL)?;

    let current_uid = 0; // TODO: 从当前进程获取真实的uid
    let current_gid = 0; // TODO: 从当前进程获取真实的gid

    match cmd {
        IPC_STAT => {
            // 获取共享内存段状态
            let segment = segment_arc.read();

            // 检查读权限
            if !segment.check_permission(current_uid, current_gid, false) {
                return Err(LinuxError::EACCES);
            }

            let shmid_ds = segment.to_shmid_ds();
            drop(segment);
            drop(manager);

            // 将数据写入用户空间
            if !buf.is_null() {
                let user_buf = buf.get_as_mut()?;
                *user_buf = shmid_ds;
            } else {
                return Err(LinuxError::EFAULT);
            }

            debug!("sys_shmctl IPC_STAT: successfully retrieved segment info");
            Ok(0)
        }

        IPC_SET => {
            // 设置共享内存段属性
            let mut segment = segment_arc.write();

            // 检查是否有修改权限 (需要是所有者或root)
            if segment.owner_uid != current_uid && current_uid != 0 {
                return Err(LinuxError::EPERM);
            }

            // 从用户空间读取新的属性
            let new_shmid_ds = if !buf.is_null() {
                let user_buf = buf.get_as_mut()?;
                *user_buf
            } else {
                return Err(LinuxError::EFAULT);
            };

            // 更新可修改的字段
            segment.owner_uid = new_shmid_ds.shm_perm.uid;
            segment.owner_gid = new_shmid_ds.shm_perm.gid;
            segment.perm = new_shmid_ds.shm_perm.mode;
            segment.change_time = axhal::time::wall_time().as_secs();

            debug!("sys_shmctl IPC_SET: updated segment attributes");
            Ok(0)
        }

        IPC_RMID => {
            // 标记共享内存段为删除
            let mut segment = segment_arc.write();

            // 检查是否有删除权限 (需要是所有者或root)
            if segment.owner_uid != current_uid && current_uid != 0 {
                return Err(LinuxError::EPERM);
            }

            // 标记为删除
            segment.marked_for_removal = true;
            segment.change_time = axhal::time::wall_time().as_secs();

            let attach_count = segment.attach_count;
            drop(segment);

            // 如果没有进程连接，立即清理
            if attach_count == 0 {
                // 从管理器中移除段
                drop(manager);
                let mut manager = SHM_MANAGER.lock();
                if let Some(removed_segment) = manager.segments.remove(&shmid) {
                    // 清理相关的attachment记录
                    manager.attachments.remove(&shmid);

                    // 获取物理地址用于可能的内存回收
                    let segment = removed_segment.read();
                    let phys_addr = segment.phys_addr;
                    drop(segment);

                    debug!(
                        "sys_shmctl IPC_RMID: immediately removed segment {} (no attachments)",
                        shmid
                    );

                    // TODO: 这里可以添加实际的物理内存释放逻辑
                    // 如果需要释放物理内存，可以在这里实现
                    if phys_addr.as_usize() != 0 {
                        debug!("Physical memory at {:?} can be freed", phys_addr);
                    }
                } else {
                    debug!("sys_shmctl IPC_RMID: segment {} already removed", shmid);
                }
            } else {
                debug!(
                    "sys_shmctl IPC_RMID: marked segment {} for removal ({} attachments remain)",
                    shmid, attach_count
                );
            }

            Ok(0)
        }

        _ => {
            warn!("sys_shmctl: unsupported command {}", cmd);
            Err(LinuxError::EINVAL)
        }
    }
}

/// System call: shmdt - detach shared memory segment
///
/// # Arguments
/// * `shmaddr` - Address of attached shared memory segment
///
/// # Returns
/// * `Ok(0)` - Success
/// * `Err(LinuxError)` - Error code on failure
pub fn sys_shmdt(shmaddr: usize) -> LinuxResult<isize> {
    debug!("sys_shmdt: shmaddr={:#x}", shmaddr);

    if shmaddr == 0 {
        return Err(LinuxError::EINVAL);
    }

    let addr = VirtAddr::from(shmaddr);

    // 检查地址是否页面对齐
    if !addr.is_aligned(PageSize::Size4K) {
        return Err(LinuxError::EINVAL);
    }

    let current_task = current();
    let current_pid = current_task.task_ext().thread.process().pid();

    // 查找该地址对应的共享内存段
    let mut manager = SHM_MANAGER.lock();
    let mut found_shmid = None;

    // 在attachments中查找该地址
    for (shmid, attachments) in manager.attachments.iter() {
        if let Some(&pid) = attachments.get(&addr) {
            if pid == current_pid {
                found_shmid = Some(*shmid);
                break;
            }
        }
    }

    let shmid = found_shmid.ok_or(LinuxError::EINVAL)?;

    // 获取共享内存段信息
    let segment_arc = manager.get_segment(shmid).ok_or(LinuxError::EINVAL)?;

    let segment_size = {
        let segment = segment_arc.read();
        segment.size
    };

    // 从attachments中移除该连接
    if let Some(attachments) = manager.attachments.get_mut(&shmid) {
        attachments.remove(&addr);
        if attachments.is_empty() {
            manager.attachments.remove(&shmid);
        }
    }

    drop(manager); // 释放管理器锁

    // 从进程地址空间中取消映射
    let mut aspace = current_task.task_ext().process_data().aspace.lock();
    let aligned_length = memory_addr::align_up_4k(segment_size);

    match aspace.unmap(addr, aligned_length) {
        Ok(_) => {
            debug!(
                "Successfully unmapped shared memory at address {:#x}",
                shmaddr
            );
        }
        Err(e) => {
            warn!(
                "Failed to unmap shared memory at address {:#x}: {:?}",
                shmaddr, e
            );
            return Err(LinuxError::EINVAL);
        }
    }

    // 更新段的分离信息并检查是否需要清理
    let should_cleanup = {
        let mut segment = segment_arc.write();
        if segment.attach_count > 0 {
            segment.attach_count -= 1;
        }
        segment.detach_time = axhal::time::wall_time().as_secs();

        // 检查是否应该清理段
        segment.marked_for_removal && segment.attach_count == 0
    };

    // 如果段被标记为删除且没有进程连接，则立即清理
    if should_cleanup {
        let mut manager = SHM_MANAGER.lock();
        if let Some(_removed_segment) = manager.segments.remove(&shmid) {
            // 清理相关的attachment记录（应该已经为空）
            manager.attachments.remove(&shmid);
            let _ = aspace.dealloc_shared(
                segment_arc.read().phys_addr,
                aligned_length,
                PageSize::Size4K,
            );
        }
    }

    drop(aspace);

    debug!(
        "Successfully detached shared memory segment {} from address {:#x}",
        shmid, shmaddr
    );

    Ok(0)
}

/// Helper structures and functions for SHM implementation

/// Internal shared memory segment descriptor
#[derive(Debug, Clone)]
pub struct ShmSegment {
    pub key: Key,
    pub size: usize,
    pub perm: u32,
    pub owner_uid: u32,
    pub owner_gid: u32,
    pub creator_uid: u32,
    pub creator_gid: u32,
    pub creator_pid: u32,
    pub last_pid: u32,
    pub attach_count: u32,
    pub change_time: u64,
    pub attach_time: u64,
    pub detach_time: u64,
    pub phys_addr: PhysAddr,
    pub marked_for_removal: bool,
}

impl ShmSegment {
    /// Create a new shared memory segment
    pub fn new(key: Key, size: usize, perm: u32, uid: u32, gid: u32, pid: u32) -> Self {
        let current_time = axhal::time::wall_time().as_secs();
        Self {
            key,
            size,
            perm,
            owner_uid: uid,
            owner_gid: gid,
            creator_uid: uid,
            creator_gid: gid,
            creator_pid: pid,
            last_pid: pid,
            attach_count: 0,
            change_time: current_time,
            attach_time: 0,
            detach_time: 0,
            phys_addr: PhysAddr::from(0),
            marked_for_removal: false,
        }
    }

    /// Convert to shmid_ds structure for user space
    pub fn to_shmid_ds(&self) -> ShmidDs {
        ShmidDs {
            shm_perm: IpcPerm {
                key: self.key,
                uid: self.owner_uid,
                gid: self.owner_gid,
                cuid: self.creator_uid,
                cgid: self.creator_gid,
                mode: self.perm,
                seq: 0,
                _unused1: [0; 5], // Unused fields
            },
            shm_segsz: self.size,
            shm_atime: self.attach_time as i64,
            shm_dtime: self.detach_time as i64,
            shm_ctime: self.change_time as i64,
            shm_cpid: self.creator_pid as i32,
            shm_lpid: self.last_pid as i32,
            shm_nattch: self.attach_count as u64,
            shm_unused: [0; 4], // Unused fields
        }
    }

    /// 检查访问权限
    pub fn check_permission(&self, uid: u32, gid: u32, want_write: bool) -> bool {
        // 所有者检查
        if self.owner_uid == uid {
            let owner_perm = (self.perm >> 6) & 0o7;
            return if want_write {
                owner_perm & 0o2 != 0
            } else {
                owner_perm & 0o4 != 0
            };
        }

        // 组检查
        if self.owner_gid == gid {
            let group_perm = (self.perm >> 3) & 0o7;
            return if want_write {
                group_perm & 0o2 != 0
            } else {
                group_perm & 0o4 != 0
            };
        }

        // 其他用户检查
        let other_perm = self.perm & 0o7;
        if want_write {
            other_perm & 0o2 != 0
        } else {
            other_perm & 0o4 != 0
        }
    }
}

/// SHM管理器 - 全局共享内存段管理
pub struct ShmManager {
    segments: BTreeMap<ShmId, Arc<RwLock<ShmSegment>>>,
    next_id: ShmId,
    attachments: BTreeMap<ShmId, BTreeMap<VirtAddr, u32>>,
}

impl ShmManager {
    pub const fn new() -> Self {
        Self {
            segments: BTreeMap::new(),
            next_id: 1,
            attachments: BTreeMap::new(),
        }
    }

    pub fn get_segment(&self, shmid: ShmId) -> Option<Arc<RwLock<ShmSegment>>> {
        self.segments.get(&shmid).cloned()
    }

    /// 获取共享内存段的只读访问
    pub fn with_segment<T, F>(&self, shmid: ShmId, f: F) -> Option<T>
    where
        F: FnOnce(&ShmSegment) -> T,
    {
        self.segments.get(&shmid).map(|segment| {
            let seg = segment.read();
            f(&*seg)
        })
    }

    pub fn create_segment(
        &mut self,
        key: Key,
        size: usize,
        perm: u32,
    ) -> Result<ShmId, LinuxError> {
        // 验证大小参数
        if size == 0 {
            return Err(LinuxError::EINVAL);
        }

        // 生成新的segment ID并创建唯一的共享内存名称
        let shmid = self.next_id;
        self.next_id += 1;

        // 创建ShmSegment
        let current_task = current();
        let creator_pid = current_task.task_ext().thread.process().pid();
        let uid = 0; // TODO: 从当前进程获取真实的uid/gid
        let gid = 0;

        let segment = ShmSegment::new(key, size, perm, uid, gid, creator_pid);

        // 将段添加到管理器
        self.segments.insert(shmid, Arc::new(RwLock::new(segment)));

        debug!(
            "Created shared memory segment: id={}, key={}, size={}",
            shmid, key, size
        );

        Ok(shmid)
    }
}

// 全局实例
static SHM_MANAGER: Mutex<ShmManager> = Mutex::new(ShmManager::new());
