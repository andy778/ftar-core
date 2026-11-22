//! MCP server: one JSON-RPC line per input, against a session confined to an
//! empty scratch directory so tools cannot touch the rest of the file system.
#![no_main]

use std::sync::OnceLock;

use ftar_automation::Session;
use ftar_mcp::Server;
use libfuzzer_sys::fuzz_target;

fn root() -> &'static std::path::Path {
    static ROOT: OnceLock<std::path::PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("ftar-fuzz-mcp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create fuzz scratch dir");
        dir
    })
}

fuzz_target!(|data: &[u8]| {
    let Ok(line) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(session) = Session::with_root(root()) else {
        return;
    };
    let mut server = Server::new(session);
    let _ = server.handle_line(line);
});
