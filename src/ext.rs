// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(bad_style, dead_code)]

use std::io;
use std::mem;
use std::net::{TcpStream, TcpListener, UdpSocket, Ipv4Addr, Ipv6Addr};
use std::net::ToSocketAddrs;

use libc::{self, c_int, socklen_t, c_void, c_uint};

use {TcpBuilder, UdpBuilder, FromInner};
use sys;
use socket;

#[cfg(feature = "nightly")] use std::time::Duration;

#[cfg(unix)] pub type Socket = c_int;
#[cfg(unix)] use std::os::unix::prelude::*;
#[cfg(windows)] pub type Socket = libc::SOCKET;
#[cfg(windows)] use std::os::windows::prelude::*;
#[cfg(windows)] use ws2_32::*;

#[cfg(target_os = "linux")] const IPV6_MULTICAST_LOOP: c_int = 19;
#[cfg(any(target_os = "macos", target_os = "ios"))] const IPV6_MULTICAST_LOOP: c_int = 11;
#[cfg(target_os = "freebsd")] const IPV6_MULTICAST_LOOP: c_int = 11;
#[cfg(target_os = "dragonfly")] const IPV6_MULTICAST_LOOP: c_int = 11;
#[cfg(target_os = "windows")] const IPV6_MULTICAST_LOOP: c_int = 11;
#[cfg(target_os = "linux")] const IPV6_V6ONLY: c_int = 26;
#[cfg(any(target_os = "macos", target_os = "ios"))] const IPV6_V6ONLY: c_int = 27;
#[cfg(target_os = "windows")] const IPV6_V6ONLY: c_int = 27;
#[cfg(target_os = "freebsd")] const IPV6_V6ONLY: c_int = 27;
#[cfg(target_os = "dragonfly")] const IPV6_V6ONLY: c_int = 27;

cfg_if! {
    if #[cfg(windows)] {
        use libc::FIONBIO;
    } else if #[cfg(any(target_os = "linux", target_os = "android"))] {
        const FIONBIO: c_int = 0x5421;
    } else {
        const FIONBIO: libc::c_ulong = 0x8004667e;
    }
}

#[cfg(windows)] const SIO_KEEPALIVE_VALS: libc::DWORD = 0x98000004;
#[cfg(windows)]
#[repr(C)]
struct tcp_keepalive {
    onoff: libc::c_ulong,
    keepalivetime: libc::c_ulong,
    keepaliveinterval: libc::c_ulong,
}

#[cfg(not(windows))]
extern "system" {
    fn getsockopt(sockfd: Socket,
                  level: c_int,
                  optname: c_int,
                  optval: *mut c_void,
                  optlen: *mut socklen_t) -> c_int;
}

pub fn setopt<T: Copy>(sock: Socket, opt: c_int, val: c_int,
                       payload: T) -> io::Result<()> {
    unsafe {
        let payload = &payload as *const T as *const c_void;
        try!(::cvt(libc::setsockopt(sock, opt, val, payload,
                                    mem::size_of::<T>() as socklen_t)));
        Ok(())
    }
}

fn getopt<T: Copy>(sock: Socket, opt: c_int, val: c_int) -> io::Result<T> {
    unsafe {
        let mut slot: T = mem::zeroed();
        let mut len = mem::size_of::<T>() as socklen_t;
        try!(::cvt(getsockopt(sock, opt, val, &mut slot as *mut _ as *mut _,
                              &mut len)));
        assert_eq!(len as usize, mem::size_of::<T>());
        Ok(slot)
    }
}

/// Extension methods for the standard [`TcpStream` type][link] in `std::net`.
///
/// [link]: https://doc.rust-lang.org/std/net/struct.TcpStream.html
pub trait TcpStreamExt {
    /// Sets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// If set, this option disables the Nagle algorithm. This means that
    /// segments are always sent as soon as possible, even if there is only a
    /// small amount of data. When not set, data is buffered until there is a
    /// sufficient amount to send out, thereby avoiding the frequent sending of
    /// small packets.
    fn set_nodelay(&self, nodelay: bool) -> io::Result<()>;

    /// Gets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// For more information about this option, see [`set_nodelay`][link].
    ///
    /// [link]: #tymethod.set_nodelay
    fn nodelay(&self) -> io::Result<bool>;

    /// Sets whether keepalive messages are enabled to be sent on this socket.
    ///
    /// On Unix, this option will set the `SO_KEEPALIVE` as well as the
    /// `TCP_KEEPALIVE` or `TCP_KEEPIDLE` option (depending on your platform).
    /// On Windows, this will set the `SIO_KEEPALIVE_VALS` option.
    ///
    /// If `None` is specified then keepalive messages are disabled, otherwise
    /// the number of milliseconds specified will be the time to remain idle
    /// before sending a TCP keepalive probe.
    ///
    /// Some platforms specify this value in seconds, so sub-second millisecond
    /// specifications may be omitted.
    fn set_keepalive_ms(&self, keepalive: Option<u32>) -> io::Result<()>;

