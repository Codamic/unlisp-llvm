use std::ffi::CStr;
use std::mem;
use std::ptr;

use super::defs::Object;

use inkwell::context::Context;
use inkwell::execution_engine::JitFunction;
use inkwell::module::{Linkage, Module};
use inkwell::AddressSpace;
use libc::c_char;

const JMP_BUF_WIDTH: usize = mem::size_of::<u32>() * 40;

#[export_name = "glob_jmp_buf"]
#[no_mangle]
#[used]
static mut GLOB_JMP_BUF: [i8; JMP_BUF_WIDTH] = [0; JMP_BUF_WIDTH];

#[export_name = "err_msg_ptr"]
#[no_mangle]
#[used]
static mut ERR_MSG_PTR: *mut i8 = ptr::null_mut();

fn glob_jmp_buf_ptr() -> *mut i8 {
    unsafe { &mut GLOB_JMP_BUF[0] as *mut i8 }
}

extern "C" {
    fn setjmp(buf: *mut i8) -> i32;
    fn longjmp(buf: *const i8);
}

pub unsafe fn run_with_global_ex_handler(
    f: JitFunction<unsafe extern "C" fn() -> Object>,
) -> Result<Object, String> {
    if setjmp(glob_jmp_buf_ptr()) == 0 {
        Ok(f.call())
    } else {
        Err((*(ERR_MSG_PTR as *mut String)).clone())
    }
}

fn set_msg_and_jump(msg: String) {
    unsafe {
        ERR_MSG_PTR = Box::into_raw(Box::new(msg)) as *mut i8;
        longjmp(glob_jmp_buf_ptr());
    }
}

pub fn gen_defs(ctx: &Context, module: &Module) {
    // sjlj_gen_def(ctx, module);
    raise_arity_error_gen_def(ctx, module);
    raise_undef_fn_error_gen_def(ctx, module);
}

// fn sjlj_gen_def(ctx: &Context, module: &Module) {
//     let i32_ty = ctx.i32_type();

//     let buf_ty = ctx.opaque_struct_type("setjmp_buf");
//     let int32_arr_ty = i32_ty.array_type(40);
//     buf_ty.set_body(&[int32_arr_ty.into()], false);

//     // has to be looked up through module, to avoid renaming
//     let buf_ptr_ty = module
//         .get_type("setjmp_buf")
//         .unwrap()
//         .as_struct_type()
//         .ptr_type(AddressSpace::Generic);
//     let void_ty = ctx.void_type();
//     let sj_fn_ty = i32_ty.fn_type(&[buf_ptr_ty.into()], false);
//     let lj_fn_ty = void_ty.fn_type(&[buf_ptr_ty.into(), i32_ty.into()], false);

//     module.add_function("setjmp", sj_fn_ty, Some(Linkage::External));
//     module.add_function("longjmp", lj_fn_ty, Some(Linkage::External));
// }

#[no_mangle]
pub extern "C" fn raise_arity_error(name: *const c_char, _expected: u64, actual: u64) {
    let name_str = if name != ptr::null() {
        unsafe { CStr::from_ptr(name).to_str().unwrap() }
    } else {
        "lambda"
    };

    let msg = format!(
        "wrong number of arguments ({}) passed to {}",
        actual, name_str
    );

    set_msg_and_jump(msg);
}

#[used]
static RAISE_ARITY_ERROR: extern "C" fn(name: *const c_char, expected: u64, actual: u64) =
    raise_arity_error;

fn raise_arity_error_gen_def(ctx: &Context, module: &Module) {
    let void_ty = ctx.void_type();
    let fn_ty = void_ty.fn_type(
        &[
            ctx.i8_type().ptr_type(AddressSpace::Generic).into(),
            ctx.i64_type().into(),
            ctx.i64_type().into(),
        ],
        false,
    );

    module.add_function("raise_arity_error", fn_ty, Some(Linkage::External));
}

#[no_mangle]
pub extern "C" fn raise_undef_fn_error(name: *const c_char) {
    let name_str = unsafe { CStr::from_ptr(name).to_str().unwrap() };

    let msg = format!("undefined function {}", name_str);

    set_msg_and_jump(msg);
}

#[used]
static RAISE_UNDEF_FN_ERROR: extern "C" fn(name: *const c_char) = raise_undef_fn_error;

fn raise_undef_fn_error_gen_def(ctx: &Context, module: &Module) {
    let void_ty = ctx.void_type();
    let fn_ty = void_ty.fn_type(
        &[ctx.i8_type().ptr_type(AddressSpace::Generic).into()],
        false,
    );

    module.add_function("raise_undef_fn_error", fn_ty, Some(Linkage::External));
}

pub fn raise_cast_error(from: String, to: String) {
    let msg = format!("cannot cast {} to {}", from, to);

    set_msg_and_jump(msg);
}