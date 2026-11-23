//! Experimental, opt-in capture through the Windows inbox Packet Monitor API.
//!
//! This module never starts/stops the global `pktmon.exe` session. It creates
//! an independent session, requires existing administrator rights and loads
//! only the Microsoft DLL in System32. Current API availability is probed at
//! runtime: Microsoft does not specify a minimum supported Windows client.
//!
//! The API's documented snapshot maximum is 9000 bytes. Ambiguous full-size
//! snapshots and packets whose IP length exceeds the snapshot are refused,
//! rather than fabricated into complete TCP segments. The OS ring sees the
//! session's packets; exact game endpoint filtering happens in the callback
//! before allocating or queueing retained bytes. Until elevated IPv4/IPv6 and
//! loopback tests pass, this is not a replacement for the default backend.

use std::collections::BTreeSet;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::Duration;

pub const SNAPSHOT_BYTES: usize = 9000;
const QUEUED_PACKETS: usize = 128;
const MAX_DESCRIPTOR_BYTES: usize = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    /// Exact remote (or local proxy) endpoints obtained from the game's sockets.
    pub endpoints: BTreeSet<(IpAddr, u16)>,
    /// Existing explicit wide-capture preference; never inferred from no sockets.
    pub allow_all_tcp: bool,
}

impl Filter {
    fn validate(&self) -> Result<(), Error> {
        if !self.allow_all_tcp && self.endpoints.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidScope,
                "No game TCP endpoints are known",
                None,
            ));
        }
        if self.endpoints.iter().any(|(_, port)| *port == 0) {
            return Err(Error::new(
                ErrorKind::InvalidScope,
                "A capture endpoint has port zero",
                None,
            ));
        }
        Ok(())
    }

    fn matches(&self, source: (IpAddr, u16), destination: (IpAddr, u16)) -> bool {
        self.allow_all_tcp
            || self.endpoints.contains(&source)
            || self.endpoints.contains(&destination)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Unsupported,
    AccessDenied,
    InvalidScope,
    Native,
    StreamClosed,
}

#[derive(Clone, Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub operation: String,
    pub hresult: Option<u32>,
}

impl Error {
    fn new(kind: ErrorKind, operation: &str, hresult: Option<u32>) -> Self {
        Self {
            kind,
            operation: operation.into(),
            hresult,
        }
    }
    #[cfg(windows)]
    fn native(operation: &str, hr: i32) -> Self {
        let kind = match hr as u32 {
            0x80070005 => ErrorKind::AccessDenied,
            0x80004001 | 0x80070032 | 0x8007051a => ErrorKind::Unsupported,
            _ => ErrorKind::Native,
        };
        Self::new(kind, operation, Some(hr as u32))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.operation)?;
        if let Some(hr) = self.hresult {
            write!(f, " (0x{hr:08X})")?;
        }
        if self.kind == ErrorKind::AccessDenied {
            write!(f, "; Packet Monitor requires administrator privileges")?;
        }
        Ok(())
    }
}
impl std::error::Error for Error {}

