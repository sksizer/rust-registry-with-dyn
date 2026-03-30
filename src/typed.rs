use std::any::Any;
use std::ascii::escape_default;
use std::panic;
use log::trace;

/// This is the module definition the system interacts with where the types have been 'erased'.
/// It has the general shape. Concrete modules can be implemented but most have inputs and output
/// wrapped to the more general &(dyn Any + Send) type (that is the type erasure part)
pub struct DynModule {
    pub id: &'static str,
    pub validate_config: fn(config: &(dyn Any + Send)) -> bool,
    pub validate_input: fn(config: &(dyn Any + Send)) -> bool,
    pub execute: fn(config: &(dyn Any + Send), input: &(dyn Any + Send)) -> Box<dyn Any + Send>,
}

// CONCRETE Module shape for implementors
/// CT - Config Type
/// IT - Input Type
/// OT - Output Type
pub struct ModuleDef<CT, IT, OT> {
    pub id: &'static str,
    pub validate_config: fn(config: &CT) -> bool,
    pub validate_input: fn(config: &IT) -> bool,
    pub execute: fn(config: &CT, input: &IT) -> OT,
}


// Macro to create the specific type erasure module around the concrete implementation
macro_rules! dyn_module {
    ($static_def:path, config: $CT:ty, input: $IT:ty, output: $OT:ty) => {
        &DynModule {
            id: $static_def.id,

            validate_config: |cfg: &(dyn Any + Send)| {
                let typed = cfg.downcast_ref::<$CT>()
                    .expect(concat!("wrong config type for ", stringify!($static_def)));
                ($static_def.validate_config)(typed)
            },

            validate_input: |input: &(dyn Any + Send)| {
                let typed = input.downcast_ref::<$IT>()
                    .expect(concat!("wrong input type for ", stringify!($static_def)));
                ($static_def.validate_input)(typed)
            },

            execute: |cfg: &(dyn Any + Send), input: &(dyn Any + Send)| {
                let typed_cfg = cfg.downcast_ref::<$CT>()
                    .expect(concat!("wrong config type for ", stringify!($static_def)));
                let typed_input = input.downcast_ref::<$IT>()
                    .expect(concat!("wrong input type for ", stringify!($static_def)));
                let result: $OT = ($static_def.execute)(typed_cfg, typed_input);
                Box::new(result) as Box<dyn Any + Send>
            },
        }
    };
}

// MODULE EXAMPLES
pub static ECHO : ModuleDef<String, String, String> = ModuleDef {
    id: "ECHO",
    validate_config: |config   | { true },
    validate_input: |input| { true },
    execute: |config, input| { return format!("config: {}, input: {}", input, config) },
};

// NOTE WE USE String instead of the trait ToString
pub static ECHO_2 : ModuleDef<String, String, String> = ModuleDef {
    id: "ECHO_2",
    validate_config: |config| { true },
    validate_input: |input| { true },
    execute: |config, input| { return format!("config: {}, input: {}", input, config) },
};




pub static REGISTRY : &[&DynModule] = &[
    // This is an example of explicitly wrapping a concrete module (ECHO) with the dynamic module
    // expectation of the system
    &DynModule {
        id: ECHO.id,
        validate_config: |config:&(dyn Any + Send)| {
            let typed = config.downcast_ref::<String>().expect("wrong type");
            (ECHO.validate_config)(typed)
        },
        validate_input: |input:&(dyn Any + Send)| {
            let typed = input.downcast_ref::<String>().expect("wrong type");
            (ECHO.validate_input)(typed)
        },
        execute: |config:&(dyn Any + Send), input:&(dyn Any + Send)| {
            let typed_config= config.downcast_ref::<String>().expect("wrong type");
            let typed_input= input.downcast_ref::<String>().expect("wrong type");
            Box::new((ECHO.execute)(typed_config, typed_input))
        },
    },
    // automatic wrapping via macro example
    dyn_module!(ECHO_2, config: String, input: String, output: String)
];

fn get_module(id:&str) -> Option<&'static DynModule> {
    // note: copied does one level of dereferencing of the found value
    REGISTRY.iter().find(|r| r.id == id).copied()
}


fn execute_module(dyn_module: &DynModule, cfg:&(dyn Any + Send), input: &(dyn Any + Send)) -> Result<Box<dyn Any + Send>, String> {
    // Could add panic catch here as well
    if !(dyn_module.validate_config)(cfg) {
        return Err(format!("{} config did not validate", dyn_module.id));
    }
    if !(dyn_module.validate_input)(input) {
        return Err(format!("{} input config did not validate", dyn_module.id.to_string()));
    }

    // Somewhat similar to try catch, but unlike most other languages happy path is virtually zero
    // cost
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        (dyn_module.execute)(cfg, input)
    }))
        .map_err(|panic_val| {
            // Try to extract a message from the panic
            let msg = panic_val
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic_val.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            format!("{} panicked: {}", dyn_module.id, msg)
        });
    result
}

fn boxed<T: Any + Send>(val: T) -> Box<dyn Any + Send> {
    Box::new(val)
}

// Convenience function that looks up module by &str and takes in refs to the dyn values
fn exec(module:&str, config: &(dyn Any + Send), input: &(dyn Any + Send)) -> Result<Box<dyn Any + Send>, String> {
    // let mod_id = module.into();
    let dyn_module = get_module(module)
        .ok_or(format!("{} module not found", module))?;
    execute_module(dyn_module, config, input)
}

pub fn run_example() {
    trace!("Running DynModule");
    let module = get_module("test");
    // We are erasing types from the module interface so we can separate execution from concrete
    // typed implementation. Because of this the interface for the module functions are (dyn Any + send)
    // This also means that we do not know the size of values at compile time, so we have to box
    // (which is an **owning pointer** to the heap allocation) the values we are going to use for testing.
    let config = boxed("test".to_string());
    let input = boxed("test".to_string());

    // Memory allocations have to be owned, hence the boxed values above but to simplify things we
    // only passing around (dyn any + send) so we can use &* which does the following:
    // * dereferences the box value (box is a pointer to heap data)
    // & borrows that value
    // So in other words we get a borrow of the box interior value to actually pass to the module
    // execution chain
    let result = exec(
        "ECHO",
        &*config,
        &*input);

    let boxed_ = result.unwrap();
    let r_s = boxed_.downcast_ref::<String>().cloned();

    println!("Result Echo: {:?}", r_s);


    let result = exec(
        "ECHO_2",
        &*config,
        &*input
    );
    let boxed_ = result.unwrap();
    let r_s = boxed_.downcast_ref::<String>().cloned();

    println!("Result Echo 2: {:?}", r_s);
}