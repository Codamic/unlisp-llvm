use std::error::Error;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::process::Command;

use unlispc::codegen::context::CodegenContext;
use unlispc::reader;
use unlispc::repr;

use clap::{App, AppSettings, Arg, SubCommand};

fn read_and_parse<'a, T: Read>(
    reader: &mut reader::Reader<'a, T>,
) -> Result<Option<repr::HIR>, Box<dyn Error>> {
    let form = reader.read_form()?;
    Ok(form
        .as_ref()
        .map(repr::form_to_hir_with_transforms)
        .transpose()?)
}

pub fn eval_and_expand_file(
    ctx: &mut CodegenContext,
    path: &str,
    panic_on_err: bool,
) -> Result<Vec<repr::HIR>, Box<dyn Error>> {
    let mut file = fs::File::open(path).expect("stdlib file not found");

    let report_err = |msg| {
        if panic_on_err {
            panic!("[{}] {}", path, msg);
        } else {
            eprintln!("[{}] {}", path, msg);
        }
    };

    let mut expanded = vec![];

    let mut reader = reader::Reader::create(&mut file);
    loop {
        match read_and_parse(&mut reader) {
            Ok(Some(hir)) => match unsafe { ctx.eval_hirs(&[hir.clone()]) } {
                Ok(_) => expanded.push(hir),
                Err(e) => report_err(e.to_string()),
            },
            Ok(None) => break,
            Err(e) => report_err(e.to_string()),
        }
        ctx.reinitialize();
    }

    Ok(expanded)
}

pub fn eval_stdlib(ctx: &mut CodegenContext, path: Option<&str>) {
    if path.is_none() {
        return;
    }

    let path = path.unwrap();
    let _ = eval_and_expand_file(ctx, path, true);
}

fn repl(ctx: &mut CodegenContext, dump_compiled: bool) {
    let mut stdin = io::stdin();

    let prompt = || {
        print!(">>> ");
        io::stdout().flush().unwrap();
    };

    let mut reader = reader::Reader::create(&mut stdin);

    prompt();
    loop {
        match read_and_parse(&mut reader) {
            Ok(Some(hir)) => unsafe {
                match ctx.compile_hirs(&[hir]) {
                    Ok(compiled_fn) => {
                        if dump_compiled {
                            eprintln!("Expression compiled to LLVM IR:");
                            ctx.get_module().print_to_stderr();
                        }
                        match unlisp_rt::exceptions::run_with_global_ex_handler(|| {
                            compiled_fn.call()
                        }) {
                            Ok(obj) => println!("{}", obj),
                            Err(err) => eprintln!("{}", err),
                        }
                    }
                    Err(err) => eprintln!("{}", err),
                }
            },
            Ok(None) => break,
            Err(e) => eprintln!("{}", e),
        }
        ctx.reinitialize();
        prompt();
    }
}

fn launch_repl(stdlib_path: Option<&str>, dump_compiled: bool) {
    unlisp_rt::defs::unlisp_rt_init_runtime();
    let mut codegen_ctx = CodegenContext::new();
    eval_stdlib(&mut codegen_ctx, stdlib_path);
    repl(&mut codegen_ctx, dump_compiled)
}

fn exec_file(stdlib_path: Option<&str>, file: &str) -> bool {
    unlisp_rt::defs::unlisp_rt_init_runtime();
    let mut codegen_ctx = CodegenContext::new();

    eval_stdlib(&mut codegen_ctx, stdlib_path);
    eval_and_expand_file(&mut codegen_ctx, file, false).is_ok()
}

fn aot_file(stdlib_path: Option<&str>, rt_lib_path: &str, file: &str, out: &str) -> bool {
    unlisp_rt::defs::unlisp_rt_init_runtime();

    let mut expand_ctx = CodegenContext::new();
    let mut aot_ctx = CodegenContext::new();

    let mut expanded = vec![];

    println!("Compiling file: {}...", file);

    if let Some(stdlib) = stdlib_path {
        expanded.append(
            &mut eval_and_expand_file(&mut expand_ctx, stdlib, true)
                .expect("stdlib evaluation shouldn't return error"),
        );
    }

    let expanded_file = eval_and_expand_file(&mut expand_ctx, file, false);

    if expanded_file.is_err() {
        return false;
    }

    expanded.append(&mut expanded_file.unwrap());

    let object_file = format!("{}.o", out);

    if let Err(e) = aot_ctx.compile_hirs_to_file(&object_file, expanded.as_slice()) {
        eprintln!("{}", e);
        return false;
    }

    println!("Linking with runtime library: {}...", rt_lib_path);

    let mut cmd_args = vec![];

    #[cfg(target_os = "linux")]
    {
        cmd_args.push("-lpthread");
        cmd_args.push("-ldl");
    }

    cmd_args.push(object_file.as_str());
    cmd_args.push(rt_lib_path);
    cmd_args.push("-o");
    cmd_args.push(out);

    let linker_output = Command::new("clang")
        .args(cmd_args.as_slice())
        .output()
        .expect("failed to execute linker");

    if !linker_output.status.success() {
        eprintln!(
            "failed to create binary: \n {}",
            String::from_utf8_lossy(&linker_output.stderr)
        );
        return false;
    }

    true
}