    /// Returns whether keepalive messages are enabled on this socket, and if so
    /// the amount of milliseconds between them.
    ///
    /// For more information about this option, see [`set_keepalive_ms`][link].
    ///
    /// [link]: #tymethod.set_keepalive_ms
    fn keepalive_ms(&self) -> io::Result<Option<u32>>;

    /// Sets whether keepalive messages are enabled to be sent on this socket.
    ///
    /// On Unix, this option will set the `SO_KEEPALIVE` as well as the
    /// `TCP_KEEPALIVE` or `TCP_KEEPIDLE` option (depending on your platform).
    /// On Windows, this will set the `SIO_KEEPALIVE_VALS` option.
    ///
    /// If `None` is specified then keepalive messages are disabled, otherwise
    /// the duration specified will be the time to remain idle before sending a
    /// TCP keepalive probe.
    ///
    /// Some platforms specify this value in seconds, so sub-second
    /// specifications may be omitted.
    #[cfg(feature = "nightly")]
    fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()>;

    /// Returns whether keepalive messages are enabled on this socket, and if so
    /// the duration of time between them.
    ///
    /// For more information about this option, see [`set_keepalive`][link].
    ///
    /// [link]: #tymethod.set_keepalive
    #[cfg(feature = "nightly")]
    fn keepalive(&self) -> io::Result<Option<Duration>>;

    /// Sets the `SO_RCVTIMEO` option for this socket.
    ///
    /// This option specifies the timeout, in milliseconds, of how long calls to
    /// this socket's `read` function will wait before returning a timeout. A
    /// value of `None` means that no read timeout should be specified and
    /// otherwise `Some` indicates the number of milliseconds for the timeout.
    fn set_read_timeout_ms(&self, val: Option<u32>) -> io::Result<()>;

    /// Sets the `SO_RCVTIMEO` option for this socket.
    ///
    /// This option specifies the timeout of how long calls to this socket's
    /// `read` function will wait before returning a timeout. A value of `None`
    /// means that no read timeout should be specified and otherwise `Some`
    /// indicates the number of duration of the timeout.
    #[cfg(feature = "nightly")]
    fn set_read_timeout(&self, val: Option<Duration>) -> io::Result<()>;

    /// Gets the value of the `SO_RCVTIMEO` option for this socket.
    ///
    /// For more information about this option, see [`set_read_timeout_ms`][link].
    ///
    /// [link]: #tymethod.set_read_timeout_ms
    fn read_timeout_ms(&self) -> io::Result<Option<u32>>;

    /// Gets the value of the `SO_RCVTIMEO` option for this socket.
    ///
    /// For more information about this option, see [`set_read_timeout`][link].
    ///
    /// [link]: #tymethod.set_read_timeout
    #[cfg(feature = "nightly")]
    fn read_timeout(&self) -> io::Result<Option<Duration>>;

    /// Sets the `SO_SNDTIMEO` option for this socket.
    ///
    /// This option specifies the timeout, in milliseconds, of how long calls to
    /// this socket's `write` function will wait before returning a timeout. A
    /// value of `None` means that no read timeout should be specified and
    /// otherwise `Some` indicates the number of milliseconds for the timeout.
    fn set_write_timeout_ms(&self, val: Option<u32>) -> io::Result<()>;

    /// Sets the `SO_SNDTIMEO` option for this socket.
    ///
    /// This option specifies the timeout of how long calls to this socket's
    /// `write` function will wait before returning a timeout. A value of `None`
    /// means that no read timeout should be specified and otherwise `Some`
    /// indicates the duration of the timeout.
    #[cfg(feature = "nightly")]
    fn set_write_timeout(&self, val: Option<Duration>) -> io::Result<()>;

    /// Gets the value of the `SO_SNDTIMEO` option for this socket.
    ///
    /// For more information about this option, see [`set_write_timeout_ms`][link].
    ///
    /// [link]: #tymethod.set_write_timeout_ms
    fn write_timeout_ms(&self) -> io::Result<Option<u32>>;

