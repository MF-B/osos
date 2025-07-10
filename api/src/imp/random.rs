use crate::ptr::UserPtr;
use axerrno::LinuxResult;

/// Generate random bytes and fill the buffer  
///   
/// # Arguments  
/// * `buf` - User buffer to fill with random bytes  
/// * `buflen` - Length of the buffer  
/// * `flags` - Flags (currently unused, for compatibility)  
///   
/// # Returns  
/// Number of bytes written on success  
pub fn sys_getrandom(buf: UserPtr<u8>, buflen: usize, flags: u32) -> LinuxResult<isize> {
    debug!(
        "sys_getrandom <= buf: {:?}, buflen: {}, flags: {}",
        buf.address(), buflen, flags
    );

    if buflen == 0 {
        return Ok(0);
    }

    // 获取用户缓冲区
    let user_buf = buf.get_as_mut_slice(buflen)?;

    // 填充随机字节
    for chunk in user_buf.chunks_mut(16) {
        // 使用 axhal 生成 128 位随机数
        let random_u128 = axhal::misc::random();
        let random_bytes = random_u128.to_le_bytes();
        
        // 复制到用户缓冲区，处理最后一个不完整的块
        let copy_len = chunk.len().min(16);
        chunk[..copy_len].copy_from_slice(&random_bytes[..copy_len]);
    }

    debug!("sys_getrandom => {}", buflen);
    Ok(buflen as isize)
}
