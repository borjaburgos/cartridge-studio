use cartridge_core::{
    service::{self, Request},
    storage::Cancel,
    Error,
};
use serde_json::{json, Value};
use std::io::{self, BufRead, Read, Write};
fn emit(v: Value) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}
pub fn run() {
    if std::env::args().any(|s| s == "--version") {
        println!("Cartridge worker {} (Rust)", cartridge_core::VERSION);
        return;
    }
    let cancel = Cancel::default();
    let signal = cancel.clone();
    if let Err(e) =
        ctrlc::set_handler(move || signal.0.store(true, std::sync::atomic::Ordering::Relaxed))
    {
        emit(
            json!({"event":"error","error":"SIGNAL_HANDLER_FAILED","message":"Safe cancellation could not be initialized.","action":"Restart the application before accessing a cartridge.","details":{"reason":e.to_string()}}),
        );
        std::process::exit(1);
    }
    let mut root = cartridge_core::storage::data_root();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut line = Vec::new();
        io::stdin()
            .lock()
            .take(1024 * 1024 + 1)
            .read_until(b'\n', &mut line)?;
        if line.len() > 1024 * 1024 {
            return Err(Error::new(
                "INVALID_REQUEST",
                "Operation request is too large.",
                "Restart the application and retry.",
            ));
        }
        let mut request: Request = serde_json::from_slice(&line)?;
        root = request.root();
        request.prepare();
        if let Some(path) = &request.directory {
            emit(json!({"event":"directory","path":path}));
        }
        service::run(&request, cancel, &mut |message| {
            emit(json!({"event":"progress","message":message}))
        })
    }));
    let result=result.unwrap_or_else(|_|Err(Error::new("INTERNAL_ERROR","The operation stopped because of an unexpected software error.","Retain the operation folder and diagnostic log. If erase/write had begun, restore the saved source after resolving the error.").exit(1)));
    match result {
        Ok(r) => emit(json!({"event":"result","result":r})),
        Err(mut e) => {
            cartridge_core::operations::diagnostic(&mut e, &root);
            let code = e.exit_code;
            let mut v = json!(e);
            v["event"] = json!("error");
            emit(v);
            std::process::exit(code);
        }
    }
}