    /// Gets the value of the `SO_SNDTIMEO` option for this socket.
    ///
    /// For more information about this option, see [`set_write_timeout`][link].
    ///
    /// [link]: #tymethod.set_write_timeout
    #[cfg(feature = "nightly")]
    fn write_timeout(&self) -> io::Result<Option<Duration>>;

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    fn set_ttl(&self, ttl: u32) -> io::Result<()>;

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`][link].
    ///
    /// [link]: #tymethod.set_ttl
    fn ttl(&self) -> io::Result<u32>;

    /// Sets the value for the `IPV6_V6ONLY` option on this socket.
    ///
    /// If this is set to `true` then the socket is restricted to sending and
    /// receiving IPv6 packets only. In this case two IPv4 and IPv6 applications
    /// can bind the same port at the same time.
    ///
    /// If this is set to `false` then the socket can be used to send and
    /// receive packets from an IPv4-mapped IPv6 address.
    fn set_only_v6(&self, only_v6: bool) -> io::Result<()>;

    /// Gets the value of the `IPV6_V6ONLY` option for this socket.
    ///
    /// For more information about this option, see [`set_only_v6`][link].
    ///
    /// [link]: #tymethod.set_only_v6
    fn only_v6(&self) -> io::Result<bool>;

    /// Executes a `connect` operation on this socket, establishing a connection
    /// to the host specified by `addr`.
    ///
    /// Note that this normally does not need to be called on a `TcpStream`,
    /// it's typically automatically done as part of a normal
    /// `TcpStream::connect` function call or `TcpBuilder::connect` method call.
    ///
    /// This should only be necessary if an unconnected socket was extracted
    /// from a `TcpBuilder` and then needs to be connected.
    fn connect<T: ToSocketAddrs>(&self, addr: T) -> io::Result<()>;

    /// Get the value of the `SO_ERROR` option on this socket.
    ///
    /// This will retrieve the stored error in the underlying socket, clearing
    /// the field in the process. This can be useful for checking errors between
    /// calls.
    fn take_error(&self) -> io::Result<Option<io::Error>>;

    /// Moves this TCP stream into or out of nonblocking mode.
    ///
    /// On Unix this corresponds to calling fcntl, and on Windows this
    /// corresponds to calling ioctlsocket.
    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()>;
}

/// Extension methods for the standard [`TcpListener` type][link] in `std::net`.
///
/// [link]: https://doc.rust-lang.org/std/net/struct.TcpListener.html
pub trait TcpListenerExt {
    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This is the same as [`TcpStreamExt::set_ttl`][other].
    ///
    /// [other]: trait.TcpStreamExt.html#tymethod.set_ttl
    fn set_ttl(&self, ttl: u32) -> io::Result<()>;

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see
    /// [`TcpStreamExt::set_ttl`][link].
    ///
    /// [link]: trait.TcpStreamExt.html#tymethod.set_ttl
    fn ttl(&self) -> io::Result<u32>;

    /// Sets the value for the `IPV6_V6ONLY` option on this socket.
    ///
    /// For more information about this option, see
    /// [`TcpStreamExt::set_only_v6`][link].
    ///
    /// [link]: trait.TcpStreamExt.html#tymethod.set_only_v6
    fn set_only_v6(&self, only_v6: bool) -> io::Result<()>;

    /// Gets the value of the `IPV6_V6ONLY` option for this socket.
    ///
    /// For more information about this option, see
    /// [`TcpStreamExt::set_only_v6`][link].
    ///
    /// [link]: trait.TcpStreamExt.html#tymethod.set_only_v6
    fn only_v6(&self) -> io::Result<bool>;

    /// Get the value of the `SO_ERROR` option on this socket.
    ///
    /// This will retrieve the stored error in the underlying socket, clearing
    /// the field in the process. This can be useful for checking errors between
    /// calls.
    fn take_error(&self) -> io::Result<Option<io::Error>>;

    /// Moves this TCP listener into or out of nonblocking mode.
    ///
    /// For more information about this option, see
    /// [`TcpStreamExt::set_nonblocking`][link].
    ///
    /// [link]: trait.TcpStreamExt.html#tymethod.set_nonblocking
    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()>;
}

/// Extension methods for the standard [`UdpSocket` type][link] in `std::net`.
///
/// [link]: https://doc.rust-lang.org/std/net/struct.UdpSocket.html
pub trait UdpSocketExt {
    /// Sets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// When enabled, this socket is allowed to send packets to a broadcast
    /// address.
    fn set_broadcast(&self, broadcast: bool) -> io::Result<()>;

    /// Gets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_broadcast`][link].
    ///
    /// [link]: #tymethod.set_broadcast
    fn broadcast(&self) -> io::Result<bool>;

    /// Sets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// If enabled, multicast packets will be looped back to the local socket.
    /// Note that this may not have any affect on IPv6 sockets.
    fn set_multicast_loop_v4(&self, multicast_loop_v4: bool) -> io::Result<()>;

    /// Gets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_multicast_loop_v4`][link].
    ///
    /// [link]: #tymethod.set_multicast_loop_v4
    fn multicast_loop_v4(&self) -> io::Result<bool>;

    /// Sets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// Indicates the time-to-live value of outgoing multicast packets for
    /// this socket. The default value is 1 which means that multicast packets
    /// don't leave the local network unless explicitly requested.
    ///
    /// Note that this may not have any affect on IPv6 sockets.
    fn set_multicast_ttl_v4(&self, multicast_ttl_v4: u32) -> io::Result<()>;

    /// Gets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_multicast_ttl_v4`][link].
    ///
    /// [link]: #tymethod.set_multicast_ttl_v4
    fn multicast_ttl_v4(&self) -> io::Result<u32>;

    /// Sets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// Controls whether this socket sees the multicast packets it sends itself.
    /// Note that this may not have any affect on IPv4 sockets.
    fn set_multicast_loop_v6(&self, multicast_loop_v6: bool) -> io::Result<()>;

    /// Gets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_multicast_loop_v6`][link].
    ///
    /// [link]: #tymethod.set_multicast_loop_v6
    fn multicast_loop_v6(&self) -> io::Result<bool>;

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This is the same as [`TcpStreamExt::set_ttl`][other].
    ///
    /// [other]: trait.TcpStreamExt.html#tymethod.set_ttl
    fn set_ttl(&self, ttl: u32) -> io::Result<()>;

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see
    /// [`TcpStreamExt::set_ttl`][link].
    ///
    /// [link]: trait.TcpStreamExt.html#tymethod.set_ttl
    fn ttl(&self) -> io::Result<u32>;

    /// Sets the value for the `IPV6_V6ONLY` option on this socket.
    ///
    /// For more information about this option, see
    /// [`TcpStreamExt::set_only_v6`][link].
    ///
    /// [link]: trait.TcpStreamExt.html#tymethod.set_only_v6
    fn set_only_v6(&self, only_v6: bool) -> io::Result<()>;

    /// Gets the value of the `IPV6_V6ONLY` option for this socket.
    ///
    /// For more information about this option, see
    /// [`TcpStreamExt::set_only_v6`][link].
    ///
    /// [link]: trait.TcpStreamExt.html#tymethod.set_only_v6
    fn only_v6(&self) -> io::Result<bool>;

    /// Executes an operation of the `IP_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// address of the local interface with which the system should join the
    /// multicast group. If it's equal to `INADDR_ANY` then an appropriate
    /// interface is chosen by the system.
    fn join_multicast_v4(&self, multiaddr: &Ipv4Addr, interface: &Ipv4Addr)
                         -> io::Result<()>;

    /// Executes an operation of the `IPV6_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// index of the interface to join/leave (or 0 to indicate any interface).
    fn join_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32)
                         -> io::Result<()>;

    /// Executes an operation of the `IP_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see
    /// [`join_multicast_v4`][link].
    ///
    /// [link]: #tymethod.join_multicast_v4
    fn leave_multicast_v4(&self, multiaddr: &Ipv4Addr, interface: &Ipv4Addr)
                          -> io::Result<()>;

    /// Executes an operation of the `IPV6_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see
    /// [`join_multicast_v6`][link].
    ///
    /// [link]: #tymethod.join_multicast_v6
    fn leave_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32)
                          -> io::Result<()>;

    /// Sets the `SO_RCVTIMEO` option for this socket.
    ///
    /// This option specifies the timeout, in milliseconds, of how long calls to
    /// this socket's `read` function will wait before returning a timeout. A
    /// value of `None` means that no read timeout should be specified and
    /// otherwise `Some` indicates the number of milliseconds for the timeout.
    fn set_read_timeout_ms(&self, val: Option<u32>) -> io::Result<()>;

    /// Sets the `SO_RCVTIMEO` option for this socket.
    ///
    /// This option specifies the timeout of how long calls to this socket's
    /// `read` function will wait before returning a timeout. A value of `None`
    /// means that no read timeout should be specified and otherwise `Some`
    /// indicates the number of duration of the timeout.
    #[cfg(feature = "nightly")]
    fn set_read_timeout(&self, val: Option<Duration>) -> io::Result<()>;

    /// Gets the value of the `SO_RCVTIMEO` option for this socket.
    ///
    /// For more information about this option, see [`set_read_timeout_ms`][link].
    ///
    /// [link]: #tymethod.set_read_timeout_ms
    fn read_timeout_ms(&self) -> io::Result<Option<u32>>;

    /// Gets the value of the `SO_RCVTIMEO` option for this socket.
    ///
    /// For more information about this option, see [`set_read_timeout`][link].
    ///
    /// [link]: #tymethod.set_read_timeout
    #[cfg(feature = "nightly")]
    fn read_timeout(&self) -> io::Result<Option<Duration>>;

    /// Sets the `SO_SNDTIMEO` option for this socket.
    ///
    /// This option specifies the timeout, in milliseconds, of how long calls to
    /// this socket's `write` function will wait before returning a timeout. A
    /// value of `None` means that no read timeout should be specified and
    /// otherwise `Some` indicates the number of milliseconds for the timeout.
    fn set_write_timeout_ms(&self, val: Option<u32>) -> io::Result<()>;

    /// Sets the `SO_SNDTIMEO` option for this socket.
    ///
    /// This option specifies the timeout of how long calls to this socket's
    /// `write` function will wait before returning a timeout. A value of `None`
    /// means that no read timeout should be specified and otherwise `Some`
    /// indicates the duration of the timeout.
    #[cfg(feature = "nightly")]
    fn set_write_timeout(&self, val: Option<Duration>) -> io::Result<()>;

    /// Gets the value of the `SO_SNDTIMEO` option for this socket.
    ///
    /// For more information about this option, see [`set_write_timeout_ms`][link].
    ///
    /// [link]: #tymethod.set_write_timeout_ms
    fn write_timeout_ms(&self) -> io::Result<Option<u32>>;

    /// Gets the value of the `SO_SNDTIMEO` option for this socket.
    ///
    /// For more information about this option, see [`set_write_timeout`][link].
    ///
    /// [link]: #tymethod.set_write_timeout
    #[cfg(feature = "nightly")]
    fn write_timeout(&self) -> io::Result<Option<Duration>>;

    /// Get the value of the `SO_ERROR` option on this socket.
    ///
    /// This will retrieve the stored error in the underlying socket, clearing
    /// the field in the process. This can be useful for checking errors between
    /// calls.
    fn take_error(&self) -> io::Result<Option<io::Error>>;

    /// Connects this UDP socket to a remote address, allowing the `send` and
    /// `recv` syscalls to be used to send data and also applies filters to only
    /// receive data from the specified address.
    fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()>;

    /// Moves this UDP socket into or out of nonblocking mode.
    ///
    /// For more information about this option, see
    /// [`TcpStreamExt::set_nonblocking`][link].
    ///
    /// [link]: trait.TcpStreamExt.html#tymethod.set_nonblocking
    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()>;
}

