use super::defs::*;
use super::exceptions;
use super::symbols;

use libc::{c_char, c_void};
use std::ffi::CString;
use std::mem;

fn arr_to_raw(arr: &[&str]) -> *const *const c_char {
    let vec: Vec<_> = arr
        .iter()
        .map(|s| CString::new(*s).unwrap().into_raw())
        .collect();
    let ptr = vec.as_ptr();

    mem::forget(vec);

    ptr as *const *const c_char
}

fn init_symbol_fn(
    invoke_fn: *const c_void,
    apply_to_fn: *const c_void,
    name: &str,
    arglist: &[&str],
    vararg: bool,
) {
    let sym = symbols::get_or_intern_symbol(name.to_string());

    let func = Function {
        ty: FunctionType::Function,
        name: CString::new(name).unwrap().into_raw(),
        arglist: arr_to_raw(arglist),
        arg_count: (arglist.len() as u64),
        is_macro: false,
        invoke_f_ptr: invoke_fn,
        apply_to_f_ptr: apply_to_fn,
        has_restarg: vararg,
    };

    let func = Box::into_raw(Box::new(func));

    unsafe { (*sym).function = func };
}

unsafe extern "C" fn native_add_invoke(_: *const Function, n: u64, args: ...) -> Object {
    let args = va_list_to_obj_array(n, args);
    let mut sum = 0;

    for i in 0..n {
        sum += (*args.offset(i as isize)).unpack_int();
    }

    Object::from_int(sum)
}

unsafe extern "C" fn native_add_apply(_: *const Function, args: List) -> Object {
    let mut sum = 0;
    let args_count = args.len;
    let mut cur_args = args;

    for _ in 0..args_count {
        sum += cur_args.first().unpack_int();
        cur_args = cur_args.rest();
    }

    Object::from_int(sum)
}

unsafe extern "C" fn native_sub_invoke(_: *const Function, n: u64, x: Object, args: ...) -> Object {
    let args = va_list_to_obj_array(n, args);
    let mut result = x.unpack_int();

    for i in 0..n {
        result -= (*args.offset(i as isize)).unpack_int();
    }

    Object::from_int(result)
}

unsafe extern "C" fn native_sub_apply(_: *const Function, args: List) -> Object {
    let mut result = args.first().unpack_int();

    let mut cur_args = args.rest();
    let args_len = cur_args.len;

    for _ in 0..args_len {
        result -= cur_args.first().unpack_int();
        cur_args = cur_args.rest();
    }

    Object::from_int(result)
}

extern "C" fn native_equal_invoke(_: *const Function, x: Object, y: Object) -> Object {
    if x == y {
        x
    } else {
        Object::nil()
    }
}

unsafe extern "C" fn native_equal_apply(f: *const Function, args: List) -> Object {
    native_equal_invoke(f, args.first(), args.rest().first())
}

extern "C" fn native_set_fn_invoke(_: *const Function, sym: Object, func: Object) -> Object {
    let sym = sym.unpack_symbol();
    let func = func.unpack_function();

    unsafe { (*sym).function = func };

    Object::nil()
}

unsafe extern "C" fn native_set_fn_apply(f: *const Function, args: List) -> Object {
    native_set_fn_invoke(f, args.first(), args.rest().first())
}

extern "C" fn native_cons_invoke(_: *const Function, x: Object, list: Object) -> Object {
    let list = list.unpack_list();
    let len = unsafe { (*list).len };

    let node = Node {
        val: Box::into_raw(Box::new(x)),
        next: list,
    };

    let new_list = List {
        node: Box::into_raw(Box::new(node)),
        len: len + 1,
    };

    Object::from_list(Box::into_raw(Box::new(new_list)))
}

unsafe extern "C" fn native_cons_apply(f: *const Function, args: List) -> Object {
    native_cons_invoke(f, args.first(), args.rest().first())
}

extern "C" fn native_rest_invoke(_: *const Function, list: Object) -> Object {
    let list = list.unpack_list();
    let len = unsafe { (*list).len };

    if len == 0 {
        Object::nil()
    } else {
        let rest = unsafe { (*(*list).node).next };
        Object::from_list(rest)
    }
}