#[derive(Debug)]
pub struct Packet {
    pub data: Vec<u8>,
    /// 1 = Ethernet, 12 = raw IP, matching the capture loop's framing values.
    pub framing: i32,
    pub whole: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Diagnostics {
    pub observed: u64,
    pub accepted: u64,
    pub filtered: u64,
    pub truncated: u64,
    pub malformed: u64,
    pub unsupported: u64,
    pub queue_full: u64,
    pub missed_write: u64,
    pub missed_read: u64,
    pub stream_events: u64,
}

#[derive(Default)]
struct Counters {
    observed: AtomicU64,
    accepted: AtomicU64,
    filtered: AtomicU64,
    truncated: AtomicU64,
    malformed: AtomicU64,
    unsupported: AtomicU64,
    queue_full: AtomicU64,
    missed_write: AtomicU64,
    missed_read: AtomicU64,
    stream_events: AtomicU64,
}

impl Counters {
    fn snapshot(&self) -> Diagnostics {
        Diagnostics {
            observed: self.observed.load(Ordering::Relaxed),
            accepted: self.accepted.load(Ordering::Relaxed),
            filtered: self.filtered.load(Ordering::Relaxed),
            truncated: self.truncated.load(Ordering::Relaxed),
            malformed: self.malformed.load(Ordering::Relaxed),
            unsupported: self.unsupported.load(Ordering::Relaxed),
            queue_full: self.queue_full.load(Ordering::Relaxed),
            missed_write: self.missed_write.load(Ordering::Relaxed),
            missed_read: self.missed_read.load(Ordering::Relaxed),
            stream_events: self.stream_events.load(Ordering::Relaxed),
        }
    }
}

struct Context {
    filter: Filter,
    sender: SyncSender<Packet>,
    counters: Arc<Counters>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rejection {
    Malformed,
    Truncated,
    Unsupported,
    Filtered,
}

fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

/// Validate framing and find exact TCP endpoints without retaining payload.
fn classify(bytes: &[u8], packet_type: u16, filter: &Filter) -> Result<i32, Rejection> {
    if bytes.len() >= SNAPSHOT_BYTES {
        return Err(Rejection::Truncated);
    }
    let (ip, framing) = match packet_type {
        1 => {
            let mut at = 12;
            let mut ip = None;
            for _ in 0..3 {
                match be16(bytes, at).ok_or(Rejection::Malformed)? {
                    0x8100 | 0x88a8 | 0x9100 => at += 4,
                    0x0800 | 0x86dd => {
                        ip = Some(at + 2);
                        break;
                    }
                    _ => return Err(Rejection::Unsupported),
                }
            }
            (ip.ok_or(Rejection::Unsupported)?, 1)
        }
        3 => (0, 12),
        _ => return Err(Rejection::Unsupported),
    };
    let available = bytes.len().checked_sub(ip).ok_or(Rejection::Malformed)?;
    let version = bytes.get(ip).ok_or(Rejection::Malformed)? >> 4;
    let (source, destination, tcp, end) = match version {
        4 => {
            let ihl = ((*bytes.get(ip).ok_or(Rejection::Malformed)? & 15) as usize) * 4;
            if ihl < 20 || available < ihl {
                return Err(Rejection::Malformed);
            }
            let declared = be16(bytes, ip + 2).ok_or(Rejection::Malformed)? as usize;
            if declared > available {
                return Err(Rejection::Truncated);
            }
            if declared != 0 && declared < ihl + 20 {
                return Err(Rejection::Malformed);
            }
            // No IP defragmentation is guessed here. TCP recovery consumes full segments.
            if be16(bytes, ip + 6).ok_or(Rejection::Malformed)? & 0x3fff != 0 {
                return Err(Rejection::Unsupported);
            }
            if *bytes.get(ip + 9).ok_or(Rejection::Malformed)? != 6 {
                return Err(Rejection::Unsupported);
            }
            let source = IpAddr::V4(Ipv4Addr::from(
                <[u8; 4]>::try_from(bytes.get(ip + 12..ip + 16).ok_or(Rejection::Malformed)?)
                    .unwrap(),
            ));
            let destination = IpAddr::V4(Ipv4Addr::from(
                <[u8; 4]>::try_from(bytes.get(ip + 16..ip + 20).ok_or(Rejection::Malformed)?)
                    .unwrap(),
            ));
            // A complete, below-cap offload buffer may have a zero length field;
            // the existing capture-loop normalizer handles that later.
            (
                source,
                destination,
                ip + ihl,
                ip + if declared == 0 { available } else { declared },
            )
        }
        6 => {
            if available < 40 {
                return Err(Rejection::Malformed);
            }
            let payload = be16(bytes, ip + 4).ok_or(Rejection::Malformed)? as usize;
            let declared = if payload == 0 {
                available
            } else {
                payload + 40
            };
            if declared > available {
                return Err(Rejection::Truncated);
            }
            let source = IpAddr::V6(Ipv6Addr::from(
                <[u8; 16]>::try_from(bytes.get(ip + 8..ip + 24).ok_or(Rejection::Malformed)?)
                    .unwrap(),
            ));
            let destination = IpAddr::V6(Ipv6Addr::from(
                <[u8; 16]>::try_from(bytes.get(ip + 24..ip + 40).ok_or(Rejection::Malformed)?)
                    .unwrap(),
            ));
            let mut protocol = *bytes.get(ip + 6).ok_or(Rejection::Malformed)?;
            let mut at = ip + 40;
            for _ in 0..8 {
                if protocol == 6 {
                    break;
                }
                let length = match protocol {
                    0 | 43 | 60 => {
                        (*bytes.get(at + 1).ok_or(Rejection::Malformed)? as usize + 1) * 8
                    }
                    51 => (*bytes.get(at + 1).ok_or(Rejection::Malformed)? as usize + 2) * 4,
                    _ => return Err(Rejection::Unsupported),
                };
                protocol = *bytes.get(at).ok_or(Rejection::Malformed)?;
                at = at.checked_add(length).ok_or(Rejection::Malformed)?;
                if at > ip + declared {
                    return Err(Rejection::Malformed);
                }
            }
            if protocol != 6 {
                return Err(Rejection::Unsupported);
            }
            (source, destination, at, ip + declared)
        }
        _ => return Err(Rejection::Unsupported),
    };
    if tcp + 20 > end {
        return Err(Rejection::Malformed);
    }
    let tcp_length = ((*bytes.get(tcp + 12).ok_or(Rejection::Malformed)? >> 4) as usize) * 4;
    if tcp_length < 20 || tcp + tcp_length > end {
        return Err(Rejection::Malformed);
    }
    let source_port = be16(bytes, tcp).ok_or(Rejection::Malformed)?;
    let destination_port = be16(bytes, tcp + 2).ok_or(Rejection::Malformed)?;
    if !filter.matches((source, source_port), (destination, destination_port)) {
        return Err(Rejection::Filtered);
    }
    Ok(framing)
}

impl Context {
    fn packet(&self, bytes: &[u8], packet_type: u16) {
        self.counters.observed.fetch_add(1, Ordering::Relaxed);
        let framing = match classify(bytes, packet_type, &self.filter) {
            Ok(framing) => framing,
            Err(rejection) => {
                let counter = match rejection {
                    Rejection::Malformed => &self.counters.malformed,
                    Rejection::Truncated => &self.counters.truncated,
                    Rejection::Unsupported => &self.counters.unsupported,
                    Rejection::Filtered => &self.counters.filtered,
                };
                counter.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        match self.sender.try_send(Packet {
            data: bytes.to_vec(),
            framing,
            whole: true,
        }) {
            Ok(()) => {
                self.counters.accepted.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Full(_)) => {
                self.counters.queue_full.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::ffi::{c_void, CStr};
    use std::ptr;

    type Handle = *mut c_void;
    type HResult = i32;
    const API_VERSION: u32 = 0x0001_0000;

    // Layouts transcribed from Microsoft's documented API structures. No
    // guessed private driver IOCTL, constraint constant or unsupported ABI.
    #[repr(C)]
    struct Configuration {
        context: Handle,
        event_callback: Option<unsafe extern "system" fn(Handle, *const c_void, i32)>,
        data_callback: Option<unsafe extern "system" fn(Handle, *const Descriptor)>,
        buffer_multiplier: u16,
        truncation_size: u16,
    }
    #[repr(C)]
    struct Descriptor {
        data: *const u8,
        data_size: u32,
        metadata_offset: u32,
        packet_offset: u32,
        packet_length: u32,
        missed_write: u32,
        missed_read: u32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Metadata {
        group: u64,
        count: u16,
        appearance: u16,
        direction: u16,
        packet_type: u16,
        component: u16,
        edge: u16,
        reserved: u16,
        drop_reason: u32,
        drop_location: u32,
        processor: u16,
        timestamp: i64,
    }

    type Initialize = unsafe extern "system" fn(u32, Handle, *mut Handle) -> HResult;
    type Close = unsafe extern "system" fn(Handle);
    type CreateSession = unsafe extern "system" fn(Handle, *const u16, *mut Handle) -> HResult;
    type CreateStream =
        unsafe extern "system" fn(Handle, *const Configuration, *mut Handle) -> HResult;
    type Attach = unsafe extern "system" fn(Handle, Handle) -> HResult;
    type SetActive = unsafe extern "system" fn(Handle, u8) -> HResult;

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExW(name: *const u16, file: Handle, flags: u32) -> Handle;
        fn GetProcAddress(module: Handle, name: *const u8) -> Handle;
        fn FreeLibrary(module: Handle) -> i32;
    }

    struct Library(Handle);
    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }
    struct Api {
        _library: Library,
        initialize: Initialize,
        uninitialize: Close,
        create_session: CreateSession,
        close_session: Close,
        create_stream: CreateStream,
        close_stream: Close,
        attach: Attach,
        set_active: SetActive,
    }

    impl Api {
        fn load() -> Result<Self, Error> {
            let name = "PktMonApi.dll\0".encode_utf16().collect::<Vec<_>>();
            let handle = unsafe { LoadLibraryExW(name.as_ptr(), ptr::null_mut(), 0x0000_0800) };
            if handle.is_null() {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "System32 Packet Monitor API is unavailable",
                    None,
                ));
            }
            let library = Library(handle);
            unsafe fn function<T: Copy>(
                library: &Library,
                name: &'static [u8],
            ) -> Result<T, Error> {
                let address = GetProcAddress(library.0, name.as_ptr());
                if address.is_null() {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        &format!(
                            "Packet Monitor API does not export {}",
                            CStr::from_bytes_with_nul(name).unwrap().to_string_lossy()
                        ),
                        None,
                    ));
                }
                assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<Handle>());
                Ok(std::mem::transmute_copy(&address))
            }
            unsafe {
                Ok(Self {
                    initialize: function(&library, b"PacketMonitorInitialize\0")?,
                    uninitialize: function(&library, b"PacketMonitorUninitialize\0")?,
                    create_session: function(&library, b"PacketMonitorCreateLiveSession\0")?,
                    close_session: function(&library, b"PacketMonitorCloseSessionHandle\0")?,
                    create_stream: function(&library, b"PacketMonitorCreateRealtimeStream\0")?,
                    close_stream: function(&library, b"PacketMonitorCloseRealtimeStream\0")?,
                    attach: function(&library, b"PacketMonitorAttachOutputToSession\0")?,
                    set_active: function(&library, b"PacketMonitorSetSessionActive\0")?,
                    _library: library,
                })
            }
        }
    }