#[doc(hidden)]
pub trait AsSock {
    fn as_sock(&self) -> Socket;
}

#[cfg(unix)]
impl<T: AsRawFd> AsSock for T {
    fn as_sock(&self) -> Socket { self.as_raw_fd() }
}
#[cfg(windows)]
impl<T: AsRawSocket> AsSock for T {
    fn as_sock(&self) -> Socket { self.as_raw_socket() }
}

cfg_if! {
    if #[cfg(any(target_os = "macos", target_os = "ios"))] {
        const KEEPALIVE_OPTION: libc::c_int = libc::TCP_KEEPALIVE;
    } else if #[cfg(unix)] {
        const KEEPALIVE_OPTION: libc::c_int = libc::TCP_KEEPIDLE;
    } else {
    }
}

impl TcpStreamExt for TcpStream {
    fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        setopt(self.as_sock(), libc::IPPROTO_TCP, libc::TCP_NODELAY,
               nodelay as c_int)
    }
    fn nodelay(&self) -> io::Result<bool> {
        getopt(self.as_sock(), libc::IPPROTO_TCP, libc::TCP_NODELAY)
            .map(int2bool)
    }

    #[cfg(feature = "nightly")]
    fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()> {
        self.set_keepalive_ms(keepalive.map(dur2ms))
    }

    #[cfg(feature = "nightly")]
    fn keepalive(&self) -> io::Result<Option<Duration>> {
        self.keepalive_ms().map(|o| o.map(ms2dur))
    }

    #[cfg(unix)]
    fn set_keepalive_ms(&self, keepalive: Option<u32>) -> io::Result<()> {
        try!(setopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_KEEPALIVE,
                    keepalive.is_some() as c_int));
        if let Some(dur) = keepalive {
            try!(setopt(self.as_sock(), libc::IPPROTO_TCP, KEEPALIVE_OPTION,
                        dur as c_int));
        }
        Ok(())
    }

    #[cfg(unix)]
    fn keepalive_ms(&self) -> io::Result<Option<u32>> {
        let keepalive = try!(getopt::<c_int>(self.as_sock(), libc::SOL_SOCKET,
                                             libc::SO_KEEPALIVE));
        if keepalive == 0 {
            return Ok(None)
        }
        let secs = try!(getopt::<c_int>(self.as_sock(), libc::IPPROTO_TCP,
                                        KEEPALIVE_OPTION));
        Ok(Some(secs as u32))
    }

    #[cfg(windows)]
    fn set_keepalive_ms(&self, keepalive: Option<u32>) -> io::Result<()> {
        let ms = keepalive.unwrap_or(libc::INFINITE);
        let ka = tcp_keepalive {
            onoff: keepalive.is_some() as libc::c_ulong,
            keepalivetime: ms as libc::c_ulong,
            keepaliveinterval: ms as libc::c_ulong,
        };
        unsafe {
            ::cvt_win(WSAIoctl(self.as_sock(),
                               SIO_KEEPALIVE_VALS,
                               &ka as *const _ as *mut _,
                               mem::size_of_val(&ka) as libc::DWORD,
                               0 as *mut _,
                               0,
                               0 as *mut _,
                               0 as *mut _,
                               None)).map(|_| ())
        }
    }

    #[cfg(windows)]
    fn keepalive_ms(&self) -> io::Result<Option<u32>> {
        let mut ka = tcp_keepalive {
            onoff: 0,
            keepalivetime: 0,
            keepaliveinterval: 0,
        };
        unsafe {
            try!(::cvt_win(WSAIoctl(self.as_sock(),
                                    SIO_KEEPALIVE_VALS,
                                    0 as *mut _,
                                    0,
                                    &mut ka as *mut _ as *mut _,
                                    mem::size_of_val(&ka) as libc::DWORD,
                                    0 as *mut _,
                                    0 as *mut _,
                                    None)));
        }
        Ok({
            if ka.onoff == 0 {
                None
            } else {
                timeout2ms(ka.keepaliveinterval as libc::DWORD)
            }
        })
    }

    fn set_read_timeout_ms(&self, dur: Option<u32>) -> io::Result<()> {
        setopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_RCVTIMEO,
               ms2timeout(dur))
    }

    fn read_timeout_ms(&self) -> io::Result<Option<u32>> {
        getopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_RCVTIMEO)
            .map(timeout2ms)
    }

    fn set_write_timeout_ms(&self, dur: Option<u32>) -> io::Result<()> {
        setopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_SNDTIMEO,
               ms2timeout(dur))
    }

    fn write_timeout_ms(&self) -> io::Result<Option<u32>> {
        getopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_SNDTIMEO)
            .map(timeout2ms)
    }

    #[cfg(feature = "nightly")]
    fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout_ms(dur.map(dur2ms))
    }

    #[cfg(feature = "nightly")]
    fn read_timeout(&self) -> io::Result<Option<Duration>> {
        self.read_timeout_ms().map(|o| o.map(ms2dur))
    }

    #[cfg(feature = "nightly")]
    fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.set_write_timeout_ms(dur.map(dur2ms))
    }

    #[cfg(feature = "nightly")]
    fn write_timeout(&self) -> io::Result<Option<Duration>> {
        self.write_timeout_ms().map(|o| o.map(ms2dur))
    }

    fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        setopt(self.as_sock(), libc::IPPROTO_IP, libc::IP_TTL, ttl as c_int)
    }

    fn ttl(&self) -> io::Result<u32> {
        getopt::<c_int>(self.as_sock(), libc::IPPROTO_IP, libc::IP_TTL)
            .map(|b| b as u32)
    }

    fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        setopt(self.as_sock(), libc::IPPROTO_IPV6, IPV6_V6ONLY, only_v6 as c_int)
    }

    fn only_v6(&self) -> io::Result<bool> {
        getopt(self.as_sock(), libc::IPPROTO_IPV6, IPV6_V6ONLY).map(int2bool)
    }

    fn connect<T: ToSocketAddrs>(&self, addr: T) -> io::Result<()> {
        do_connect(self.as_sock(), addr)
    }

    fn take_error(&self) -> io::Result<Option<io::Error>> {
        getopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_ERROR).map(int2err)
    }

    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        set_nonblocking(self.as_sock(), nonblocking)
    }
}

