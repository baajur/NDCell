//! Miscellaneous functions.

use std::sync::Arc;

use super::FuncConstructor;
use crate::ast::{AssignableFunction, FuncCallInfo, FuncCallInfoMut, Function};
use crate::compiler::{Compiler, Value};
use crate::errors::*;
use crate::types::FnSignature;
use crate::{ConstValue, Type};
use LangErrorMsg::Unimplemented;

/// Built-in function that returns a fixed variable.
#[derive(Debug)]
pub struct GetVar {
    /// Name of variable to get.
    var_name: Arc<String>,
    /// Type of this variable.
    var_type: Type,
}
impl GetVar {
    /// Returns a constructor for a new GetVar instance with the given variable
    /// name.
    pub fn with_name(var_name: Arc<String>) -> FuncConstructor {
        Box::new(|info| {
            let var_type;
            if &**var_name == crate::THROWAWAY_VARIABLE {
                var_type = Type::Void
            } else {
                var_type = info.userfunc.try_get_var(info.span, &var_name)?.clone()
            };
            Ok(Box::new(Self { var_name, var_type }))
        })
    }
}
impl Function for GetVar {
    fn return_type(&self, info: &mut FuncCallInfoMut) -> LangResult<Type> {
        if info.arg_types().len() != 0 {
            internal_error!("Arguments passed to variable access function");
        }
        // We checked argument types in the constructor, so we don't need to
        // worry about doing that here.
        Ok(self.var_type.clone())
    }
    fn compile(&self, compiler: &mut Compiler, _info: FuncCallInfo) -> LangResult<Value> {
        if self.var_type == Type::Void {
            return Ok(Value::Void);
        }
        compiler.build_var_load(&*self.var_name)
    }
    fn as_assignable<'a>(&self, _info: FuncCallInfo) -> Option<&dyn AssignableFunction> {
        Some(self)
    }
}
impl AssignableFunction for GetVar {
    fn compile_assign(
        &self,
        compiler: &mut Compiler,
        value: Value,
        _info: FuncCallInfo,
    ) -> LangResult<()> {
        if self.var_type == Type::Void {
            return Ok(());
        }
        compiler.build_var_store(&*self.var_name, &value)
    }
}

/// Built-in function that calls a user-written helper function.
#[derive(Debug)]
pub struct CallUserFn {
    /// Name of user function to call.
    func_name: Arc<String>,
    /// Signature of the function.
    signature: FnSignature,
}
impl CallUserFn {
    /// Returns a constructor for a new CallUserFn instance that calls the
    /// function with the given name.
    pub fn with_name(func_name: Arc<String>) -> FuncConstructor {
        Box::new(|info| {
            let signature = info
                .userfunc
                .rule_meta()
                .helper_function_signatures
                .get(&**func_name)
                .ok_or_else(|| internal_error_value!("Cannot find user function"))?
                .clone();
            Ok(Box::new(Self {
                func_name,
                signature,
            }))
        })
    }
}
impl Function for CallUserFn {
    fn return_type(&self, info: &mut FuncCallInfoMut) -> LangResult<Type> {
        if self.signature.matches(&info.arg_types()) {
            Ok(self.signature.ret.clone())
        } else {
            Err(info.invalid_args_err())
        }
    }
    fn compile(&self, _compiler: &mut Compiler, info: FuncCallInfo) -> LangResult<Value> {
        Err(Unimplemented.with_span(info.span))
    }
}

/// Built-in function that returns a default value for any type.
#[derive(Debug)]
pub struct New {
    /// Type to return.
    ty: Type,
}
impl New {
    pub fn with_type(ty: Type) -> FuncConstructor {
        Box::new(|_info| {
            if !ty.has_runtime_representation() {
                internal_error!("Cannot call .new() on type without runtime representation");
            }
            Ok(Box::new(Self { ty }))
        })
    }
}
impl Function for New {
    fn return_type(&self, info: &mut FuncCallInfoMut) -> LangResult<Type> {
        info.check_args_len(0)?;
        Ok(self.ty.clone())
    }
    fn compile(&self, compiler: &mut Compiler, _info: FuncCallInfo) -> LangResult<Value> {
        Ok(compiler.default_var_value(&self.ty)?)
    }
    fn const_eval(&self, _info: FuncCallInfo) -> LangResult<ConstValue> {
        ConstValue::default(&self.ty)
    }
}
