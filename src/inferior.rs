use std::collections::HashMap;
use std::mem::size_of;
use std::os::unix::process::CommandExt;
use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::process::{Child, Command};
use nix::sys::signal::Signal;
use crate::dwarf_data::{DwarfData, Line};

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

pub struct Inferior {
    child: Child,
    breakpoint_map: HashMap<usize, u8>,
}

fn align_addr_to_word(addr: usize) -> usize {
    addr & (-(size_of::<usize>() as isize) as usize)
}

impl Inferior {
    fn write_byte(pid: Pid, addr: usize, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(pid, aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        unsafe {
            ptrace::write(
                pid,
                aligned_addr as ptrace::AddressType,
                updated_word as *mut std::ffi::c_void,
            )?;
        }
        Ok(orig_byte as u8)
    }
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>, breakpoints: &Vec<usize>) -> Option<Inferior> {
        let mut cmd = Command::new(target);
        cmd.args(args);
        unsafe {
            cmd.pre_exec(child_traceme);
        }
        let child = cmd.spawn().ok()?;
        let mut inferior = Inferior { child, breakpoint_map: HashMap::new() };
        let stat = inferior.wait(None).ok()?;
        if let Status::Stopped(Signal::SIGTRAP, _) = stat {
            breakpoints.iter().for_each(|&x| {
                inferior.breakpoint_map.insert(x, Inferior::write_byte(inferior.pid(), x, 0xccu8).ok().unwrap());
            })
        }
        else { return None }
        Some(inferior)
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }

    pub fn install_breakpoints(&mut self, breakpoints: &Vec::<usize>) {
        breakpoints.iter().for_each(|&x| {
            self.breakpoint_map.insert(x, Inferior::write_byte(self.pid(), x, 0xccu8).ok().unwrap());
        })
    }

    pub fn go(&self) -> Result<Status, nix::Error> {
        let mut regs = ptrace::getregs(self.pid())?;

        // if it is a call after a breakpoint (%rip = breakpoint addr + 1),
        // restore the original byte and rewind program execution.
        // Then execute THIS instruction, stop, and write 0xcc INT instruction back to addr.
        if let Some((&addr, &byte)) = self.breakpoint_map.get_key_value(&((regs.rip - 1) as usize)) {
            Inferior::write_byte(self.pid(), addr, byte)?;
            regs.rip = regs.rip - 1;
            ptrace::setregs(self.pid(), regs)?;

            ptrace::step(self.pid(), None)?;
            match self.wait(None) {
                Ok(Status::Stopped(Signal::SIGTRAP, _)) => {
                    Inferior::write_byte(self.pid(), addr, 0xccu8)?;
                },
                _ => {},
                // no need to handle Exited since next cont is called.
            }
        }
        ptrace::cont(self.pid(), None)?;
        self.wait(None)
    }

    pub fn kill(&mut self) -> Result<Status, nix::Error> {
        if let Err(err) = self.child.kill() {
            println!("command cannot be killed.");
            return Err(nix::Error::from_i32(err.raw_os_error().unwrap()));
        }
        self.wait(None)
    }

    pub fn print_backtrace(&self, debug_data: &DwarfData) -> Result<(), nix::Error> {
        let regs = ptrace::getregs(self.pid())?;
        let mut rip = regs.rip as usize;
        let mut rbp = regs.rbp as usize;
        loop {
            let func = debug_data.get_function_from_addr(rip)
                .unwrap_or(String::from("Unrecognized function"));
            let line = debug_data.get_line_from_addr(rip)
                .unwrap_or(Line {file: String::from("Unrecognized file"), number: 0, address: 0 });
            println!("{} ({})", func, line);
            if func == "main" { break }
            rip = ptrace::read(self.pid(), (rbp + 8) as ptrace::AddressType)? as usize;
            rbp = ptrace::read(self.pid(), rbp as ptrace::AddressType)? as usize;
        }
        Ok(())
    }
}