#[cfg(unix)]
fn ms2timeout(dur: Option<u32>) -> libc::timeval {
    // TODO: be more rigorous
    match dur {
        Some(d) => libc::timeval {
            tv_sec: (d / 1000) as libc::time_t,
            tv_usec: (d % 1000) as libc::suseconds_t,
        },
        None => libc::timeval { tv_sec: 0, tv_usec: 0 },
    }
}

#[cfg(unix)]
fn timeout2ms(dur: libc::timeval) -> Option<u32> {
    if dur.tv_sec == 0 && dur.tv_usec == 0 {
        None
    } else {
        Some(dur.tv_sec as u32 * 1000 + dur.tv_usec as u32 / 1000)
    }
}

#[cfg(windows)]
fn ms2timeout(dur: Option<u32>) -> libc::DWORD {
    dur.unwrap_or(0)
}

#[cfg(windows)]
fn timeout2ms(dur: libc::DWORD) -> Option<u32> {
    if dur == 0 {
        None
    } else {
        Some(dur)
    }
}

#[cfg(feature = "nightly")]
fn ms2dur(ms: u32) -> Duration {
    Duration::new((ms as u64) / 1000, (ms as u32) % 1000 * 1_000_000)
}

#[cfg(feature = "nightly")]
fn dur2ms(dur: Duration) -> u32 {
    (dur.as_secs() as u32 * 1000) + (dur.subsec_nanos() / 1_000_000)
}

