# Socket系统调用实现计划
## 第一阶段：核心Socket系统调用
实现基础socket系统调用
- sys_socket() - 创建socket
- sys_bind() - 绑定地址
- sys_listen() - 监听连接
- sys_accept() - 接受连接
- sys_connect() - 建立连接
## 第二阶段：数据传输系统调用
实现数据传输调用
- sys_send() - 发送数据
- sys_recv() - 接收数据
- sys_sendto() - 发送到指定地址
- sys_recvfrom() - 从指定地址接收

> 利用现有Socket实现：
可以直接使用 net.rs:29-53 中已实现的recv()、sendto()、recvfrom()方法

## 第三阶段：Socket选项和控制
实现socket选项调用
- sys_setsockopt() - 设置socket选项
- sys_getsockopt() - 获取socket选项
- sys_shutdown() - 关闭socket连接

> 扩展现有ioctl支持：
当前 ctl.rs:28-31 的sys_ioctl()只是占位实现，需要添加socket相关的ioctl操作

## 第四阶段：高级功能（暂时不做）
实现多路复用支持
- sys_select() - 多路复用I/O
- sys_poll() - 轮询I/O事件
- sys_epoll_create() - 创建epoll实例

> 集成现有poll机制 net.rs:100-102 已实现poll方法，可以作为基础