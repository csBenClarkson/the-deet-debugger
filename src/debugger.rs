use crate::debugger_command::DebuggerCommand;
use crate::inferior::{Inferior, Status};
use rustyline::error::ReadlineError;
use rustyline::Editor;
use rustyline::history::FileHistory;
use crate::dwarf_data::{DwarfData, Error as DwarfError};

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<(), FileHistory>,
    inferior: Option<Inferior>,
    debug_data: DwarfData,
    break_points: Vec<usize>,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str, print_info: bool) -> Debugger {
        let debug_data = match DwarfData::from_file(target) {
            Ok(val)=> val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not debugging symbols from {}: {:?}", target, err);
                std::process::exit(1);
            }
        };

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<(), FileHistory>::new().expect("Create editor fails.");
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        if print_info { debug_data.print(); }
        println!();

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            debug_data,
            break_points: Vec::new(),
        }
    }

    fn print_status(&self, status: Option<Status>) -> Option<Status> {
        match status {
            Some(Status::Exited(code)) => { println!("Child exited (status {})", code); return status; },
            Some(Status::Stopped(sig, rip)) => {
                println!("Child stopped (signal {})", sig);
                if let Some(line) = self.debug_data.get_line_from_addr(rip) {
                    println!("Stopped at {}", line);
                }
                return status;
            }
            None => { println!("continue fails!"); None }
            _ => { None }     // other cases
        }
    }

    fn parse_address(addr: &str) -> Option<usize> {
        let addr_without_0x = if addr.to_lowercase().starts_with("0x") {
            &addr[2..]
        } else {
            &addr
        };
        usize::from_str_radix(addr_without_0x, 16).ok()
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    if let Some(inferior) = Inferior::new(&self.target, &args, &self.break_points) {
                        if self.inferior.is_some() {
                            self.inferior.as_mut().unwrap().kill().ok();
                        }
                        // Create the inferior
                        self.inferior = Some(inferior);
                        let status = self.inferior.as_mut().unwrap().go().ok();
                        if let Some(Status::Exited(_)) = self.print_status(status) {
                            self.inferior = None;
                        }
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Continue => {
                    if self.inferior.is_none() {
                        println!("The program is not being run.");
                        continue;
                    }
                    let status = self.inferior.as_mut().unwrap().go().ok();
                    if let Some(Status::Exited(_)) = self.print_status(status) {
                        self.inferior = None;
                    }
                },
                DebuggerCommand::Backtrace => {
                    if self.inferior.is_none() {
                        println!("The program is not being run.");
                        continue;
                    }
                    self.inferior.as_ref().unwrap().print_backtrace(&self.debug_data).expect("");
                },
                DebuggerCommand::Breakpoint(target) => {
                    if target.starts_with('*') {
                        if let Some(addr) = Debugger::parse_address(&target[1..]) {
                            self.break_points.push(addr);
                            println!("Set breakpoint {} at {:#x}", self.break_points.len()-1, addr);
                        }
                        else { println!("Invalid address."); }
                    }
                    else if let Ok(line_no) = target.parse::<usize>() {
                        if let Some(addr) = self.debug_data.get_addr_for_line(None, line_no) {
                            self.break_points.push(addr);
                            println!("Set breakpoint {} at {:#x}", self.break_points.len()-1, addr);
                        }
                    }
                    else if let Some(function) = self.debug_data.find_function(target) {
                        if let Some(addr) = self.debug_data.get_addr_for_function(None, &function) {
                            self.break_points.push(addr);
                            println!("Set breakpoint {} at {:#x}", self.break_points.len()-1, addr);
                        }
                    }
                    else { println!("Invalid breakpoint target."); }
                    if self.inferior.is_some() {
                        self.inferior.as_mut().unwrap().install_breakpoints(&self.break_points);
                    }
                }
                DebuggerCommand::Quit => {
                    if self.inferior.is_some() {
                        self.inferior.as_mut().unwrap().kill().ok();
                    }
                    return;
                }
            }
        }
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().len() == 0 {
                        continue;
                    }
                    let _ = self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}