fn int2bool(n: c_int) -> bool {
    if n == 0 {false} else {true}
}

fn int2err(n: c_int) -> Option<io::Error> {
    if n == 0 {
        None
    } else {
        Some(io::Error::from_raw_os_error(n as i32))
    }
}

impl UdpSocketExt for UdpSocket {
    fn set_broadcast(&self, broadcast: bool) -> io::Result<()> {
        setopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_BROADCAST,
               broadcast as c_int)
    }
    fn broadcast(&self) -> io::Result<bool> {
        getopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_BROADCAST)
            .map(int2bool)
    }
    fn set_multicast_loop_v4(&self, multicast_loop_v4: bool) -> io::Result<()> {
        setopt(self.as_sock(), libc::IPPROTO_IP, libc::IP_MULTICAST_LOOP,
               multicast_loop_v4 as c_int)
    }
    fn multicast_loop_v4(&self) -> io::Result<bool> {
        getopt(self.as_sock(), libc::IPPROTO_IP, libc::IP_MULTICAST_LOOP)
            .map(int2bool)
    }
    fn set_multicast_ttl_v4(&self, multicast_ttl_v4: u32) -> io::Result<()> {
        setopt(self.as_sock(), libc::IPPROTO_IP, libc::IP_MULTICAST_TTL,
               multicast_ttl_v4 as c_int)
    }
    fn multicast_ttl_v4(&self) -> io::Result<u32> {
        getopt::<c_int>(self.as_sock(), libc::IPPROTO_IP, libc::IP_MULTICAST_TTL)
            .map(|b| b as u32)
    }
    fn set_multicast_loop_v6(&self, multicast_loop_v6: bool) -> io::Result<()> {
        setopt(self.as_sock(), libc::IPPROTO_IPV6, IPV6_MULTICAST_LOOP,
               multicast_loop_v6 as c_int)
    }
    fn multicast_loop_v6(&self) -> io::Result<bool> {
        getopt(self.as_sock(), libc::IPPROTO_IPV6, IPV6_MULTICAST_LOOP)
            .map(int2bool)
    }

    fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        setopt(self.as_sock(), libc::IPPROTO_IP, libc::IP_TTL, ttl as c_int)
    }

    fn ttl(&self) -> io::Result<u32> {
        getopt::<c_int>(self.as_sock(), libc::IPPROTO_IP, libc::IP_TTL)
            .map(|b| b as u32)
    }

    fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        setopt(self.as_sock(), libc::IPPROTO_IPV6, IPV6_V6ONLY, only_v6 as c_int)
    }

    fn only_v6(&self) -> io::Result<bool> {
        getopt(self.as_sock(), libc::IPPROTO_IPV6, IPV6_V6ONLY).map(int2bool)
    }

    fn join_multicast_v4(&self, multiaddr: &Ipv4Addr, interface: &Ipv4Addr)
                         -> io::Result<()> {
        let mreq = libc::ip_mreq {
            imr_multiaddr: ip2in_addr(multiaddr),
            imr_interface: ip2in_addr(interface),
        };
        setopt(self.as_sock(), libc::IPPROTO_IP, libc::IP_ADD_MEMBERSHIP, mreq)
    }

    fn join_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32)
                         -> io::Result<()> {
        let mreq = libc::ip6_mreq {
            ipv6mr_multiaddr: ip2in6_addr(multiaddr),
            ipv6mr_interface: interface as c_uint,
        };
        setopt(self.as_sock(), libc::IPPROTO_IPV6, libc::IPV6_ADD_MEMBERSHIP,
               mreq)
    }

    fn leave_multicast_v4(&self, multiaddr: &Ipv4Addr, interface: &Ipv4Addr)
                          -> io::Result<()> {
        let mreq = libc::ip_mreq {
            imr_multiaddr: ip2in_addr(multiaddr),
            imr_interface: ip2in_addr(interface),
        };
        setopt(self.as_sock(), libc::IPPROTO_IP, libc::IP_DROP_MEMBERSHIP, mreq)
    }

    fn leave_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32)
                          -> io::Result<()> {
        let mreq = libc::ip6_mreq {
            ipv6mr_multiaddr: ip2in6_addr(multiaddr),
            ipv6mr_interface: interface as c_uint,
        };
        setopt(self.as_sock(), libc::IPPROTO_IPV6, libc::IPV6_DROP_MEMBERSHIP,
               mreq)
    }

    fn set_read_timeout_ms(&self, dur: Option<u32>) -> io::Result<()> {
        setopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_RCVTIMEO,
               ms2timeout(dur))
    }

    fn read_timeout_ms(&self) -> io::Result<Option<u32>> {
        getopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_RCVTIMEO)
            .map(timeout2ms)
    }

    fn set_write_timeout_ms(&self, dur: Option<u32>) -> io::Result<()> {
        setopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_SNDTIMEO,
               ms2timeout(dur))
    }

    fn write_timeout_ms(&self) -> io::Result<Option<u32>> {
        getopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_SNDTIMEO)
            .map(timeout2ms)
    }

    #[cfg(feature = "nightly")]
    fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout_ms(dur.map(dur2ms))
    }

    #[cfg(feature = "nightly")]
    fn read_timeout(&self) -> io::Result<Option<Duration>> {
        self.read_timeout_ms().map(|o| o.map(ms2dur))
    }

    #[cfg(feature = "nightly")]
    fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.set_write_timeout_ms(dur.map(dur2ms))
    }

    #[cfg(feature = "nightly")]
    fn write_timeout(&self) -> io::Result<Option<Duration>> {
        self.write_timeout_ms().map(|o| o.map(ms2dur))
    }

    fn take_error(&self) -> io::Result<Option<io::Error>> {
        getopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_ERROR).map(int2err)
    }

    fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()> {
        do_connect(self.as_sock(), addr)
    }

    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        set_nonblocking(self.as_sock(), nonblocking)
    }
}

