use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

fn should_skip_next(arg: &str) -> bool {
    matches!(arg, "-flavor" | "-L" | "-z")
}

fn ignore(arg: &str) -> bool {
    arg.starts_with("-plugin-opt")
        || matches!(
            arg,
            "--gc-sections"
                | "--as-needed"
                | "--eh-frame-hdr"
                | "--strip-debug"
                | "-Bstatic"
                | "-Bdynamic"
        )
}

fn run(cmd: &mut Command) {
    println!("Running: {:?}", cmd);
    match cmd.status() {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("Command failed with code: {}", status.code().unwrap_or(-1));
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("Error executing command: {error}");
            std::process::exit(1);
        }
    }
}

fn tool(root: &Path, parts: &[&str], name: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    path.extend(parts);
    path.push(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    });
    path
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut files = Vec::new();
    let mut output = None;

    while let Some(arg) = args.next() {
        if should_skip_next(&arg) {
            args.next();
        } else if ignore(&arg) {
        } else if arg == "-o" {
            output = args.next().map(PathBuf::from);
        } else {
            files.push(PathBuf::from(arg));
        }
    }

    let output = output.expect("linker output path was not provided");
    let output_stem = output
        .file_stem()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .unwrap_or("output");
    let program_name: String = output_stem
        .split('-')
        .next()
        .unwrap_or("output")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(8)
        .collect();
    let work = output.with_extension("ti83-link");
    std::fs::create_dir_all(&work).expect("failed to create linker work directory");

    let cedev = std::path::absolute("CEdev").expect("failed to locate CEdev");
    let clang = tool(&cedev, &["bin"], "ez80-clang");
    let llvm_link = tool(&cedev, &["bin"], "ez80-link");
    let cedev_obj = tool(&cedev, &["bin"], "cedev-obj");
    let assembler = tool(&cedev, &["binutils", "bin"], "z80-none-elf-as");
    let linker = tool(&cedev, &["binutils", "bin"], "z80-none-elf-ld");
    let strip = tool(&cedev, &["binutils", "bin"], "z80-none-elf-strip");
    let convbin = tool(&cedev, &["bin"], "convbin");

    let mut bitcode = Vec::new();
    for (index, file) in files.iter().enumerate() {
        if file.file_stem() == Some(OsStr::new("symbols"))
            || file.to_string_lossy().contains("libcompiler_builtins")
        {
            continue;
        }
        if file.extension() == Some(OsStr::new("o")) {
            bitcode.push(file.clone());
        } else if file.extension() == Some(OsStr::new("c")) {
            let object = work.join(format!("wrapper-{index}.bc"));
            run(Command::new(&clang).args([
                OsStr::new("-c"),
                OsStr::new("-emit-llvm"),
                OsStr::new("-Oz"),
                OsStr::new("-nostdinc"),
                OsStr::new("-isystem"),
                cedev.join("include").as_os_str(),
                OsStr::new("-D__TICE__=1"),
                OsStr::new("-fno-autolink"),
                OsStr::new("-fno-addrsig"),
                OsStr::new("-fno-threadsafe-statics"),
                OsStr::new("-Xclang"),
                OsStr::new("-fforce-mangle-main-argc-argv"),
                file.as_os_str(),
                OsStr::new("-o"),
                object.as_os_str(),
            ]));
            bitcode.push(object);
        } else {
            eprintln!("unknown linker input: {}", file.display());
            std::process::exit(1);
        }
    }

    let linked_bc = work.join("lto.bc");
    let mut command = Command::new(&llvm_link);
    command.args(&bitcode).arg("-o").arg(&linked_bc);
    run(&mut command);

    let assembly = work.join("lto.s");
    run(Command::new(&clang).args([
        OsStr::new("-S"),
        OsStr::new("-Oz"),
        OsStr::new("-fno-autolink"),
        OsStr::new("-fno-addrsig"),
        OsStr::new("-mllvm"),
        OsStr::new("-z80-gas-style"),
        linked_bc.as_os_str(),
        OsStr::new("-o"),
        assembly.as_os_str(),
    ]));

    let lto_object = work.join("lto.o");
    run(Command::new(&assembler)
        .arg("-march=ez80+full")
        .arg(&assembly)
        .arg("-o")
        .arg(&lto_object));

    let lib = cedev.join("lib");
    let libraries = [
        lib.join("libc/allocator_standard.a"),
        lib.join("crt/libcrt.a"),
        lib.join("ce/libce.a"),
        lib.join("softfloat/libsoftfloat.a"),
        lib.join("libc/libc_os.a"),
    ];
    let program_object = work.join("program.o");
    let mut command = Command::new(&linker);
    command
        .args([
            "-i",
            "-static",
            "--entry=__start",
            "--build-id=none",
            "--gc-sections",
            "--omagic",
        ])
        .arg("--defsym=__TICE__=1")
        .arg("--defsym=NDEBUG=1")
        .arg(&lto_object)
        .arg("--start-group")
        .args(&libraries)
        .arg("--end-group")
        .arg("-o")
        .arg(&program_object);
    run(&mut command);
    run(Command::new(&strip)
        .arg("--strip-unneeded")
        .arg(&program_object));

    let crt_header = work.join("crt.h");
    let mut command = Command::new(&cedev_obj);
    command
        .arg("--elf")
        .arg(&program_object)
        .arg("--output")
        .arg(&crt_header)
        .arg("--libs");
    for library in [
        "fatdrvce", "fileioc", "fontlibc", "graphx", "keypadc", "lcddrvce", "msddrvce", "srldrvce",
        "usbdrvce",
    ] {
        command.arg(lib.join(format!("libload/{library}.lib")));
    }
    run(&mut command);

    let crt_assembly = work.join("crt0.s");
    run(Command::new(&clang)
        .arg("-I")
        .arg(&work)
        .args(["-E", "-P"])
        .arg(lib.join("crt/crt0.S"))
        .arg("-o")
        .arg(&crt_assembly));
    let crt_object = work.join("crt0.o");
    run(Command::new(&assembler)
        .arg(format!("-I{}", work.display()))
        .arg("-march=ez80+full")
        .arg(&crt_assembly)
        .arg("-o")
        .arg(&crt_object));

    let executable = work.join("program.obj");
    let mut command = Command::new(&linker);
    command
        .args([
            "-static",
            "--entry=__start",
            "--no-warn-rwx-segments",
            "--gc-sections",
            "--omagic",
        ])
        .args([
            "--defsym=LOAD_ADDR=0xD1A87F",
            "--defsym=BSSHEAP_LOW=0xD052C6",
            "--defsym=BSSHEAP_HIGH=0xD13FD8",
            "--defsym=STACK_HIGH=0xD1A87E",
            "--defsym=__TICE__=1",
        ])
        .arg(format!(
            "-T{}",
            cedev.join("meta/linker_script.ld").display()
        ))
        .arg(format!("-Map={}", work.join("program.map").display()))
        .arg(&program_object)
        .arg(&crt_object)
        .arg("--start-group")
        .args(&libraries)
        .arg("--end-group")
        .arg("-o")
        .arg(&executable);
    run(&mut command);

    run(Command::new(&convbin)
        .args(["-k", "8xp", "-u", "-n", &program_name, "-j", "elf", "-i"])
        .arg(&executable)
        .arg("-o")
        .arg(&output));
}
