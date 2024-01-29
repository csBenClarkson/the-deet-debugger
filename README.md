# the-deet-debugger
The Deet Debugger implementation from Stanford CS110L Projects.

CS110L Assignment handouts are available [here](https://reberhardt.com/cs110l/spring-2020/).  
Includes starter code from [here](https://github.com/reberhardt7/cs110l-spr-2020-starter-code).  
Full development process can be found [here](https://github.com/csBenClarkson/cs110l-spr-2020).  

# Description
This is a C program debugger written in Rust, which implements following GDB-like functions:  
- Setting breakpoints at raw address, functions and line numbers.
- Continue from breakpoints.
- Print backtrace information.

# Usage
In the root directory, simply run  
```
cargo run <program>
```
or  
```
complied-rust-executable <program>
```
to enter the command-line interface of the debugger.  
Some sample C programs are [provided](https://github.com/reberhardt7/cs110l-spr-2020-starter-code) in `samples/` directory, run `make` to complie.  
  

The command-line interface support following commands: 
- start the tracee program
```
(deet) r | run
```
- set breakpoints at raw address, functions or line number.
```
(deet) b | break | breakpoint <*raw_addresses | function_name | line_number>
```
examples: 
```
(deet) b *0x12345678
(deet) break func1
(deet) breakpoint 10
```

- print backtrace information when tracee program stops.
```
(deet) bt | back | backtrace
```

- resume program execution.
```
(deet) c | cont | continue
```

- stop the tracee program and exit.
```
(deet) q | quit
```