fn do_connect<A: ToSocketAddrs>(sock: Socket, addr: A) -> io::Result<()> {
    let err = io::Error::new(io::ErrorKind::Other,
                             "no socket addresses resolved");
    let addrs = try!(addr.to_socket_addrs());
    let sys = sys::Socket::from_inner(sock);
    let sock = socket::Socket::from_inner(sys);
    let ret = addrs.fold(Err(err), |prev, addr| {
        prev.or_else(|_| sock.connect(&addr))
    });
    mem::forget(sock);
    return ret
}

#[cfg(unix)]
fn set_nonblocking(sock: Socket, nonblocking: bool) -> io::Result<()> {
    use libc::funcs::bsd44::ioctl;
    let mut nonblocking = nonblocking as libc::c_ulong;
    ::cvt(unsafe {
        ioctl(sock, FIONBIO, &mut nonblocking)
    }).map(|_| ())
}

#[cfg(windows)]
fn set_nonblocking(sock: Socket, nonblocking: bool) -> io::Result<()> {
    let mut nonblocking = nonblocking as libc::c_ulong;
    ::cvt(unsafe {
        libc::ioctlsocket(sock, FIONBIO, &mut nonblocking)
    })
}

fn ip2in_addr(ip: &Ipv4Addr) -> libc::in_addr {
    let oct = ip.octets();
    libc::in_addr {
        s_addr: ::hton(((oct[0] as u32) << 24) |
                       ((oct[1] as u32) << 16) |
                       ((oct[2] as u32) <<  8) |
                       ((oct[3] as u32) <<  0)),
    }
}

