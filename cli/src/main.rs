use clap::{crate_version, App, Arg};
use solana_rbpf::{
    aligned_memory::AlignedMemory,
    assembler::assemble,
    ebpf,
    elf::Executable,
    memory_region::{MemoryMapping, MemoryRegion},
    static_analysis::Analysis,
    verifier::{RequisiteVerifier, TautologyVerifier},
    vm::{BuiltInProgram, Config, DynamicAnalysis, EbpfVm, TestContextObject},
};
use std::{fs::File, io::Read, path::Path, sync::Arc};

fn main() {
    let matches = App::new("Solana RBPF CLI")
        .version(crate_version!())
        .author("Solana Maintainers <maintainers@solana.foundation>")
        .about("CLI to test and analyze eBPF programs")
        .arg(
            Arg::new("assembler")
                .about("Assemble and load eBPF executable")
                .short('a')
                .long("asm")
                .value_name("FILE")
                .takes_value(true)
                .required_unless_present("elf"),
        )
        .arg(
            Arg::new("elf")
                .about("Load ELF as eBPF executable")
                .short('e')
                .long("elf")
                .value_name("FILE")
                .takes_value(true)
                .required_unless_present("assembler"),
        )
        .arg(
            Arg::new("input")
                .about("Input for the program to run on")
                .short('i')
                .long("input")
                .value_name("FILE / BYTES")
                .takes_value(true)
                .default_value("0"),
        )
        .arg(
            Arg::new("memory")
                .about("Heap memory for the program to run on")
                .short('m')
                .long("mem")
                .value_name("BYTES")
                .takes_value(true)
                .default_value("0"),
        )
        .arg(
            Arg::new("use")
                .about("Method of execution to use")
                .short('u')
                .long("use")
                .takes_value(true)
                .possible_values(&["cfg", "debugger", "disassembler", "interpreter", "jit"])
                .required(true),
        )
        .arg(
            Arg::new("instruction limit")
                .about("Limit the number of instructions to execute")
                .short('l')
                .long("lim")
                .takes_value(true)
                .value_name("COUNT")
                .default_value(&i64::MAX.to_string()),
        )
        .arg(
            Arg::new("trace")
                .about("Display trace using tracing instrumentation")
                .short('t')
                .long("trace"),
        )
        .arg(
            Arg::new("port")
                .about("Port to use for the connection with a remote debugger")
                .long("port")
                .takes_value(true)
                .value_name("PORT")
                .default_value("9001"),
        )
        .arg(
            Arg::new("profile")
                .about("Display profile using tracing instrumentation")
                .short('p')
                .long("prof"),
        )
        .get_matches();

    let loader = Arc::new(BuiltInProgram::new_loader(Config {
        enable_instruction_tracing: matches.is_present("trace") || matches.is_present("profile"),
        enable_symbol_and_section_labels: true,
        ..Config::default()
    }));
    let executable = match matches.value_of("assembler") {
        Some(asm_file_name) => {
            let mut file = File::open(Path::new(asm_file_name)).unwrap();
            let mut source = Vec::new();
            file.read_to_end(&mut source).unwrap();
            assemble::<TestContextObject>(std::str::from_utf8(source.as_slice()).unwrap(), loader)
        }
        None => {
            let mut file = File::open(Path::new(matches.value_of("elf").unwrap())).unwrap();
            let mut elf = Vec::new();
            file.read_to_end(&mut elf).unwrap();
            Executable::<TautologyVerifier, TestContextObject>::from_elf(&elf, loader)
                .map_err(|err| format!("Executable constructor failed: {err:?}"))
        }
    }
    .unwrap();

    #[allow(unused_mut)]
    let verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();

    let mut mem = match matches.value_of("input").unwrap().parse::<usize>() {
        Ok(allocate) => vec![0u8; allocate],
        Err(_) => {
            let mut file = File::open(Path::new(matches.value_of("input").unwrap())).unwrap();
            let mut memory = Vec::new();
            file.read_to_end(&mut memory).unwrap();
            memory
        }
    };
    #[cfg(all(feature = "jit", not(target_os = "windows"), target_arch = "x86_64"))]
    if matches.value_of("use") == Some("jit") {
        verified_executable.jit_compile().unwrap();
    }
    let mut context_object = TestContextObject::new(
        matches
            .value_of("instruction limit")
            .unwrap()
            .parse::<u64>()
            .unwrap(),
    );
    let config = verified_executable.get_config();
    let mut stack = AlignedMemory::<{ ebpf::HOST_ALIGN }>::zero_filled(config.stack_size());
    let stack_len = stack.len();
    let mut heap = AlignedMemory::<{ ebpf::HOST_ALIGN }>::zero_filled(
        matches
            .value_of("memory")
            .unwrap()
            .parse::<usize>()
            .unwrap(),
    );
    let regions: Vec<MemoryRegion> = vec![
        verified_executable.get_ro_region(),
        MemoryRegion::new_writable_gapped(
            stack.as_slice_mut(),
            ebpf::MM_STACK_START,
            if !config.dynamic_stack_frames && config.enable_stack_frame_gaps {
                config.stack_frame_size as u64
            } else {
                0
            },
        ),
        MemoryRegion::new_writable(heap.as_slice_mut(), ebpf::MM_HEAP_START),
        MemoryRegion::new_writable(&mut mem, ebpf::MM_INPUT_START),
    ];

    let memory_mapping = MemoryMapping::new(regions, config).unwrap();

    let mut vm = EbpfVm::new(
        &verified_executable,
        &mut context_object,
        memory_mapping,
        stack_len,
    );

    let analysis = if matches.value_of("use") == Some("cfg")
        || matches.value_of("use") == Some("disassembler")
        || matches.is_present("trace")
        || matches.is_present("profile")
    {
        Some(Analysis::from_executable(&verified_executable).unwrap())
    } else {
        None
    };
    match matches.value_of("use") {
        Some("cfg") => {
            let mut file = File::create("cfg.dot").unwrap();
            analysis
                .as_ref()
                .unwrap()
                .visualize_graphically(&mut file, None)
                .unwrap();
            return;
        }
        Some("disassembler") => {
            let stdout = std::io::stdout();
            analysis
                .as_ref()
                .unwrap()
                .disassemble(&mut stdout.lock())
                .unwrap();
            return;
        }
        _ => {}
    }

    if matches.value_of("use").unwrap() == "debugger" {
        vm.debug_port = Some(matches.value_of("port").unwrap().parse::<u16>().unwrap());
    }
    let (instruction_count, result) = vm.execute_program(matches.value_of("use").unwrap() != "jit");
    println!("Result: {result:?}");
    println!("Instruction Count: {instruction_count}");
    if matches.is_present("trace") {
        println!("Trace:\n");
        let stdout = std::io::stdout();
        analysis
            .as_ref()
            .unwrap()
            .disassemble_trace_log(&mut stdout.lock(), &vm.env.context_object_pointer.trace_log)
            .unwrap();
    }
    if matches.is_present("profile") {
        let dynamic_analysis = DynamicAnalysis::new(
            &vm.env.context_object_pointer.trace_log,
            analysis.as_ref().unwrap(),
        );
        let mut file = File::create("profile.dot").unwrap();
        analysis
            .as_ref()
            .unwrap()
            .visualize_graphically(&mut file, Some(&dynamic_analysis))
            .unwrap();
    }
}
