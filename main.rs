// AUTOGENESIS - Self-Evolving P2P In-Memory JIT Compiler
// Compile: rustc main.rs -O -o autogenesis
// Run: ./autogenesis 8080
// WARNING: Heavy unsafe code. Expect segfaults during aggressive mutation.

use std::net::{UdpSocket, SocketAddr};
use std::thread;
use std::time::{Duration, Instant};
use std::ptr;

// --- RAW FFI FOR EXECUTABLE MEMORY ---

#[cfg(unix)]
mod sys {
    use std::os::raw::{c_int, c_void};
    pub const PROT_READ: c_int = 1;
    pub const PROT_WRITE: c_int = 2;
    pub const PROT_EXEC: c_int = 4;
    pub const MAP_PRIVATE: c_int = 0x02;
    pub const MAP_ANONYMOUS: c_int = 0x20;

    extern "C" {
        pub fn mmap(addr: *mut c_void, len: usize, prot: c_int, flags: c_int, fd: c_int, offset: i64) -> *mut c_void;
    }
}

#[cfg(windows)]
mod sys {
    use std::os::raw::c_void;
    pub const MEM_COMMIT: u32 = 0x1000;
    pub const MEM_RESERVE: u32 = 0x2000;
    pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;

    extern "system" {
        pub fn VirtualAlloc(lpAddress: *mut c_void, dwSize: usize, flAllocationType: u32, flProtect: u32) -> *mut c_void;
    }
}

fn alloc_executable_memory(size: usize) -> *mut u8 {
    unsafe {
        #[cfg(unix)]
        {
            let ptr = sys::mmap(
                ptr::null_mut(),
                size,
                sys::PROT_READ | sys::PROT_WRITE | sys::PROT_EXEC,
                sys::MAP_PRIVATE | sys::MAP_ANONYMOUS,
                -1,
                0,
            );
            if ptr == usize::MAX as *mut _ { panic!("mmap failed"); }
            ptr as *mut u8
        }
        #[cfg(windows)]
        {
            let ptr = sys::VirtualAlloc(
                ptr::null_mut(),
                size,
                sys::MEM_COMMIT | sys::MEM_RESERVE,
                sys::PAGE_EXECUTE_READWRITE,
            );
            if ptr.is_null() { panic!("VirtualAlloc failed"); }
            ptr as *mut u8
        }
    }
}

// --- MACHINE CODE GENETICS (x86_64) ---

struct JitGenome {
    code: Vec<u8>,
}

impl JitGenome {
    fn new_random() -> Self {
        // mov rax, 1; ret
        let code = vec![0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, 0xC3];
        Self { code }
    }

    fn mutate(&mut self) {
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as usize;
        
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        
        // Mutation: Insert ADD RAX, imm8 before RET
        if seed % 2 == 0 && self.code.len() < 64 {
            let insert_idx = self.code.len() - 1;
            let add_op = vec![0x48, 0x83, 0xC0, (seed % 10 + 1) as u8]; 
            self.code.splice(insert_idx..insert_idx, add_op);
        }
    }
}

// --- JIT EXECUTION & BENCHMARK ---

fn execute_jit(genome: &JitGenome) -> (u64, i64) {
    let size = genome.code.len();
    let mem = alloc_executable_memory(size);
    
    unsafe {
        ptr::copy_nonoverlapping(genome.code.as_ptr(), mem, size);
        let func: extern "C" fn() -> i64 = std::mem::transmute(mem);
        
        let start = Instant::now();
        let mut result = 0;
        for _ in 0..100_000 {
            result = func();
        }
        let duration = start.elapsed().as_nanos() as u64;
        
        (duration, result)
    }
}

// --- P2P UDP NETWORK ---

struct P2PNode {
    socket: UdpSocket,
    peers: Vec<SocketAddr>,
}

impl P2PNode {
    fn new(port: u16) -> Self {
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).expect("Failed to bind UDP");
        socket.set_nonblocking(true).unwrap();
        Self { socket, peers: vec![] }
    }

    fn add_peer(&mut self, addr: &str) {
        if let Ok(peer) = addr.parse() {
            self.peers.push(peer);
        }
    }

    fn broadcast_genome(&self, genome: &JitGenome) {
        for peer in &self.peers {
            let _ = self.socket.send_to(&genome.code, peer);
        }
    }

    fn receive_genomes(&self) -> Vec<JitGenome> {
        let mut buf = [0u8; 1024];
        let mut received = vec![];
        
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((amt, _)) => {
                    received.push(JitGenome { code: buf[..amt].to_vec() });
                }
                Err(_) => break,
            }
        }
        received
    }
}

// --- EVOLUTION LOOP ---

fn main() {
    let port = std::env::args().nth(1).unwrap_or_else(|| "8080".to_string()).parse().unwrap();
    let mut node = P2PNode::new(port);
    
    // Hardcoded peer discovery for local testing
    if port == 8080 { node.add_peer("127.0.0.1:8081"); }
    if port == 8081 { node.add_peer("127.0.0.1:8080"); }
    
    let mut best_genome = JitGenome::new_random();
    let (mut best_time, mut best_result) = execute_jit(&best_genome);
    
    println!("[INIT] Port: {} | Base Code: {:?}", port, best_genome.code);
    println!("[INIT] Time: {} ns | Result: {}", best_time, best_result);

    let mut generation = 0;
    
    loop {
        generation += 1;
        
        // 1. Local Mutation
        let mut candidate = JitGenome { code: best_genome.code.clone() };
        candidate.mutate();
        
        let (time, result) = execute_jit(&candidate);
        
        // 2. Selection
        if time < best_time && result > 0 {
            best_time = time;
            best_result = result;
            best_genome = candidate;
            println!("[EVO] Gen {} | NEW BEST! Time: {} ns | Code: {:?}", generation, best_time, best_genome.code);
            node.broadcast_genome(&best_genome);
        }
        
        // 3. Swarm Sync
        for fg in node.receive_genomes() {
            if !fg.code.is_empty() {
                let (f_time, f_result) = execute_jit(&fg);
                if f_time < best_time && f_result > 0 {
                    best_time = f_time;
                    best_result = f_result;
                    best_genome = fg;
                    println!("[P2P] Foreign Genome Accepted! Time: {} ns | Code: {:?}", best_time, best_genome.code);
                }
            }
        }
        
        thread::sleep(Duration::from_millis(10));
    }
}