fn ip2in6_addr(ip: &Ipv6Addr) -> libc::in6_addr {
    let seg = ip.segments();
    libc::in6_addr {
        s6_addr: [
            ::hton(seg[0]),
            ::hton(seg[1]),
            ::hton(seg[2]),
            ::hton(seg[3]),
            ::hton(seg[4]),
            ::hton(seg[5]),
            ::hton(seg[6]),
            ::hton(seg[7]),
        ],
    }
}

impl TcpListenerExt for TcpListener {
    fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        setopt(self.as_sock(), libc::IPPROTO_IP, libc::IP_TTL, ttl as c_int)
    }

    fn ttl(&self) -> io::Result<u32> {
        getopt::<c_int>(self.as_sock(), libc::IPPROTO_IP, libc::IP_TTL)
            .map(|b| b as u32)
    }

    fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        setopt(self.as_sock(), libc::IPPROTO_IPV6, IPV6_V6ONLY, only_v6 as c_int)
    }

    fn only_v6(&self) -> io::Result<bool> {
        getopt(self.as_sock(), libc::IPPROTO_IPV6, IPV6_V6ONLY).map(int2bool)
    }

    fn take_error(&self) -> io::Result<Option<io::Error>> {
        getopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_ERROR).map(int2err)
    }

    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        set_nonblocking(self.as_sock(), nonblocking)
    }
}

impl TcpBuilder {
    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This is the same as [`TcpStreamExt::set_ttl`][other].
    ///
    /// [other]: trait.TcpStreamExt.html#tymethod.set_ttl
    pub fn ttl(&self, ttl: u32) -> io::Result<&Self> {
        setopt(self.as_sock(), libc::IPPROTO_IP, libc::IP_TTL, ttl as c_int)
            .map(|()| self)
    }

    /// Sets the value for the `IPV6_V6ONLY` option on this socket.
    ///
    /// This is the same as [`TcpStreamExt::set_only_v6`][other].
    ///
    /// [other]: trait.TcpStreamExt.html#tymethod.set_only_v6
    pub fn only_v6(&self, only_v6: bool) -> io::Result<&Self> {
        setopt(self.as_sock(), libc::IPPROTO_IPV6, IPV6_V6ONLY, only_v6 as c_int)
            .map(|()| self)
    }

    /// Set value for the `SO_REUSEADDR` option on this socket.
    ///
    /// This indicates that futher calls to `bind` may allow reuse of local
    /// addresses. For IPv4 sockets this means that a socket may bind even when
    /// there's a socket already listening on this port.
    pub fn reuse_address(&self, reuse: bool) -> io::Result<&Self> {
        setopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_REUSEADDR,
               reuse as c_int).map(|()| self)
    }

    /// Get the value of the `SO_ERROR` option on this socket.
    ///
    /// This will retrieve the stored error in the underlying socket, clearing
    /// the field in the process. This can be useful for checking errors between
    /// calls.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        getopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_ERROR).map(int2err)
    }
}

impl UdpBuilder {
    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This is the same as [`TcpStreamExt::set_ttl`][other].
    ///
    /// [other]: trait.TcpStreamExt.html#tymethod.set_ttl
    pub fn ttl(&self, ttl: u32) -> io::Result<&Self> {
        setopt(self.as_sock(), libc::IPPROTO_IP, libc::IP_TTL, ttl as c_int)
            .map(|()| self)
    }

    /// Sets the value for the `IPV6_V6ONLY` option on this socket.
    ///
    /// This is the same as [`TcpStream::only_v6`][other].
    ///
    /// [other]: struct.TcpBuilder.html#method.only_v6
    pub fn only_v6(&self, only_v6: bool) -> io::Result<&Self> {
        setopt(self.as_sock(), libc::IPPROTO_IPV6, IPV6_V6ONLY, only_v6 as c_int)
            .map(|()| self)
    }

    /// Set value for the `SO_REUSEADDR` option on this socket.
    ///
    /// This is the same as [`TcpBuilder::reuse_address`][other].
    ///
    /// [other]: struct.TcpBuilder.html#method.reuse_address
    pub fn reuse_address(&self, reuse: bool) -> io::Result<&Self> {
        setopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_REUSEADDR,
               reuse as c_int).map(|()| self)
    }

    /// Get the value of the `SO_ERROR` option on this socket.
    ///
    /// This will retrieve the stored error in the underlying socket, clearing
    /// the field in the process. This can be useful for checking errors between
    /// calls.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        getopt(self.as_sock(), libc::SOL_SOCKET, libc::SO_ERROR).map(int2err)
    }
}