unsafe extern "C" fn native_rest_apply(f: *const Function, args: List) -> Object {
    native_rest_invoke(f, args.first())
}

extern "C" fn native_first_invoke(_: *const Function, list: Object) -> Object {
    let list = list.unpack_list();
    let len = unsafe { (*list).len };

    if len == 0 {
        panic!("cannot do first on empty list");
    } else {
        unsafe { (*(*(*list).node).val).clone() }
    }
}

unsafe extern "C" fn native_first_apply(f: *const Function, args: List) -> Object {
    native_first_invoke(f, args.first())
}

unsafe fn apply_to_list(f: *const Function, args: List) -> Object {
    if !unlisp_rt_check_arity(f, args.len) {
        exceptions::raise_arity_error((*f).name, (*f).arg_count, args.len);
    }

    let apply_fn: unsafe extern "C" fn(*const Function, List) -> Object =
        mem::transmute((*f).apply_to_f_ptr);
    apply_fn(f, args)
}

unsafe extern "C" fn native_apply_invoke(
    _: *const Function,
    n: u64,
    f: Object,
    args: ...
) -> Object {
    let f = f.unpack_function();
    let args_arr = va_list_to_obj_array(n, args);
    let last_arg = (*args_arr.offset((n as isize) - 1)).unpack_list();
    let args_list = obj_array_to_list(n - 1, args_arr, Some(last_arg));

    apply_to_list(f, (*args_list).clone())
}

unsafe extern "C" fn native_apply_apply(_: *const Function, args: List) -> Object {
    apply_to_list(args.first().unpack_function(), args.rest())
}

unsafe extern "C" fn native_symbol_fn_invoke(_: *const Function, sym: Object) -> Object {
    let sym = sym.unpack_symbol();
    let f = (*sym).function;
    Object::from_function(f)
}

unsafe extern "C" fn native_symbol_fn_apply(f: *const Function, args: List) -> Object {
    native_symbol_fn_invoke(f, args.first())
}

unsafe extern "C" fn native_set_macro_invoke(_: *const Function, f: Object) -> Object {
    let f = f.unpack_function();

    (*f).is_macro = true;

    Object::nil()
}

unsafe extern "C" fn native_set_macro_apply(f: *const Function, args: List) -> Object {
    native_set_macro_invoke(f, args.first())
}

pub fn init() {
    init_symbol_fn(
        native_add_invoke as *const c_void,
        native_add_apply as *const c_void,
        "+",
        &[],
        true,
    );

    init_symbol_fn(
        native_sub_invoke as *const c_void,
        native_sub_apply as *const c_void,
        "-",
        &["x"],
        true,
    );

    init_symbol_fn(
        native_equal_invoke as *const c_void,
        native_equal_apply as *const c_void,
        "equal",
        &["x", "y"],
        false,
    );

    init_symbol_fn(
        native_set_fn_invoke as *const c_void,
        native_set_fn_apply as *const c_void,
        "set-fn",
        &["sym", "func"],
        false,
    );

    init_symbol_fn(
        native_symbol_fn_invoke as *const c_void,
        native_symbol_fn_apply as *const c_void,
        "symbol-function",
        &["sym"],
        false,
    );

    init_symbol_fn(
        native_cons_invoke as *const c_void,
        native_cons_apply as *const c_void,
        "cons",
        &["x", "list"],
        false,
    );
    init_symbol_fn(
        native_rest_invoke as *const c_void,
        native_rest_apply as *const c_void,
        "rest",
        &["list"],
        false,
    );
    init_symbol_fn(
        native_first_invoke as *const c_void,
        native_first_apply as *const c_void,
        "first",
        &["list"],
        false,
    );

    init_symbol_fn(
        native_apply_invoke as *const c_void,
        native_apply_apply as *const c_void,
        "apply",
        &["f"],
        true,
    );

    init_symbol_fn(
        native_set_macro_invoke as *const c_void,
        native_set_macro_apply as *const c_void,
        "set-macro",
        &["f"],
        false,
    );
}