fn main() {
    let app = App::new("unlisp")
        .version("0.1.0")
        .author("Oleh Palianytsia <oleh.palianytsia@protonmail.com>")
        .about("Compiler for a toy Lisp language")
        .setting(AppSettings::ArgRequiredElseHelp)
        .arg(Arg::with_name("stdlib-path")
             .long("stdlib-path")
             .value_name("FILE")
             .help("Sets path for stdlib file (default: ./stdlib.unl)")
             .conflicts_with("no-stdlib")
             .takes_value(true))
        .arg(Arg::with_name("no-stdlib")
             .long("no-stdlib")
             .conflicts_with("stdlib-path")
             .help("Don't precompile stdlib file"))
        .subcommand(SubCommand::with_name("repl")
                    .about("Launch Unlisp REPL")
                    .arg(Arg::with_name("dump-compiled")
                         .long("dump-compiled")
                         .short("d")
                         .help("Dump compiled IR to stderr")))
        .subcommand(SubCommand::with_name("eval")
                    .about("Eval a file")
                    .arg(Arg::with_name("file")
                         .short("f")
                         .long("file")
                         .value_name("FILE")
                         .takes_value(true)
                         .required(true)
                         .help("A file to eval")))
        .subcommand(SubCommand::with_name("compile")
                    .about("AOT compile a file")
                    .arg(Arg::with_name("file")
                         .short("f")
                         .long("file")
                         .value_name("FILE")
                         .takes_value(true)
                         .required(true)
                         .help("A file to compile"))
                    .arg(Arg::with_name("output")
                         .short("o")
                         .long("output")
                         .value_name("FILE")
                         .takes_value(true)
                         .help("An output binary file"))
                    .arg(Arg::with_name("runtime-lib")
                         .long("runtime-lib-path")
                         .value_name("FILE")
                         .takes_value(true)
                         .help("Path to Unlisp runtime library to link (default: ./unlisp_rt_staticlib/target/<debug/release>/libunlisp_rt.a)")));
    let matches = app.get_matches();

    let stdlib_path;

    if matches.is_present("no-stdlib") {
        stdlib_path = None;
    } else {
        stdlib_path = Some(matches.value_of("stdlib-path").unwrap_or("./stdlib.unl"));
    }

    match matches.subcommand_name() {
        Some("repl") => {
            launch_repl(
                stdlib_path,
                matches
                    .subcommand_matches("repl")
                    .unwrap()
                    .is_present("dump-compiled"),
            );
        }
        Some("eval") => {
            if !exec_file(
                stdlib_path,
                matches
                    .subcommand_matches("eval")
                    .unwrap()
                    .value_of("file")
                    .unwrap(),
            ) {
                std::process::exit(1);
            }
        }
        Some("compile") => {
            #[cfg(debug_assertions)]
            let default_rt_lib_path = "./unlisp_rt_staticlib/target/debug/libunlisp_rt.a";
            #[cfg(not(debug_assertions))]
            let default_rt_lib_path = "./unlisp_rt_staticlib/target/release/libunlisp_rt.a";

            let matches = matches.subcommand_matches("compile").unwrap();

            let runtime_lib_path = matches
                .value_of("runtime-lib")
                .unwrap_or(default_rt_lib_path);

            if !aot_file(
                stdlib_path,
                runtime_lib_path,
                matches.value_of("file").unwrap(),
                matches.value_of("output").unwrap_or("./a.out"),
            ) {
                std::process::exit(1);
            }
        }
        Some(cmd) => panic!("unknown command: {}", cmd),
        None => println!("{}", matches.usage()),
    }
}
