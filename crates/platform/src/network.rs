//! Linux local-only socket effects through std and safe nix wrappers.
//! Registrations own no native fd. Wait temporarily borrows live socket owners;
//! the single driver must not concurrently wait and close its streams.
use nix::{
    errno::Errno,
    poll::{PollFd, PollFlags, ppoll},
    sys::socket::{self, AddressFamily, SockFlag, SockType, SockaddrIn, SockaddrIn6, UnixAddr},
};
use std::{
    fs::{self, DirBuilder},
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd},
        unix::{
            fs::{DirBuilderExt, FileTypeExt, MetadataExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use xadb_foundation::network::{
    ByteStream, Connector, Interest, Listener, ReadinessEvent, ReadinessWait, Ready, Registration,
};
const REGISTRATIONS: usize = 131;
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn poisoned() -> io::Error {
    io::Error::other("native registration lock poisoned")
}
fn closed() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "socket closed")
}

/// Newly created private directory capability. Existing directories are rejected.
/// Only listeners made through this capability may bind Unix endpoints here.
#[derive(Clone)]
pub struct UnixDirectory(Arc<DirectoryOwner>);
struct DirectoryOwner {
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl UnixDirectory {
    /// Atomically create a mode-0700 directory; never reuse or remove existing data.
    ///
    /// # Errors
    /// Returns creation/metadata errors. Caller owns the parent directory and must
    /// not rename/swap any ancestor while this capability is live.
    pub fn create(path: &Path) -> io::Result<Self> {
        DirBuilder::new().mode(0o700).create(path)?;
        let meta = fs::symlink_metadata(path)?;
        Ok(Self(Arc::new(DirectoryOwner {
            path: path.to_owned(),
            device: meta.dev(),
            inode: meta.ino(),
        })))
    }
    /// Directory path for fixture identity; not a capability for arbitrary cleanup.
    pub fn path(&self) -> &Path {
        &self.0.path
    }
    fn check(&self) -> io::Result<()> {
        let meta = fs::symlink_metadata(&self.0.path)?;
        if !meta.is_dir()
            || meta.dev() != self.0.device
            || meta.ino() != self.0.inode
            || meta.mode() & 0o077 != 0
        {
            return Err(invalid("Unix directory identity or permissions changed"));
        }
        Ok(())
    }
}
impl Drop for DirectoryOwner {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.is_dir() && m.dev() == self.device && m.ino() == self.inode)
        {
            let _ = fs::remove_dir(&self.path); // Only succeeds when empty; never recursive.
        }
    }
}
/// Validated numeric loopback or owned Unix endpoint. No wildcard, DNS or remote API.
#[derive(Clone)]
pub struct LocalEndpoint(Endpoint);
#[derive(Clone)]
enum Endpoint {
    Tcp(SocketAddr),
    Unix {
        directory: UnixDirectory,
        path: PathBuf,
    },
}
impl LocalEndpoint {
    /// Validate a numeric IPv4/IPv6 loopback address. Port zero is useful for bind.
    ///
    /// # Errors
    /// Rejects wildcard/non-loopback addresses, including IPv4-mapped remote IPv6.
    pub fn tcp(address: SocketAddr) -> io::Result<Self> {
        if !address.ip().is_loopback() {
            return Err(invalid("C1 requires numeric loopback"));
        }
        Ok(Self(Endpoint::Tcp(address)))
    }
    /// Name one direct child of a newly owned directory.
    ///
    /// # Errors
    /// Rejects empty, dot, parent, slash, NUL, and replaced directory identities.
    pub fn unix(directory: &UnixDirectory, name: &str) -> io::Result<Self> {
        directory.check()?;
        if name.is_empty()
            || matches!(name, "." | "..")
            || name.contains('/')
            || name.contains('\0')
        {
            return Err(invalid("Unix endpoint must be a direct filename"));
        }
        Ok(Self(Endpoint::Unix {
            directory: directory.clone(),
            path: directory.path().join(name),
        }))
    }
    /// Numeric bound address, if TCP.
    pub fn socket_addr(&self) -> Option<SocketAddr> {
        if let Endpoint::Tcp(a) = self.0 {
            Some(a)
        } else {
            None
        }
    }
    /// Owned pathname, if Unix.
    pub fn unix_path(&self) -> Option<&Path> {
        if let Endpoint::Unix { path, .. } = &self.0 {
            Some(path)
        } else {
            None
        }
    }
}
enum Socket {
    Tcp(TcpStream),
    Unix(UnixStream),
    TcpListener(TcpListener),
    UnixListener(UnixListener),
}
impl AsFd for Socket {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match self {
            Self::Tcp(s) => s.as_fd(),
            Self::Unix(s) => s.as_fd(),
            Self::TcpListener(s) => s.as_fd(),
            Self::UnixListener(s) => s.as_fd(),
        }
    }
}
struct Registry {
    sockets: [Weak<Socket>; REGISTRATIONS],
    generations: [u64; REGISTRATIONS],
}
/// One bounded native registration domain. Clones share identity/lifetime state.
/// Use one driver owner; this adapter does not implement a multi-reactor protocol.
#[derive(Clone)]
pub struct LocalNetwork(Arc<Mutex<Registry>>);
impl Default for LocalNetwork {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Registry {
            sockets: std::array::from_fn(|_| Weak::new()),
            generations: [0; REGISTRATIONS],
        })))
    }
}
struct Handle {
    network: LocalNetwork,
    registration: Registration,
    socket: Option<Arc<Socket>>,
}
impl Handle {
    fn socket(&self) -> io::Result<&Socket> {
        self.socket.as_deref().ok_or_else(closed)
    }
    fn close(&mut self) {
        let mut registry = self
            .network
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let slot = self.registration.slot as usize;
        if registry.generations[slot] == self.registration.generation {
            registry.sockets[slot] = Weak::new();
        }
        self.socket = None;
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.close();
    }
}
impl LocalNetwork {
    fn register(&self, socket: Socket) -> io::Result<Handle> {
        let socket = Arc::new(socket);
        let mut registry = self.0.lock().map_err(|_| poisoned())?;
        let slot = (0..REGISTRATIONS)
            .find(|i| {
                registry.sockets[*i].strong_count() == 0 && registry.generations[*i] < u64::MAX
            })
            .ok_or_else(|| io::Error::other("native registration capacity exhausted"))?;
        registry.generations[slot] += 1;
        registry.sockets[slot] = Arc::downgrade(&socket);
        let registration = Registration {
            slot: slot as u32,
            generation: registry.generations[slot],
        };
        Ok(Handle {
            network: self.clone(),
            registration,
            socket: Some(socket),
        })
    }
    /// Build a connector without performing an effect; transport reserves first.
    pub fn connector(&self, endpoint: LocalEndpoint) -> LocalConnector {
        LocalConnector {
            network: self.clone(),
            endpoint,
        }
    }
    /// Bind a nonblocking local listener. Existing Unix objects are never unlinked.
    ///
    /// # Errors
    /// Preserves bind, identity, permission, and registration-limit errors.
    pub fn listen(&self, endpoint: LocalEndpoint) -> io::Result<LocalListener> {
        let (socket, endpoint, owned) = match &endpoint.0 {
            Endpoint::Tcp(address) => {
                let listener = TcpListener::bind(address)?;
                listener.set_nonblocking(true)?;
                let bound = LocalEndpoint::tcp(listener.local_addr()?)?;
                (Socket::TcpListener(listener), bound, None)
            }
            Endpoint::Unix { directory, path } => {
                directory.check()?;
                match fs::symlink_metadata(path) {
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "Unix endpoint exists",
                        ));
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e),
                }
                let listener = UnixListener::bind(path)?;
                let meta = fs::symlink_metadata(path)?;
                let owned = OwnedPath {
                    directory: directory.clone(),
                    path: path.clone(),
                    device: meta.dev(),
                    inode: meta.ino(),
                };
                listener.set_nonblocking(true)?;
                (
                    Socket::UnixListener(listener),
                    endpoint.clone(),
                    Some(owned),
                )
            }
        };
        let handle = self.register(socket)?;
        Ok(LocalListener {
            handle,
            endpoint,
            owned,
        })
    }
}
struct OwnedPath {
    directory: UnixDirectory,
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl Drop for OwnedPath {
    fn drop(&mut self) {
        if self.directory.check().is_ok()
            && fs::symlink_metadata(&self.path).is_ok_and(|m| {
                m.file_type().is_socket() && m.dev() == self.device && m.ino() == self.inode
            })
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}
/// Configured nonblocking connector. Unix connect also requires a socket within
/// its private directory; symlink/regular-file endpoints are rejected.
pub struct LocalConnector {
    network: LocalNetwork,
    endpoint: LocalEndpoint,
}
impl Connector for LocalConnector {
    fn connect(&mut self) -> io::Result<Box<dyn ByteStream>> {
        let flags = SockFlag::SOCK_NONBLOCK | SockFlag::SOCK_CLOEXEC;
        let socket = match &self.endpoint.0 {
            Endpoint::Tcp(address) => {
                if address.port() == 0 {
                    return Err(invalid("cannot connect to port zero"));
                }
                let fd = socket::socket(
                    if address.is_ipv4() {
                        AddressFamily::Inet
                    } else {
                        AddressFamily::Inet6
                    },
                    SockType::Stream,
                    flags,
                    None,
                )?;
                let result = match address {
                    SocketAddr::V4(a) => socket::connect(fd.as_raw_fd(), &SockaddrIn::from(*a)),
                    SocketAddr::V6(a) => socket::connect(fd.as_raw_fd(), &SockaddrIn6::from(*a)),
                };
                match result {
                    Ok(()) | Err(Errno::EINPROGRESS) => (),
                    Err(e) => return Err(e.into()),
                }
                Socket::Tcp(TcpStream::from(fd))
            }
            Endpoint::Unix { directory, path } => {
                directory.check()?;
                if !fs::symlink_metadata(path)?.file_type().is_socket() {
                    return Err(invalid("owned endpoint is not a socket"));
                }
                let fd = socket::socket(AddressFamily::Unix, SockType::Stream, flags, None)?;
                match socket::connect(fd.as_raw_fd(), &UnixAddr::new(path)?) {
                    Ok(()) | Err(Errno::EINPROGRESS) => (),
                    Err(e) => return Err(e.into()),
                }
                Socket::Unix(UnixStream::from(fd))
            }
        };
        Ok(Box::new(LocalStream(self.network.register(socket)?)))
    }
}
struct LocalStream(Handle);
impl ByteStream for LocalStream {
    fn registration(&self) -> Registration {
        self.0.registration
    }
    fn try_read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        match self.0.socket()? {
            Socket::Tcp(s) => (&*s).read(output),
            Socket::Unix(s) => (&*s).read(output),
            _ => Err(invalid("not a stream")),
        }
    }
    fn try_write(&mut self, input: &[u8]) -> io::Result<usize> {
        match self.0.socket()? {
            Socket::Tcp(s) => (&*s).write(input),
            Socket::Unix(s) => (&*s).write(input),
            _ => Err(invalid("not a stream")),
        }
    }
    fn finish_connect(&mut self) -> io::Result<()> {
        if let Some(e) = self.take_error()? {
            return Err(e);
        }
        let result = match self.0.socket()? {
            Socket::Tcp(s) => s.peer_addr().map(|_| ()),
            Socket::Unix(s) => s.peer_addr().map(|_| ()),
            _ => Err(invalid("not a stream")),
        };
        result.map_err(|e| {
            if e.kind() == io::ErrorKind::NotConnected {
                io::Error::from(io::ErrorKind::WouldBlock)
            } else {
                e
            }
        })
    }
    fn take_error(&mut self) -> io::Result<Option<io::Error>> {
        match self.0.socket()? {
            Socket::Tcp(s) => s.take_error(),
            Socket::Unix(s) => s.take_error(),
            _ => Err(invalid("not a stream")),
        }
    }
    fn close(&mut self) {
        self.0.close();
    }
}
/// Native listener with pathname identity guard. Close is idempotent and removes
/// only the original socket inode; replacement files/symlinks are preserved.
pub struct LocalListener {
    handle: Handle,
    endpoint: LocalEndpoint,
    owned: Option<OwnedPath>,
}
impl LocalListener {
    /// Actual address after bind, including the OS-assigned TCP port.
    pub fn endpoint(&self) -> &LocalEndpoint {
        &self.endpoint
    }
}
impl Listener for LocalListener {
    fn registration(&self) -> Registration {
        self.handle.registration
    }
    fn try_accept(&mut self) -> io::Result<Box<dyn ByteStream>> {
        let socket = match self.handle.socket()? {
            Socket::TcpListener(l) => {
                let (s, a) = l.accept()?;
                if !a.ip().is_loopback() {
                    return Err(invalid("peer is not loopback"));
                }
                s.set_nonblocking(true)?;
                Socket::Tcp(s)
            }
            Socket::UnixListener(l) => {
                let (s, _) = l.accept()?;
                s.set_nonblocking(true)?;
                Socket::Unix(s)
            }
            _ => return Err(invalid("not a listener")),
        };
        Ok(Box::new(LocalStream(self.handle.network.register(socket)?)))
    }
    fn close(&mut self) {
        self.handle.close();
        self.owned = None;
    }
}
impl Drop for LocalListener {
    fn drop(&mut self) {
        self.close();
    }
}
impl ReadinessWait for LocalNetwork {
    fn wait(
        &mut self,
        interests: &[Interest],
        timeout: Duration,
        output: &mut [ReadinessEvent],
    ) -> io::Result<usize> {
        if interests.len() > REGISTRATIONS
            || output.len() < interests.len()
            || timeout > Duration::from_millis(20)
        {
            return Err(invalid("native wait bounds"));
        }
        let mut sockets: [Option<Arc<Socket>>; REGISTRATIONS] = std::array::from_fn(|_| None);
        {
            let registry = self.0.lock().map_err(|_| poisoned())?;
            for (i, interest) in interests.iter().enumerate() {
                if !interest.readable && !interest.writable {
                    return Err(invalid("empty interest must be omitted"));
                }
                let slot = interest.registration.slot as usize;
                if registry.generations.get(slot) == Some(&interest.registration.generation) {
                    sockets[i] = registry.sockets[slot].upgrade();
                }
            }
        }
        // Vec metadata is bounded by 131; no registry/lifecycle lock during poll.
        let mut polls = Vec::new();
        polls
            .try_reserve_exact(interests.len())
            .map_err(|_| io::Error::other("poll allocation"))?;
        let mut indices = [0; REGISTRATIONS];
        let mut count = 0;
        for (i, interest) in interests.iter().enumerate() {
            if let Some(socket) = &sockets[i] {
                let mut flags = PollFlags::empty();
                if interest.readable {
                    flags |= PollFlags::POLLIN;
                }
                if interest.writable {
                    flags |= PollFlags::POLLOUT;
                }
                indices[polls.len()] = i;
                polls.push(PollFd::new(socket.as_fd(), flags));
            } else {
                output[count] = ReadinessEvent {
                    registration: interest.registration,
                    ready: Ready {
                        invalid: true,
                        ..Ready::default()
                    },
                };
                count += 1;
            }
        }
        let duration = if count > 0 { Duration::ZERO } else { timeout };
        ppoll(&mut polls, Some(duration.into()), None)?;
        for (i, fd) in polls.iter().enumerate() {
            let flags = fd.revents().unwrap_or(PollFlags::POLLNVAL);
            if flags.is_empty() {
                continue;
            }
            output[count] = ReadinessEvent {
                registration: interests[indices[i]].registration,
                ready: Ready {
                    readable: flags.contains(PollFlags::POLLIN),
                    writable: flags.contains(PollFlags::POLLOUT),
                    hangup: flags.contains(PollFlags::POLLHUP),
                    error: flags.contains(PollFlags::POLLERR),
                    invalid: flags.contains(PollFlags::POLLNVAL),
                },
            };
            count += 1;
        }
        Ok(count)
    }
}
