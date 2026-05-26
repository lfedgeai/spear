pub const SPEAR_OK: i32 = 0;

pub const SPEAR_EIO: i32 = 5;
pub const SPEAR_EBADF: i32 = 9;
pub const SPEAR_EAGAIN: i32 = 11;
pub const SPEAR_EACCES: i32 = 13;
pub const SPEAR_EFAULT: i32 = 14;
pub const SPEAR_EINVAL: i32 = 22;
pub const SPEAR_ENOSPC: i32 = 28;
pub const SPEAR_EPIPE: i32 = 32;
pub const SPEAR_ENOTCONN: i32 = 107;

pub const EIO: i32 = SPEAR_EIO;
pub const EBADF: i32 = SPEAR_EBADF;
pub const EACCES: i32 = SPEAR_EACCES;
pub const EINVAL: i32 = SPEAR_EINVAL;

#[cfg(test)]
pub const EAGAIN: i32 = SPEAR_EAGAIN;