    fn checked_range(offset: u32, length: usize, total: u32) -> Option<std::ops::Range<usize>> {
        let start = offset as usize;
        let end = start.checked_add(length)?;
        (end <= total as usize).then_some(start..end)
    }

    unsafe extern "system" fn receive(context: Handle, descriptor: *const Descriptor) {
        if context.is_null() || descriptor.is_null() {
            return;
        }
        let context = &*(context as *const Context);
        // No panic, logging, blocking send, statistics parsing or user code is
        // allowed to unwind through the operating system's callback boundary.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let descriptor = &*descriptor;
            context
                .counters
                .missed_write
                .fetch_add(descriptor.missed_write as u64, Ordering::Relaxed);
            context
                .counters
                .missed_read
                .fetch_add(descriptor.missed_read as u64, Ordering::Relaxed);
            if descriptor.data.is_null() || descriptor.data_size as usize > MAX_DESCRIPTOR_BYTES {
                context.counters.malformed.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let metadata = checked_range(
                descriptor.metadata_offset,
                std::mem::size_of::<Metadata>(),
                descriptor.data_size,
            );
            let packet = checked_range(
                descriptor.packet_offset,
                descriptor.packet_length as usize,
                descriptor.data_size,
            );
            let (Some(metadata), Some(packet)) = (metadata, packet) else {
                context.counters.malformed.fetch_add(1, Ordering::Relaxed);
                return;
            };
            if metadata.start < packet.end && packet.start < metadata.end {
                context.counters.malformed.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let metadata =
                ptr::read_unaligned(descriptor.data.add(metadata.start) as *const Metadata);
            if metadata.drop_reason != 0 {
                return;
            }
            let bytes = std::slice::from_raw_parts(descriptor.data.add(packet.start), packet.len());
            context.packet(bytes, metadata.packet_type);
        }));
        if result.is_err() {
            context.counters.malformed.fetch_add(1, Ordering::Relaxed);
        }
    }

    unsafe extern "system" fn event(context: Handle, _information: *const c_void, _kind: i32) {
        if let Some(context) = (context as *const Context).as_ref() {
            context
                .counters
                .stream_events
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Checks DLL exports and permission without creating or activating a
    /// capture session. Never prompts for elevation or modifies global capture.
    pub fn probe() -> Result<(), Error> {
        let api = Api::load()?;
        let mut handle = ptr::null_mut();
        let hr = unsafe { (api.initialize)(API_VERSION, ptr::null_mut(), &mut handle) };
        if hr < 0 {
            return Err(Error::native("Cannot initialize Packet Monitor", hr));
        }
        if handle.is_null() {
            return Err(Error::new(
                ErrorKind::Native,
                "Packet Monitor returned no handle",
                None,
            ));
        }
        unsafe {
            (api.uninitialize)(handle);
        }
        Ok(())
    }

    pub struct Session {
        api: Api,
        handle: Handle,
        session: Handle,
        stream: Handle,
        active: bool,
        // The callback context outlives both stream and session shutdown.
        context: Box<Context>,
        receiver: Receiver<Packet>,
    }

    impl Session {
        pub fn open(filter: Filter) -> Result<Self, Error> {
            filter.validate()?;
            let api = Api::load()?;
            let (sender, receiver) = mpsc::sync_channel(QUEUED_PACKETS);
            let mut capture = Self {
                api,
                handle: ptr::null_mut(),
                session: ptr::null_mut(),
                stream: ptr::null_mut(),
                active: false,
                context: Box::new(Context {
                    filter,
                    sender,
                    counters: Arc::new(Counters::default()),
                }),
                receiver,
            };
            unsafe {
                let mut handle = ptr::null_mut();
                let hr = (capture.api.initialize)(API_VERSION, ptr::null_mut(), &mut handle);
                if hr < 0 {
                    return Err(Error::native("Cannot initialize Packet Monitor", hr));
                }
                if handle.is_null() {
                    return Err(Error::new(
                        ErrorKind::Native,
                        "Packet Monitor returned no handle",
                        None,
                    ));
                }
                capture.handle = handle;
                static SEQUENCE: AtomicU64 = AtomicU64::new(0);
                let name = format!(
                    "HS Tracker {} {}\0",
                    std::process::id(),
                    SEQUENCE.fetch_add(1, Ordering::Relaxed)
                )
                .encode_utf16()
                .collect::<Vec<_>>();
                let mut session = ptr::null_mut();
                let hr = (capture.api.create_session)(capture.handle, name.as_ptr(), &mut session);
                if hr < 0 {
                    return Err(Error::native(
                        "Cannot create private Packet Monitor session",
                        hr,
                    ));
                }
                if session.is_null() {
                    return Err(Error::new(
                        ErrorKind::Native,
                        "Packet Monitor returned no session",
                        None,
                    ));
                }
                capture.session = session;
                let configuration = Configuration {
                    context: (&mut *capture.context as *mut Context).cast(),
                    event_callback: Some(event),
                    data_callback: Some(receive),
                    buffer_multiplier: 4,
                    truncation_size: SNAPSHOT_BYTES as u16,
                };
                let mut stream = ptr::null_mut();
                let hr = (capture.api.create_stream)(capture.handle, &configuration, &mut stream);
                if hr < 0 {
                    return Err(Error::native("Cannot create Packet Monitor stream", hr));
                }
                if stream.is_null() {
                    return Err(Error::new(
                        ErrorKind::Native,
                        "Packet Monitor returned no stream",
                        None,
                    ));
                }
                capture.stream = stream;
                let hr = (capture.api.attach)(capture.session, capture.stream);
                if hr < 0 {
                    return Err(Error::native("Cannot attach Packet Monitor stream", hr));
                }
                let hr = (capture.api.set_active)(capture.session, 1);
                if hr < 0 {
                    return Err(Error::native("Cannot activate Packet Monitor session", hr));
                }
                capture.active = true;
            }
            Ok(capture)
        }
        pub fn next_packet(&self, timeout: Duration) -> Result<Option<Packet>, Error> {
            match self.receiver.recv_timeout(timeout) {
                Ok(packet) => Ok(Some(packet)),
                Err(RecvTimeoutError::Timeout) => Ok(None),
                Err(RecvTimeoutError::Disconnected) => Err(Error::new(
                    ErrorKind::StreamClosed,
                    "Packet Monitor stream ended",
                    None,
                )),
            }
        }
        pub fn diagnostics(&self) -> Diagnostics {
            self.context.counters.snapshot()
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                // SetSessionActive(false) synchronously stops monitoring. Closing
                // the stream detaches callbacks before the boxed context dies.
                if self.active && !self.session.is_null() {
                    (self.api.set_active)(self.session, 0);
                }
                if !self.stream.is_null() {
                    (self.api.close_stream)(self.stream);
                }
                if !self.session.is_null() {
                    (self.api.close_session)(self.session);
                }
                if !self.handle.is_null() {
                    (self.api.uninitialize)(self.handle);
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn documented_x64_ffi_layouts_and_descriptor_bounds_match() {
            if std::mem::size_of::<usize>() == 8 {
                assert_eq!(std::mem::size_of::<Configuration>(), 32);
                assert_eq!(std::mem::offset_of!(Configuration, truncation_size), 26);
                assert_eq!(std::mem::size_of::<Descriptor>(), 32);
                assert_eq!(std::mem::size_of::<Metadata>(), 48);
                assert_eq!(std::mem::offset_of!(Metadata, packet_type), 14);
                assert_eq!(std::mem::offset_of!(Metadata, timestamp), 40);
            }
            assert_eq!(checked_range(48, 20, 68), Some(48..68));
            assert_eq!(checked_range(u32::MAX, 5, 68), None);
            assert_eq!(checked_range(48, usize::MAX, 68), None);
        }
        #[test]
        fn malformed_native_descriptors_do_not_read_or_queue_unbounded_bytes() {
            let (sender, receiver) = mpsc::sync_channel(1);
            let mut context = Context {
                filter: Filter {
                    endpoints: BTreeSet::new(),
                    allow_all_tcp: true,
                },
                sender,
                counters: Arc::new(Counters::default()),
            };
            let mut descriptor = Descriptor {
                data: ptr::null(),
                data_size: u32::MAX,
                metadata_offset: 0,
                packet_offset: 0,
                packet_length: u32::MAX,
                missed_write: 2,
                missed_read: 3,
            };
            unsafe {
                receive((&mut context as *mut Context).cast(), &descriptor);
            }
            let buffer = [0u8; 64];
            descriptor.data = buffer.as_ptr();
            descriptor.data_size = 64;
            unsafe {
                receive((&mut context as *mut Context).cast(), &descriptor);
            }
            descriptor.packet_offset = 32;
            descriptor.packet_length = 8;
            unsafe {
                receive((&mut context as *mut Context).cast(), &descriptor);
            }
            assert!(receiver.try_recv().is_err());
            assert_eq!(context.counters.snapshot().malformed, 3);
        }
    }
}

#[cfg(windows)]
pub use windows::{probe, Session};

#[cfg(test)]
mod tests {
    use super::*;
    fn ipv4(source: [u8; 4], sport: u16, destination: [u8; 4], dport: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 44];
        bytes[0] = 0x45;
        bytes[2..4].copy_from_slice(&44u16.to_be_bytes());
        bytes[9] = 6;
        bytes[12..16].copy_from_slice(&source);
        bytes[16..20].copy_from_slice(&destination);
        bytes[20..22].copy_from_slice(&sport.to_be_bytes());
        bytes[22..24].copy_from_slice(&dport.to_be_bytes());
        bytes[32] = 0x50;
        bytes[40..].copy_from_slice(b"test");
        bytes
    }
    fn filter() -> Filter {
        Filter {
            endpoints: [("203.0.113.10".parse().unwrap(), 7000)]
                .into_iter()
                .collect(),
            allow_all_tcp: false,
        }
    }
    #[test]
    fn exact_endpoint_filter_accepts_both_directions_but_not_neighbors_or_other_ports() {
        let outgoing = ipv4([192, 0, 2, 5], 50000, [203, 0, 113, 10], 7000);
        let incoming = ipv4([203, 0, 113, 10], 7000, [192, 0, 2, 5], 50000);
        assert_eq!(classify(&outgoing, 3, &filter()), Ok(12));
        assert_eq!(classify(&incoming, 3, &filter()), Ok(12));
        assert_eq!(
            classify(
                &ipv4([203, 0, 113, 11], 7000, [192, 0, 2, 5], 50000),
                3,
                &filter()
            ),
            Err(Rejection::Filtered)
        );
        assert_eq!(
            classify(
                &ipv4([203, 0, 113, 10], 7001, [192, 0, 2, 5], 50000),
                3,
                &filter()
            ),
            Err(Rejection::Filtered)
        );
    }
    #[test]
    fn empty_scope_is_not_implicitly_a_machine_wide_capture() {
        assert_eq!(
            Filter {
                endpoints: BTreeSet::new(),
                allow_all_tcp: false
            }
            .validate()
            .unwrap_err()
            .kind,
            ErrorKind::InvalidScope
        );
        assert!(Filter {
            endpoints: BTreeSet::new(),
            allow_all_tcp: true
        }
        .validate()
        .is_ok());
    }
    #[test]
    fn ipv6_loopback_and_extension_headers_keep_full_tcp_endpoint_identity() {
        let mut bytes = vec![0u8; 68];
        bytes[0] = 0x60;
        bytes[4..6].copy_from_slice(&28u16.to_be_bytes());
        bytes[6] = 0;
        bytes[8..24].copy_from_slice(&Ipv6Addr::LOCALHOST.octets());
        bytes[24..40].copy_from_slice(&Ipv6Addr::LOCALHOST.octets());
        bytes[40] = 6;
        bytes[48..50].copy_from_slice(&50000u16.to_be_bytes());
        bytes[50..52].copy_from_slice(&7000u16.to_be_bytes());
        bytes[60] = 0x50;
        let filter = Filter {
            endpoints: [(IpAddr::V6(Ipv6Addr::LOCALHOST), 7000)]
                .into_iter()
                .collect(),
            allow_all_tcp: false,
        };
        assert_eq!(classify(&bytes, 3, &filter), Ok(12));
        bytes[6] = 44;
        assert_eq!(classify(&bytes, 3, &filter), Err(Rejection::Unsupported));
    }
    #[test]
    fn ethernet_and_vlan_use_the_capture_loops_existing_link_type() {
        let ip = ipv4([203, 0, 113, 10], 7000, [192, 0, 2, 5], 50000);
        let mut bytes = vec![0u8; 18];
        bytes[12..14].copy_from_slice(&0x8100u16.to_be_bytes());
        bytes[16..18].copy_from_slice(&0x0800u16.to_be_bytes());
        bytes.extend(ip);
        assert_eq!(classify(&bytes, 1, &filter()), Ok(1));
        assert_eq!(classify(&bytes, 2, &filter()), Err(Rejection::Unsupported));
    }
    #[test]
    fn truncated_offloaded_and_ambiguous_snapshots_are_not_fabricated_into_tcp() {
        let mut ip = ipv4([203, 0, 113, 10], 7000, [192, 0, 2, 5], 50000);
        ip[2..4].copy_from_slice(&100u16.to_be_bytes());
        assert_eq!(classify(&ip, 3, &filter()), Err(Rejection::Truncated));
        ip[2..4].copy_from_slice(&0u16.to_be_bytes());
        assert_eq!(classify(&ip, 3, &filter()), Ok(12));
        ip.resize(SNAPSHOT_BYTES, 0);
        assert_eq!(classify(&ip, 3, &filter()), Err(Rejection::Truncated));
    }
    #[test]
    fn callback_queue_is_bounded_and_unrelated_payloads_are_never_retained() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let context = Context {
            filter: filter(),
            sender,
            counters: Arc::new(Counters::default()),
        };
        let packet = ipv4([203, 0, 113, 10], 7000, [192, 0, 2, 5], 50000);
        context.packet(&packet, 3);
        context.packet(&packet, 3);
        context.packet(&ipv4([203, 0, 113, 11], 7000, [192, 0, 2, 5], 50000), 3);
        let kept = receiver.try_recv().unwrap();
        assert_eq!(kept.data, packet);
        assert!(kept.whole);
        assert!(receiver.try_recv().is_err());
        let diagnostics = context.counters.snapshot();
        assert_eq!(
            (
                diagnostics.observed,
                diagnostics.accepted,
                diagnostics.queue_full,
                diagnostics.filtered
            ),
            (3, 1, 1, 1)
        );
    }
}
